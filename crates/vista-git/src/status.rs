//! `git status` as a snapshot: per-file status, folder roll-up, ignored paths
//! and branch info.
//!
//! Repositories cloned inside the project (in an ignored or untracked folder,
//! like `repos/` holding other checkouts) get their own `git status`; their
//! changes join the snapshot, and each path takes its status, ignore rules and
//! branch from the repository it is actually in.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::cmd::{Git, GitError};

/// The status kinds vista colours, in IntelliJ's terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FileStatus {
    Conflicted,
    Added,
    Modified,
    Renamed,
    Deleted,
    Unversioned,
    Ignored,
}

impl FileStatus {
    /// Theme token for this status.
    pub fn token(self) -> &'static str {
        match self {
            FileStatus::Conflicted => "git.conflict",
            FileStatus::Added => "git.added",
            FileStatus::Modified => "git.modified",
            FileStatus::Renamed => "git.renamed",
            FileStatus::Deleted => "git.deleted",
            FileStatus::Unversioned => "git.unversioned",
            FileStatus::Ignored => "git.ignored",
        }
    }

    /// One-letter indicator (for looks with `git_indicator = "colour+letter"`).
    pub fn letter(self) -> char {
        match self {
            FileStatus::Conflicted => '!',
            FileStatus::Added => 'A',
            FileStatus::Modified => 'M',
            FileStatus::Renamed => 'R',
            FileStatus::Deleted => 'D',
            FileStatus::Unversioned => '?',
            FileStatus::Ignored => 'I',
        }
    }
}

/// One changed path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Absolute path.
    pub path: PathBuf,
    pub status: FileStatus,
    /// For renames and copies: the path in HEAD.
    pub orig_path: Option<PathBuf>,
    /// The index differs from HEAD.
    pub staged: bool,
    /// The working tree differs from the index.
    pub unstaged: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchInfo {
    /// Branch name; `None` when HEAD is detached.
    pub head: Option<String>,
    /// Commit id of HEAD; `None` before the first commit.
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

impl BranchInfo {
    /// `main`, or the short commit id when detached.
    pub fn label(&self) -> String {
        match (&self.head, &self.oid) {
            (Some(h), _) => h.clone(),
            (None, Some(oid)) => oid.chars().take(8).collect(),
            (None, None) => "(no commits)".into(),
        }
    }
}

/// The repository's status at one moment.
#[derive(Debug, Clone, Default)]
pub struct StatusSnapshot {
    pub toplevel: PathBuf,
    pub branch: BranchInfo,
    /// Changes of this repository and of the nested ones, by path.
    changes: Vec<Change>,
    by_path: HashMap<PathBuf, usize>,
    ignored: HashSet<PathBuf>,
    dirs: HashMap<PathBuf, FileStatus>,
    /// Folders git reports whole (ignored ones, untracked repositories):
    /// where nested repositories can be.
    folders: Vec<PathBuf>,
    /// Repositories inside this one, flattened and sorted by path. Their
    /// changes are in `changes`; each keeps its branch and ignored paths.
    nested: Vec<StatusSnapshot>,
}

/// How many folders below an ignored folder a nested repository is looked
/// for (`repos/` › `team/` › `app/.git`).
const NESTED_DEPTH: usize = 3;

/// How many repositories deep nesting goes (a clone inside a clone…).
const NESTED_LEVELS: usize = 3;

/// The git directory shared by all checkouts of the repository at `top`:
/// `top/.git`, or for a linked worktree (a `.git` file saying
/// `gitdir: <main>/.git/worktrees/<name>`) the main one's.
fn common_git_dir(top: &Path) -> Option<PathBuf> {
    let dot_git = top.join(".git");
    if dot_git.is_dir() {
        return dot_git.canonicalize().ok();
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let dir = top.join(text.strip_prefix("gitdir:")?.trim()).canonicalize().ok()?;
    match dir.parent() {
        Some(p) if p.file_name().is_some_and(|n| n == "worktrees") => p.parent().map(Path::to_path_buf),
        _ => Some(dir),
    }
}

/// The repositories cloned in `folders` (git's ignored and untracked
/// folders) of the repository at `top`: a folder with a `.git`, or one
/// below it. Not followed: symlinks, `.git`, `node_modules` and caches
/// (a `CACHEDIR.TAG`, like Cargo's `target/`): big, and never clones.
/// Worktrees of `top`'s own repository (`.claude/worktrees/…`) are other
/// checkouts of the same project, not repositories of their own.
fn find_nested(top: &Path, folders: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
        if dir.join(".git").exists() {
            // Its own status finds the repositories inside it.
            found.push(dir.to_path_buf());
            return;
        }
        if depth == 0 || dir.join("CACHEDIR.TAG").exists() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let skip = e.file_name() == ".git" || e.file_name() == "node_modules";
            if !skip && e.file_type().is_ok_and(|t| t.is_dir()) {
                walk(&e.path(), depth - 1, found);
            }
        }
    }
    let mut found = Vec::new();
    for f in folders {
        walk(f, NESTED_DEPTH, &mut found);
    }
    if let Some(own) = common_git_dir(top) {
        found.retain(|d| common_git_dir(d).is_none_or(|c| c != own));
    }
    found.sort();
    found.dedup();
    found
}

fn classify(xy: &[u8]) -> FileStatus {
    let (x, y) = (xy.first().copied().unwrap_or(b'.'), xy.get(1).copied().unwrap_or(b'.'));
    if x == b'R' || y == b'R' {
        FileStatus::Renamed
    } else if x == b'A' || x == b'C' {
        FileStatus::Added
    } else if x == b'D' || y == b'D' {
        FileStatus::Deleted
    } else {
        FileStatus::Modified
    }
}

impl StatusSnapshot {
    /// Runs `git status` for the whole repository, and for the
    /// repositories cloned inside it.
    pub fn load(git: &Git) -> Result<StatusSnapshot, GitError> {
        // With GIT_DIR set every git command sees that one repository.
        let levels = if std::env::var_os("GIT_DIR").is_some() {
            0
        } else {
            NESTED_LEVELS
        };
        StatusSnapshot::load_nested(git, levels)
    }

    fn load_nested(git: &Git, levels: usize) -> Result<StatusSnapshot, GitError> {
        let mut snap = StatusSnapshot::load_one(git)?;
        if levels == 0 {
            return Ok(snap);
        }
        let tops = find_nested(&git.toplevel, &snap.folders);
        // In parallel: a git process each.
        let nested: Vec<StatusSnapshot> = std::thread::scope(|s| {
            let jobs: Vec<_> = tops
                .into_iter()
                .map(|toplevel| s.spawn(move || StatusSnapshot::load_nested(&Git { toplevel }, levels - 1)))
                .collect();
            jobs.into_iter()
                .filter_map(|j| match j.join() {
                    Ok(Ok(snap)) => Some(snap),
                    Ok(Err(e)) => {
                        tracing::warn!("nested repository left out: {e}");
                        None
                    }
                    Err(_) => None,
                })
                .collect()
        });
        snap.nest(nested);
        Ok(snap)
    }

    /// `git status` of this repository alone.
    fn load_one(git: &Git) -> Result<StatusSnapshot, GitError> {
        let out = git.run_bytes(&[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
            "--ignored=matching",
        ])?;
        Ok(StatusSnapshot::parse(&git.toplevel, &out))
    }

    /// Parses `git status --porcelain=v2 -z --branch` output.
    pub fn parse(toplevel: &Path, bytes: &[u8]) -> StatusSnapshot {
        let mut snap = StatusSnapshot {
            toplevel: toplevel.to_path_buf(),
            ..Default::default()
        };
        let mut records = bytes.split(|b| *b == 0).filter(|r| !r.is_empty());
        let abs = |rel: &[u8]| -> PathBuf {
            let s = String::from_utf8_lossy(rel);
            toplevel.join(s.trim_end_matches('/'))
        };
        while let Some(rec) = records.next() {
            let text = String::from_utf8_lossy(rec);
            let mut push = |path: PathBuf, status: FileStatus, orig: Option<PathBuf>, xy: &[u8]| {
                let staged = xy.first().is_some_and(|c| *c != b'.');
                let unstaged = xy.get(1).is_some_and(|c| *c != b'.');
                snap.changes.push(Change {
                    path,
                    status,
                    orig_path: orig,
                    staged,
                    unstaged,
                });
            };
            match rec.first() {
                Some(b'#') => {
                    let mut parts = text.splitn(3, ' ');
                    let _ = parts.next();
                    let key = parts.next().unwrap_or("");
                    let val = parts.next().unwrap_or("").to_string();
                    match key {
                        "branch.oid" if val != "(initial)" => snap.branch.oid = Some(val),
                        "branch.head" if val != "(detached)" => snap.branch.head = Some(val),
                        "branch.upstream" => snap.branch.upstream = Some(val),
                        "branch.ab" => {
                            for p in val.split(' ') {
                                if let Some(n) = p.strip_prefix('+') {
                                    snap.branch.ahead = n.parse().unwrap_or(0);
                                } else if let Some(n) = p.strip_prefix('-') {
                                    snap.branch.behind = n.parse().unwrap_or(0);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Some(b'1') => {
                    // 1 XY sub mH mI mW hH hI <path>
                    let fields: Vec<&[u8]> = rec.splitn(9, |b| *b == b' ').collect();
                    if fields.len() == 9 {
                        push(abs(fields[8]), classify(fields[1]), None, fields[1]);
                    }
                }
                Some(b'2') => {
                    // 2 XY sub mH mI mW hH hI Xscore <path> NUL <origPath>
                    let fields: Vec<&[u8]> = rec.splitn(10, |b| *b == b' ').collect();
                    let orig = records.next().map(abs);
                    if fields.len() == 10 {
                        push(abs(fields[9]), classify(fields[1]), orig, fields[1]);
                    }
                }
                Some(b'u') => {
                    // u XY sub m1 m2 m3 mW h1 h2 h3 <path>
                    let fields: Vec<&[u8]> = rec.splitn(11, |b| *b == b' ').collect();
                    if fields.len() == 11 {
                        push(abs(fields[10]), FileStatus::Conflicted, None, b"UU");
                    }
                }
                Some(b'?') => {
                    // With every untracked file listed, a folder is a repository.
                    if rec.ends_with(b"/") {
                        snap.folders.push(abs(&rec[2..]));
                    }
                    push(abs(&rec[2..]), FileStatus::Unversioned, None, b"..");
                }
                Some(b'!') => {
                    if rec.ends_with(b"/") {
                        snap.folders.push(abs(&rec[2..]));
                    }
                    snap.ignored.insert(abs(&rec[2..]));
                }
                _ => {}
            }
        }
        snap.changes.sort_by(|a, b| a.path.cmp(&b.path));
        snap.by_path = snap
            .changes
            .iter()
            .enumerate()
            .map(|(i, c)| (c.path.clone(), i))
            .collect();
        snap.dirs = rollup(toplevel, &snap.changes);
        snap
    }

    /// Takes in the snapshots of repositories inside this one: their
    /// changes join this one's, and what this repository says about them
    /// (`? vendor/lib/`) goes.
    pub fn nest(&mut self, nested: Vec<StatusSnapshot>) {
        let mut theirs = Vec::new();
        for mut n in nested {
            theirs.append(&mut n.changes);
            n.by_path.clear();
            n.dirs.clear();
            self.nested.append(&mut n.nested);
            self.nested.push(n);
        }
        self.nested.sort_by(|a, b| a.toplevel.cmp(&b.toplevel));
        let mut changes = std::mem::take(&mut self.changes);
        changes.retain(|c| self.repo(&c.path).toplevel == self.toplevel);
        changes.append(&mut theirs);
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        self.by_path = changes.iter().enumerate().map(|(i, c)| (c.path.clone(), i)).collect();
        // A folder rolls up every change inside it, whichever repository.
        self.dirs = rollup(&self.toplevel, &changes);
        self.changes = changes;
    }

    /// The repository `path` is in: the innermost nested one holding it,
    /// or this one. Its `toplevel` and `branch` describe it.
    pub fn repo(&self, path: &Path) -> &StatusSnapshot {
        self.nested
            .iter()
            .filter(|n| path.starts_with(&n.toplevel))
            .max_by_key(|n| n.toplevel.components().count())
            .unwrap_or(self)
    }

    /// The repositories inside this one, sorted by path.
    pub fn nested(&self) -> &[StatusSnapshot] {
        &self.nested
    }

    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    pub fn change(&self, path: &Path) -> Option<&Change> {
        self.by_path.get(path).map(|i| &self.changes[*i])
    }

    /// Whether `path` or any ancestor is git-ignored, by the repository it
    /// is in. Folders holding nested repositories are not: they lead to them.
    pub fn is_ignored(&self, path: &Path) -> bool {
        let repo = self.repo(path);
        if repo.ignored.is_empty() || self.nested.iter().any(|n| n.toplevel.starts_with(path)) {
            return false;
        }
        path.ancestors()
            .take_while(|p| p.starts_with(&repo.toplevel) && *p != repo.toplevel)
            .any(|p| repo.ignored.contains(p))
    }

    /// Status of a file: its change, or `Ignored`.
    pub fn file_status(&self, path: &Path) -> Option<FileStatus> {
        match self.change(path) {
            Some(c) => Some(c.status),
            None if self.is_ignored(path) => Some(FileStatus::Ignored),
            None => None,
        }
    }

    /// Status of a folder: `Ignored`, or the roll-up of changes inside it
    /// (`Unversioned` when everything inside is new, otherwise `Modified`).
    pub fn dir_status(&self, path: &Path) -> Option<FileStatus> {
        if self.is_ignored(path) {
            return Some(FileStatus::Ignored);
        }
        self.dirs.get(path).copied()
    }

    /// Number of changes per kind (for the status bar).
    pub fn counts(&self) -> BTreeMap<FileStatus, usize> {
        let mut m = BTreeMap::new();
        for c in &self.changes {
            *m.entry(c.status).or_insert(0) += 1;
        }
        m
    }
}

fn rollup(toplevel: &Path, changes: &[Change]) -> HashMap<PathBuf, FileStatus> {
    let mut dirs: HashMap<PathBuf, FileStatus> = HashMap::new();
    for c in changes {
        let mut dir = c.path.parent();
        while let Some(d) = dir {
            if !d.starts_with(toplevel) {
                break;
            }
            let merged = match (dirs.get(d), c.status) {
                (None, FileStatus::Unversioned) => FileStatus::Unversioned,
                (Some(FileStatus::Unversioned), FileStatus::Unversioned) => FileStatus::Unversioned,
                _ => FileStatus::Modified,
            };
            dirs.insert(d.to_path_buf(), merged);
            if d == toplevel {
                break;
            }
            dir = d.parent();
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top() -> PathBuf {
        PathBuf::from("/repo")
    }

    fn parse(records: &[&str]) -> StatusSnapshot {
        let mut bytes = Vec::new();
        for r in records {
            bytes.extend_from_slice(r.as_bytes());
            bytes.push(0);
        }
        StatusSnapshot::parse(&top(), &bytes)
    }

    #[test]
    fn parses_every_record_type() {
        let s = parse(&[
            "# branch.oid 1234567890abcdef",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +2 -1",
            "1 .M N... 100644 100644 100644 aaa bbb src/main.rs",
            "1 A. N... 000000 100644 100644 000 ccc new file.txt",
            "1 .D N... 100644 100644 000000 ddd ddd gone.rs",
            "2 R. N... 100644 100644 100644 eee eee R100 docs/new name.md",
            "docs/old name.md",
            "u UU N... 100644 100644 100644 100644 a b c conflict.rs",
            "? notes/ünïcode.txt",
            "! target/",
            "! debug.log",
        ]);
        assert_eq!(s.branch.head.as_deref(), Some("main"));
        assert_eq!(s.branch.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.branch.ahead, s.branch.behind), (2, 1));
        assert_eq!(s.branch.label(), "main");

        let st = |p: &str| s.file_status(&top().join(p));
        assert_eq!(st("src/main.rs"), Some(FileStatus::Modified));
        assert_eq!(st("new file.txt"), Some(FileStatus::Added));
        assert_eq!(st("gone.rs"), Some(FileStatus::Deleted));
        assert_eq!(st("docs/new name.md"), Some(FileStatus::Renamed));
        assert_eq!(st("conflict.rs"), Some(FileStatus::Conflicted));
        assert_eq!(st("notes/ünïcode.txt"), Some(FileStatus::Unversioned));
        assert_eq!(st("target/debug/app"), Some(FileStatus::Ignored));
        assert_eq!(st("debug.log"), Some(FileStatus::Ignored));
        assert_eq!(st("README.md"), None);

        let renamed = s.change(&top().join("docs/new name.md")).unwrap();
        assert_eq!(renamed.orig_path, Some(top().join("docs/old name.md")));
        assert!(renamed.staged && !renamed.unstaged);
        let modified = s.change(&top().join("src/main.rs")).unwrap();
        assert!(!modified.staged && modified.unstaged);
        assert_eq!(s.changes().len(), 6);
    }

    #[test]
    fn rolls_changes_up_to_folders() {
        let s = parse(&[
            "1 .M N... 100644 100644 100644 a b crates/git/src/status.rs",
            "? crates/git/src/watch.rs",
            "? fresh/a.txt",
            "? fresh/sub/b.txt",
        ]);
        let d = |p: &str| s.dir_status(&top().join(p));
        assert_eq!(d("crates"), Some(FileStatus::Modified));
        assert_eq!(d("crates/git/src"), Some(FileStatus::Modified));
        assert_eq!(d("fresh"), Some(FileStatus::Unversioned), "only new files inside");
        assert_eq!(d("fresh/sub"), Some(FileStatus::Unversioned));
        assert_eq!(d("docs"), None);
        assert_eq!(s.dir_status(&top()), Some(FileStatus::Modified));
    }

    #[test]
    fn ignored_folders_cover_their_contents() {
        let s = parse(&["! node_modules/"]);
        assert!(s.is_ignored(&top().join("node_modules")));
        assert!(s.is_ignored(&top().join("node_modules/a/b.js")));
        assert!(!s.is_ignored(&top().join("src")));
        assert_eq!(s.dir_status(&top().join("node_modules")), Some(FileStatus::Ignored));
    }

    fn parse_at(toplevel: &Path, records: &[&str]) -> StatusSnapshot {
        let mut bytes = Vec::new();
        for r in records {
            bytes.extend_from_slice(r.as_bytes());
            bytes.push(0);
        }
        StatusSnapshot::parse(toplevel, &bytes)
    }

    #[test]
    fn nested_repositories_keep_their_own_status() {
        let mut s = parse(&[
            "# branch.head main",
            "1 .M N... 100644 100644 100644 a b src/a.rs",
            "! repos/",
            "! debug.log",
            "? vendor/lib/",
        ]);
        assert_eq!(s.folders, vec![top().join("repos"), top().join("vendor/lib")]);
        let foo = parse_at(
            &top().join("repos/foo"),
            &[
                "# branch.head dev",
                "1 .M N... 100644 100644 100644 a b x.rs",
                "? new.txt",
                "! build/",
            ],
        );
        let lib = parse_at(&top().join("vendor/lib"), &["# branch.head main", "? z.txt"]);
        s.nest(vec![lib, foo]);

        let st = |p: &str| s.file_status(&top().join(p));
        assert_eq!(st("src/a.rs"), Some(FileStatus::Modified));
        assert_eq!(
            st("repos/foo/x.rs"),
            Some(FileStatus::Modified),
            "not ignored: its own repo"
        );
        assert_eq!(st("repos/foo/new.txt"), Some(FileStatus::Unversioned));
        assert_eq!(
            st("repos/foo/build/out.o"),
            Some(FileStatus::Ignored),
            "the nested repo's ignores"
        );
        assert_eq!(st("repos/foo/README.md"), None);
        assert_eq!(
            st("repos/notes.txt"),
            Some(FileStatus::Ignored),
            "beside the clones: still ignored"
        );
        assert_eq!(st("debug.log"), Some(FileStatus::Ignored));
        // The outer repository's `? vendor/lib/` gave way to the repository itself.
        assert!(s.change(&top().join("vendor/lib")).is_none());
        assert_eq!(st("vendor/lib/z.txt"), Some(FileStatus::Unversioned));
        assert_eq!(s.changes().len(), 4);

        // The folder holding clones stays visible, and rolls up their changes.
        assert!(!s.is_ignored(&top().join("repos")));
        assert!(!s.is_ignored(&top().join("repos/foo")));
        assert_eq!(s.dir_status(&top().join("repos")), Some(FileStatus::Modified));
        assert_eq!(s.dir_status(&top().join("repos/foo")), Some(FileStatus::Modified));
        assert_eq!(s.dir_status(&top().join("vendor/lib")), Some(FileStatus::Unversioned));

        // Which repository a path is in, with its branch.
        assert_eq!(s.repo(&top().join("src/a.rs")).toplevel, top());
        let foo = s.repo(&top().join("repos/foo/x.rs"));
        assert_eq!(foo.toplevel, top().join("repos/foo"));
        assert_eq!(foo.branch.label(), "dev");
        assert_eq!(s.repo(&top().join("repos/foo")).toplevel, top().join("repos/foo"));
        let tops: Vec<_> = s.nested().iter().map(|n| n.toplevel.clone()).collect();
        assert_eq!(tops, vec![top().join("repos/foo"), top().join("vendor/lib")]);
    }

    #[test]
    fn nesting_flattens_repositories_inside_nested_ones() {
        let mut inner = parse_at(&top().join("a"), &["! deps/"]);
        inner.nest(vec![parse_at(&top().join("a/deps/b"), &["? b.txt"])]);
        let mut s = parse(&["! a/"]);
        s.nest(vec![inner]);
        assert_eq!(s.nested().len(), 2);
        assert_eq!(s.repo(&top().join("a/deps/b/b.txt")).toplevel, top().join("a/deps/b"));
        assert_eq!(s.repo(&top().join("a/x")).toplevel, top().join("a"));
        assert_eq!(
            s.file_status(&top().join("a/deps/b/b.txt")),
            Some(FileStatus::Unversioned)
        );
        assert!(!s.is_ignored(&top().join("a/deps")), "leads to b");
        assert!(s.is_ignored(&top().join("a/deps/other")));
    }

    #[test]
    fn detached_head_and_initial_commit() {
        let s = parse(&["# branch.oid abcdef1234567890", "# branch.head (detached)"]);
        assert_eq!(s.branch.head, None);
        assert_eq!(s.branch.label(), "abcdef12");
        let s = parse(&["# branch.oid (initial)", "# branch.head main"]);
        assert_eq!(s.branch.oid, None);
        assert_eq!(s.branch.label(), "main");
    }

    #[test]
    fn counts_by_kind() {
        let s = parse(&[
            "1 .M N... 100644 100644 100644 a b a.rs",
            "1 .M N... 100644 100644 100644 a b b.rs",
            "? c.rs",
        ]);
        let c = s.counts();
        assert_eq!(c[&FileStatus::Modified], 2);
        assert_eq!(c[&FileStatus::Unversioned], 1);
    }
}
