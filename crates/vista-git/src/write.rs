//! Changing the repository: commit, amend, rollback, add, push.
//!
//! Commits are built in a temporary index that starts from HEAD and receives
//! exactly the selected files (or, for partial commits, selected content).
//! The real index is then reset for those paths. Unselected staged files stay
//! staged and out of the commit, hooks run normally (`GIT_INDEX_FILE` points
//! them at the temporary index), and signing/credential helpers keep working.

use std::path::{Path, PathBuf};

use crate::cmd::{Git, GitError};
use crate::status::{Change, FileStatus};

/// What to put in the commit for one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitContent {
    /// The file as it is on disk (or its deletion if it's gone).
    WorkingTree,
    /// Exactly these bytes (partial commits).
    Blob(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFile {
    pub path: PathBuf,
    /// For renames: the old path, removed in the commit.
    pub orig_path: Option<PathBuf>,
    pub content: CommitContent,
}

impl CommitFile {
    pub fn whole(change: &Change) -> CommitFile {
        CommitFile {
            path: change.path.clone(),
            orig_path: change.orig_path.clone(),
            content: CommitContent::WorkingTree,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitResult {
    /// Short id of the new commit.
    pub id: String,
    pub subject: String,
    /// Output of git and its hooks.
    pub output: String,
}

fn rel(git: &Git, p: &Path) -> Result<String, GitError> {
    git.relative(p).ok_or_else(|| GitError::Failed {
        args: "commit".into(),
        stderr: format!("{} is outside the repository", p.display()),
    })
}

impl Git {
    fn has_head(&self) -> bool {
        self.run(&["rev-parse", "--verify", "-q", "HEAD"]).is_ok()
    }

    fn git_path(&self, name: &str) -> Result<PathBuf, GitError> {
        let p = self.run(&["rev-parse", "--git-path", name])?;
        let p = PathBuf::from(p.trim());
        Ok(if p.is_absolute() { p } else { self.toplevel.join(p) })
    }

    /// Commits `files` with `message` (or amends HEAD).
    pub fn commit(&self, files: &[CommitFile], message: &str, amend: bool) -> Result<CommitResult, GitError> {
        let index = self.git_path(&format!("vista-index-{}", std::process::id()))?;
        let index_s = index.to_string_lossy().into_owned();
        let env = [("GIT_INDEX_FILE", index_s.as_str())];
        let result = (|| {
            if self.has_head() {
                self.run_with_input(&["read-tree", "HEAD"], b"", &env)?;
            } else {
                self.run_with_input(&["read-tree", "--empty"], b"", &env)?;
            }
            for f in files {
                if let Some(orig) = &f.orig_path {
                    let o = rel(self, orig)?;
                    self.run_with_input(&["update-index", "--force-remove", "--", &o], b"", &env)?;
                }
                let r = rel(self, &f.path)?;
                match &f.content {
                    CommitContent::WorkingTree if f.path.exists() => {
                        self.run_with_input(&["update-index", "--add", "--", &r], b"", &env)?;
                    }
                    CommitContent::WorkingTree => {
                        self.run_with_input(&["update-index", "--force-remove", "--", &r], b"", &env)?;
                    }
                    CommitContent::Blob(bytes) => {
                        let sha = self.run_with_input(&["hash-object", "-w", "--stdin"], bytes, &[])?;
                        let mode = file_mode(&f.path);
                        let info = format!("{mode},{},{r}", sha.trim());
                        self.run_with_input(&["update-index", "--add", "--cacheinfo", &info], b"", &env)?;
                    }
                }
            }
            let mut args = vec!["commit", "-F", "-"];
            if amend {
                args.push("--amend");
            }
            self.run_with_input(&args, message.as_bytes(), &env)
        })();
        let _ = std::fs::remove_file(&index);
        let output = result?;
        // Sync the real index for the committed paths with the new HEAD.
        let mut paths: Vec<String> = Vec::new();
        for f in files {
            paths.push(rel(self, &f.path)?);
            if let Some(o) = &f.orig_path {
                paths.push(rel(self, o)?);
            }
        }
        if !paths.is_empty() {
            let mut args = vec!["reset", "-q", "--"];
            args.extend(paths.iter().map(String::as_str));
            let _ = self.run(&args);
        }
        let id = self.run(&["rev-parse", "--short", "HEAD"])?.trim().to_string();
        let subject = self.run(&["log", "-1", "--format=%s"])?.trim().to_string();
        Ok(CommitResult { id, subject, output })
    }

    /// Message of the last commit (to prefill Amend).
    pub fn last_message(&self) -> Option<String> {
        self.run(&["log", "-1", "--format=%B"])
            .ok()
            .map(|s| s.trim_end().to_string())
    }

    /// Reverts changes to HEAD. Added files are unstaged and kept on disk;
    /// renames get their old path back (the new file stays, unversioned).
    pub fn rollback(&self, changes: &[Change]) -> Result<(), GitError> {
        for c in changes {
            let r = rel(self, &c.path)?;
            match c.status {
                FileStatus::Modified | FileStatus::Deleted => {
                    self.run(&["restore", "--source=HEAD", "--staged", "--worktree", "--", &r])?;
                }
                FileStatus::Added => {
                    self.run(&["rm", "-q", "--cached", "-f", "--", &r])?;
                }
                FileStatus::Renamed => {
                    if let Some(o) = &c.orig_path {
                        let o = rel(self, o)?;
                        self.run(&["restore", "--source=HEAD", "--staged", "--worktree", "--", &o])?;
                    }
                    self.run(&["rm", "-q", "--cached", "-f", "--", &r])?;
                }
                FileStatus::Conflicted | FileStatus::Unversioned | FileStatus::Ignored => {}
            }
        }
        Ok(())
    }

    /// `git add` (Add to VCS).
    pub fn add(&self, paths: &[PathBuf]) -> Result<(), GitError> {
        let rels: Vec<String> = paths.iter().map(|p| rel(self, p)).collect::<Result<_, _>>()?;
        let mut args = vec!["add", "--"];
        args.extend(rels.iter().map(String::as_str));
        self.run(&args).map(|_| ())
    }

    /// `git push` of the current branch; returns git's output.
    pub fn push(&self) -> Result<String, GitError> {
        self.run_with_input(&["push"], b"", &[])
    }
}

fn file_mode(path: &Path) -> &'static str {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0) {
            return "100755";
        }
    }
    let _ = path;
    "100644"
}
