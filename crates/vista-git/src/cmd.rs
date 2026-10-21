//! Running git.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git is not installed or not on PATH: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("{} is not inside a git repository", .0.display())]
    NotARepo(PathBuf),
    #[error("git {args} failed: {stderr}")]
    Failed { args: String, stderr: String },
}

/// A handle on one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Git {
    /// The repository's top-level directory (where `.git` lives).
    pub toplevel: PathBuf,
}

impl Git {
    /// Finds the repository containing `dir`: the nearest ancestor with a
    /// `.git` entry (directory, or file for worktrees and submodules). This
    /// avoids spawning git at startup; with `GIT_DIR` set, git decides.
    pub fn discover(dir: &Path) -> Result<Git, GitError> {
        if std::env::var_os("GIT_DIR").is_none() {
            let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
            return dir
                .ancestors()
                .find(|a| a.join(".git").exists())
                .map(|top| Git {
                    toplevel: top.to_path_buf(),
                })
                .ok_or_else(|| GitError::NotARepo(dir.clone()));
        }
        let out = Command::new("git")
            .current_dir(dir)
            .args(["rev-parse", "--show-toplevel"])
            .stdin(Stdio::null())
            .output()
            .map_err(GitError::Spawn)?;
        if !out.status.success() {
            return Err(GitError::NotARepo(dir.to_path_buf()));
        }
        let top = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let toplevel = PathBuf::from(top);
        // Canonicalise so paths compare equal with canonicalised roots (macOS /private/var…).
        let toplevel = toplevel.canonicalize().unwrap_or(toplevel);
        Ok(Git { toplevel })
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.toplevel)
            .arg("-c")
            .arg("core.quotepath=off")
            .args(args)
            .stdin(Stdio::null());
        cmd
    }

    /// Runs git and returns stdout as bytes.
    pub fn run_bytes(&self, args: &[&str]) -> Result<Vec<u8>, GitError> {
        let out = self.command(args).output().map_err(GitError::Spawn)?;
        if !out.status.success() {
            return Err(GitError::Failed {
                args: args.join(" "),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(out.stdout)
    }

    /// Runs git and returns stdout as (lossy) UTF-8.
    pub fn run(&self, args: &[&str]) -> Result<String, GitError> {
        Ok(String::from_utf8_lossy(&self.run_bytes(args)?).into_owned())
    }

    /// Runs git with `input` on stdin; returns combined stdout+stderr on
    /// success, or the error with both streams (hooks print to either).
    pub fn run_with_input(&self, args: &[&str], input: &[u8], env: &[(&str, &str)]) -> Result<String, GitError> {
        let mut cmd = self.command(args);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().map_err(GitError::Spawn)?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input);
        }
        let out = child.wait_with_output().map_err(GitError::Spawn)?;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        if out.status.success() {
            Ok(text)
        } else {
            Err(GitError::Failed {
                args: args.join(" "),
                stderr: text.trim().to_string(),
            })
        }
    }

    /// A file's content in HEAD, or `None` if it isn't in HEAD (new file, no
    /// commits yet, or outside the repository).
    pub fn head_content(&self, path: &Path) -> Option<Vec<u8>> {
        let rel = self.relative(path)?;
        self.run_bytes(&["cat-file", "blob", &format!("HEAD:{rel}")]).ok()
    }

    /// Path relative to the repository top level, with `/` separators.
    pub fn relative(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(&self.toplevel).ok()?;
        Some(rel.to_string_lossy().replace('\\', "/"))
    }
}
