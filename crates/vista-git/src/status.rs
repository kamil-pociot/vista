//! `git status` as a snapshot: per-file status, folder roll-up, ignored paths
//! and branch info.

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
    changes: Vec<Change>,
    by_path: HashMap<PathBuf, usize>,
    ignored: HashSet<PathBuf>,
    dirs: HashMap<PathBuf, FileStatus>,
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
    /// Runs `git status` for the whole repository.
    pub fn load(git: &Git) -> Result<StatusSnapshot, GitError> {
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
                Some(b'?') => push(abs(&rec[2..]), FileStatus::Unversioned, None, b".."),
                Some(b'!') => {
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

    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    pub fn change(&self, path: &Path) -> Option<&Change> {
        self.by_path.get(path).map(|i| &self.changes[*i])
    }

    /// Whether `path` or any ancestor is git-ignored.
    pub fn is_ignored(&self, path: &Path) -> bool {
        if self.ignored.is_empty() {
            return false;
        }
        path.ancestors()
            .take_while(|p| p.starts_with(&self.toplevel) && *p != self.toplevel)
            .any(|p| self.ignored.contains(p))
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
