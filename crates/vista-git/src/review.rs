//! A review: every file that differs between a base revision and the
//! working tree (committed since the base, uncommitted, untracked), for
//! looking over what an agent changed on a branch.

use std::path::Path;

use crate::{CommitFile, Git, GitError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// The base, resolved once (it doesn't move as work goes on).
    pub base: String,
    pub short: String,
    /// The changed files: `M`, `A`, `D`, `R` (with the old path), `C`, `T`;
    /// untracked files are `A`.
    pub files: Vec<CommitFile>,
    /// Commits between the base and HEAD.
    pub commits: usize,
}

/// The commit `rev` names, as a full hash (`rev-parse --verify <rev>^{commit}`).
pub fn resolve(git: &Git, rev: &str) -> Result<String, GitError> {
    let spec = format!("{rev}^{{commit}}");
    match git.run(&["rev-parse", "--verify", "--quiet", &spec]) {
        Ok(hash) if !hash.trim().is_empty() => Ok(hash.trim().to_string()),
        _ => Err(GitError::Failed {
            args: format!("rev-parse {rev}"),
            stderr: format!("`{rev}` is not a commit here"),
        }),
    }
}

/// Whether `base` is HEAD: nothing committed to review (the Commit window
/// shows the rest).
pub fn is_head(git: &Git, base: &str) -> bool {
    git.run(&["rev-parse", "--verify", "--quiet", "HEAD"])
        .is_ok_and(|h| h.trim() == base)
}

impl Review {
    /// Lists what changed since `base` (a resolved hash).
    pub fn load(git: &Git, base: &str) -> Result<Review, GitError> {
        // Against the working tree, ignoring the index: staged or not, a
        // change is a change.
        let tracked = git.run(&["diff", "--name-status", "-z", "-M", base, "--"])?;
        let untracked = git.run(&["ls-files", "--others", "--exclude-standard", "-z"])?;
        let commits = git
            .run(&["rev-list", "--count", &format!("{base}..HEAD")])
            .ok()
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(0);
        let short = base.chars().take(7).collect();
        Ok(Review {
            base: base.to_string(),
            short,
            files: files(&tracked, &untracked, &git.toplevel),
            commits,
        })
    }
}

/// `git diff --name-status -z` output plus untracked paths → the files, in
/// path order.
fn files(tracked: &str, untracked: &str, top: &Path) -> Vec<CommitFile> {
    let mut files = crate::log::parse_name_status(tracked, top);
    files.extend(untracked.split('\0').filter(|p| !p.is_empty()).map(|p| CommitFile {
        status: 'A',
        path: top.join(p),
        old_path: None,
    }));
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracked_and_untracked_changes_make_the_list() {
        let top = Path::new("/r");
        let tracked = "M\0a.txt\0R100\0b.txt\0c.txt\0D\0gone.txt\0";
        let untracked = "d.txt\0new/e.txt\0";
        let f = files(tracked, untracked, top);
        let summary: Vec<(char, &str, Option<&str>)> = f
            .iter()
            .map(|f| {
                (
                    f.status,
                    f.path.strip_prefix(top).unwrap().to_str().unwrap(),
                    f.old_path
                        .as_deref()
                        .map(|p| p.strip_prefix(top).unwrap().to_str().unwrap()),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ('M', "a.txt", None),
                ('R', "c.txt", Some("b.txt")),
                ('A', "d.txt", None),
                ('D', "gone.txt", None),
                ('A', "new/e.txt", None),
            ]
        );
        assert!(files("", "", top).is_empty());
    }
}
