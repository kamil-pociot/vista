//! Throw-away project directories and git repositories for tests.
//!
//! Git runs hermetically: a private `HOME` and global config, no system config,
//! fixed author and dates, so screens and hashes are deterministic.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

/// Environment for running git (and vista) hermetically against `home`.
pub fn git_env(home: &Path) -> Vec<(String, String)> {
    let s = |p: PathBuf| p.to_string_lossy().into_owned();
    vec![
        ("HOME".into(), s(home.to_path_buf())),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ("GIT_CONFIG_GLOBAL".into(), s(home.join(".gitconfig"))),
        ("GIT_AUTHOR_NAME".into(), "Vista Test".into()),
        ("GIT_AUTHOR_EMAIL".into(), "test@vista.invalid".into()),
        ("GIT_COMMITTER_NAME".into(), "Vista Test".into()),
        ("GIT_COMMITTER_EMAIL".into(), "test@vista.invalid".into()),
        ("GIT_AUTHOR_DATE".into(), "2026-01-01T12:00:00Z".into()),
        ("GIT_COMMITTER_DATE".into(), "2026-01-01T12:00:00Z".into()),
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
        ("GIT_OPTIONAL_LOCKS".into(), "0".into()),
    ]
}

fn write_gitconfig(home: &Path) {
    std::fs::write(
        home.join(".gitconfig"),
        "[user]\n\tname = Vista Test\n\temail = test@vista.invalid\n[init]\n\tdefaultBranch = main\n[core]\n\tautocrlf = false\n[commit]\n\tgpgsign = false\n",
    )
    .expect("write gitconfig");
}

/// A built fixture: a project directory plus a private home directory.
pub struct Repo {
    root: TempDir,
    home: TempDir,
    pub is_git: bool,
}

impl Repo {
    pub fn path(&self) -> &Path {
        self.root.path()
    }

    pub fn home(&self) -> &Path {
        self.home.path()
    }

    pub fn env(&self) -> Vec<(String, String)> {
        git_env(self.home.path())
    }

    /// Runs git in the repo and returns stdout. Panics on failure.
    pub fn git(&self, args: &[&str]) -> String {
        run_git(self.root.path(), self.home.path(), args)
    }

    /// Runs git and returns (success, stdout+stderr) without panicking.
    pub fn try_git(&self, args: &[&str]) -> (bool, String) {
        let out = git_command(self.root.path(), self.home.path(), args)
            .output()
            .expect("run git");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }

    pub fn write(&self, rel: &str, content: impl AsRef<[u8]>) {
        let p = self.root.path().join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(p, content).expect("write file");
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.root.path().join(rel)).expect("read file")
    }

    pub fn remove(&self, rel: &str) {
        let p = self.root.path().join(rel);
        if p.is_dir() {
            std::fs::remove_dir_all(p).expect("remove dir");
        } else {
            std::fs::remove_file(p).expect("remove file");
        }
    }

    /// `git status --porcelain=v1` lines, sorted.
    pub fn status(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .git(&["status", "--porcelain=v1", "--untracked-files=all"])
            .lines()
            .map(str::to_string)
            .collect();
        v.sort();
        v
    }
}

fn git_command(dir: &Path, home: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir).args(args);
    for (k, v) in git_env(home) {
        cmd.env(k, v);
    }
    cmd
}

fn run_git(dir: &Path, home: &Path, args: &[&str]) -> String {
    let out = git_command(dir, home, args).output().expect("run git");
    assert!(
        out.status.success(),
        "git {} failed:\n{}{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

enum Op {
    Write(String, Vec<u8>),
    Remove(String),
    Git(Vec<String>),
}

/// Builder for a fixture. Operations run in order on `build()`.
pub struct Fixture {
    git: bool,
    ops: Vec<Op>,
}

impl Fixture {
    /// An empty git repository.
    pub fn git() -> Fixture {
        Fixture {
            git: true,
            ops: Vec::new(),
        }
    }

    /// A plain directory without git.
    pub fn plain() -> Fixture {
        Fixture {
            git: false,
            ops: Vec::new(),
        }
    }

    pub fn file(mut self, rel: &str, content: impl AsRef<[u8]>) -> Fixture {
        self.ops.push(Op::Write(rel.to_string(), content.as_ref().to_vec()));
        self
    }

    /// Alias of [`file`](Self::file) that reads better after a commit.
    pub fn modify(self, rel: &str, content: impl AsRef<[u8]>) -> Fixture {
        self.file(rel, content)
    }

    pub fn delete(mut self, rel: &str) -> Fixture {
        self.ops.push(Op::Remove(rel.to_string()));
        self
    }

    /// `git add -A && git commit -m msg`.
    pub fn commit(self, msg: &str) -> Fixture {
        self.run(&["add", "-A"]).run(&["commit", "-q", "-m", msg])
    }

    pub fn stage(self, rel: &str) -> Fixture {
        self.run(&["add", "--", rel])
    }

    /// Any git command.
    pub fn run(mut self, args: &[&str]) -> Fixture {
        self.ops.push(Op::Git(args.iter().map(|s| s.to_string()).collect()));
        self
    }

    pub fn build(self) -> Repo {
        let root = tempfile::Builder::new()
            .prefix("vista-e2e-")
            .tempdir()
            .expect("tempdir");
        let home = tempfile::Builder::new()
            .prefix("vista-home-")
            .tempdir()
            .expect("tempdir");
        write_gitconfig(home.path());
        let repo = Repo {
            root,
            home,
            is_git: self.git,
        };
        if self.git {
            run_git(repo.path(), repo.home(), &["init", "-q", "-b", "main"]);
        }
        for op in self.ops {
            match op {
                Op::Write(rel, content) => repo.write(&rel, content),
                Op::Remove(rel) => repo.remove(&rel),
                Op::Git(args) => {
                    let args: Vec<&str> = args.iter().map(String::as_str).collect();
                    run_git(repo.path(), repo.home(), &args);
                }
            }
        }
        repo
    }

    /// The sample project used in the design mockups: a Rust workspace with
    /// modified, added, unversioned and ignored files.
    pub fn rust_project() -> Fixture {
        Fixture::git()
            .file(".gitignore", "target/\n*.log\n")
            .file("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n")
            .file("Cargo.lock", "# lock v1\n")
            .file("README.md", "# file-browser\n\nA sample project.\n")
            .file("crates/fb-git/Cargo.toml", "[package]\nname = \"fb-git\"\n")
            .file("crates/fb-git/src/lib.rs", "pub mod diff;\npub mod status;\n")
            .file("crates/fb-git/src/diff.rs", "pub fn diff() {}\n")
            .file("crates/fb-git/src/status.rs", STATUS_RS_HEAD)
            .file("crates/fb-core/src/lib.rs", "pub fn core() {}\n")
            .file("docs/guide.md", "# Guide\n")
            .commit("Initial commit")
            .modify("crates/fb-git/src/status.rs", STATUS_RS_WORKING)
            .modify("crates/fb-git/src/diff.rs", "pub fn diff() -> usize { 0 }\n")
            .modify("Cargo.lock", "# lock v2\n")
            .file("ROADMAP.md", "# Roadmap\n")
            .stage("ROADMAP.md")
            .file("crates/fb-git/src/watch.rs", "pub fn watch() {}\n")
            .file("screenshot.png", TINY_PNG)
            .file("target/debug/app", "binary")
            .file("build.log", "log")
    }
}

/// `status.rs` as committed in [`Fixture::rust_project`].
pub const STATUS_RS_HEAD: &str = "use std::path::{Path, PathBuf};

use gix::status::index_worktree::Item;

pub struct StatusMap {
    root: PathBuf,
    entries: HashMap<PathBuf, FileStatus>,
}

impl StatusMap {
    pub fn load(repo: &gix::Repository) -> anyhow::Result<Self> {
        let root = repo.work_dir().expect(\"bare repository\");
        let mut entries = HashMap::new();
        for item in repo.status()?.into_iter()? {
            entries.insert(root.join(item.location()), classify(&item?));
        }
        Ok(Self { root: root.to_owned(), entries })
    }

    pub fn get(&self, path: &Path) -> Option<FileStatus> {
        self.entries.get(path).copied()
    }
}
";

/// `status.rs` in the working tree of [`Fixture::rust_project`].
pub const STATUS_RS_WORKING: &str = "use std::path::{Path, PathBuf};

use anyhow::Context;
use gix::status::index_worktree::Item;

pub struct StatusMap {
    root: PathBuf,
    entries: HashMap<PathBuf, FileStatus>,
}

impl StatusMap {
    pub fn load(repo: &gix::Repository) -> anyhow::Result<Self> {
        let root = repo.workdir().context(\"bare repository\")?;
        let mut entries = HashMap::new();
        for item in repo.status(gix::progress::Discard)?.into_iter(None)? {
            let item = item?;
            entries.insert(root.join(item.location()), classify(&item));
        }
        Ok(Self { root: root.to_owned(), entries })
    }

    /// Status of `path`, or `None` when the file is clean.
    pub fn get(&self, path: &Path) -> Option<FileStatus> {
        self.entries.get(path).copied()
    }

    pub fn is_dirty(&self) -> bool {
        !self.entries.is_empty()
    }
}
";

/// A valid 2×2 PNG (red, green / blue, white).
pub const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00,
    0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xFD, 0xD4, 0x9A, 0x73, 0x00, 0x00, 0x00, 0x12, 0x49,
    0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xC0, 0x00, 0xC2, 0x0C, 0xFF, 0x81, 0x00, 0x00, 0x1F, 0xEE,
    0x05, 0xFB, 0x0B, 0xD9, 0x68, 0x8B, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_project_has_every_status_kind() {
        let repo = Fixture::rust_project().build();
        let st = repo.status();
        assert!(st.contains(&" M crates/fb-git/src/status.rs".to_string()), "{st:?}");
        assert!(st.contains(&"A  ROADMAP.md".to_string()), "{st:?}");
        assert!(st.contains(&"?? crates/fb-git/src/watch.rs".to_string()), "{st:?}");
        assert!(!st.iter().any(|l| l.contains("target/")), "target/ is ignored");
        let log = repo.git(&["log", "--format=%an %s"]);
        assert_eq!(log.trim(), "Vista Test Initial commit");
    }

    #[test]
    fn plain_fixture_has_no_git() {
        let repo = Fixture::plain().file("a.txt", "x").build();
        assert!(!repo.path().join(".git").exists());
        assert_eq!(repo.read("a.txt"), "x");
    }
}
