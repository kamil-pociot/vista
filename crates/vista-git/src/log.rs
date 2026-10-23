//! History: the commit log (optionally of one file) and the files each
//! commit changed.

use std::path::{Path, PathBuf};

use crate::cmd::{Git, GitError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    /// Parent hashes (none for a root commit, two or more for a merge).
    pub parents: Vec<String>,
    pub author: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    /// Branch and tag names pointing here (`HEAD -> main, tag: v1`).
    pub refs: String,
    pub subject: String,
}

/// A file a commit changed (compared with its first parent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFile {
    /// `A`dded, `M`odified, `D`eleted, `R`enamed, `C`opied, `T`ype change.
    pub status: char,
    /// Absolute path (the new one for renames).
    pub path: PathBuf,
    /// The path before a rename or copy.
    pub old_path: Option<PathBuf>,
}

const SEP: char = '\u{1f}';

/// Up to `max` commits reachable from HEAD, newest first; only those
/// touching `path` (following renames) when given.
pub fn log(git: &Git, path: Option<&Path>, max: usize) -> Result<Vec<Commit>, GitError> {
    let max = format!("-n{max}");
    let mut args = vec![
        "log",
        "--format=%H%x1f%h%x1f%P%x1f%an%x1f%ad%x1f%D%x1f%s",
        "--date=short",
        max.as_str(),
    ];
    let rel = path.and_then(|p| git.relative(p));
    if let Some(rel) = &rel {
        args.extend(["--follow", "--", rel.as_str()]);
    }
    let out = match git.run(&args) {
        Ok(out) => out,
        // A repository without commits has no history (not an error).
        Err(GitError::Failed { stderr, .. }) if stderr.contains("does not have any commits") => String::new(),
        Err(e) => return Err(e),
    };
    Ok(parse_log(&out))
}

fn parse_log(out: &str) -> Vec<Commit> {
    out.lines()
        .filter_map(|line| {
            let mut f = line.splitn(7, SEP);
            Some(Commit {
                hash: f.next()?.to_string(),
                short: f.next()?.to_string(),
                parents: f.next()?.split_whitespace().map(str::to_string).collect(),
                author: f.next()?.to_string(),
                date: f.next()?.to_string(),
                refs: f.next()?.to_string(),
                subject: f.next().unwrap_or_default().to_string(),
            })
        })
        .collect()
}

/// The files `commit` changed, compared with its first parent (everything,
/// for a root commit).
pub fn commit_files(git: &Git, commit: &Commit) -> Result<Vec<CommitFile>, GitError> {
    let parent = commit.parents.first().map(|p| p.as_str());
    let mut args = vec!["diff-tree", "-r", "-M", "--name-status", "-z", "--no-commit-id"];
    match parent {
        Some(p) => args.extend([p, commit.hash.as_str()]),
        None => args.extend(["--root", commit.hash.as_str()]),
    }
    let out = git.run(&args)?;
    Ok(parse_name_status(&out, &git.toplevel))
}

/// `M\0path\0R100\0old\0new\0…` → files.
pub(crate) fn parse_name_status(out: &str, top: &Path) -> Vec<CommitFile> {
    let mut parts = out.split('\0').filter(|s| !s.is_empty());
    let mut files = Vec::new();
    while let Some(status) = parts.next() {
        let kind = status.chars().next().unwrap_or('M');
        let file = if matches!(kind, 'R' | 'C') {
            let (Some(old), Some(new)) = (parts.next(), parts.next()) else {
                break;
            };
            CommitFile {
                status: kind,
                path: top.join(new),
                old_path: Some(top.join(old)),
            }
        } else {
            let Some(path) = parts.next() else { break };
            CommitFile {
                status: kind,
                path: top.join(path),
                old_path: None,
            }
        };
        files.push(file);
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_log_lines() {
        let out = "aaa\u{1f}a1\u{1f}p1 p2\u{1f}Ann\u{1f}2026-10-01\u{1f}HEAD -> main\u{1f}Merge: x\u{1f}y\n\
                   bbb\u{1f}b1\u{1f}\u{1f}Bob\u{1f}2026-09-30\u{1f}\u{1f}first\n";
        let log = parse_log(out);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].parents, vec!["p1", "p2"]);
        assert_eq!(log[0].refs, "HEAD -> main");
        assert_eq!(log[0].subject, "Merge: x\u{1f}y", "the subject keeps any separator");
        assert!(log[1].parents.is_empty(), "root commit");
        assert_eq!(log[1].subject, "first");
    }

    #[test]
    fn parses_name_status_with_renames() {
        let top = Path::new("/r");
        let out = "M\0src/a.rs\0R087\0old.rs\0new.rs\0A\0b.txt\0D\0gone.txt\0";
        let files = parse_name_status(out, top);
        assert_eq!(
            files,
            vec![
                CommitFile {
                    status: 'M',
                    path: top.join("src/a.rs"),
                    old_path: None
                },
                CommitFile {
                    status: 'R',
                    path: top.join("new.rs"),
                    old_path: Some(top.join("old.rs"))
                },
                CommitFile {
                    status: 'A',
                    path: top.join("b.txt"),
                    old_path: None
                },
                CommitFile {
                    status: 'D',
                    path: top.join("gone.txt"),
                    old_path: None
                },
            ]
        );
    }
}
