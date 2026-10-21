//! Integration: commit, amend, rollback, add and push against real repos.

use vista_e2e::{Fixture, Repo};
use vista_git::write::{CommitContent, CommitFile};
use vista_git::{FileStatus, Git, StatusSnapshot};

fn git_of(repo: &Repo) -> Git {
    Git::discover(repo.path()).unwrap()
}

fn change(snap: &StatusSnapshot, git: &Git, rel: &str) -> vista_git::Change {
    snap.change(&git.toplevel.join(rel))
        .cloned()
        .unwrap_or_else(|| panic!("no change for {rel}"))
}

#[test]
fn commits_only_the_selected_files() {
    let repo = Fixture::rust_project().build();
    let git = git_of(&repo);
    let snap = StatusSnapshot::load(&git).unwrap();
    let files = vec![
        CommitFile::whole(&change(&snap, &git, "crates/fb-git/src/status.rs")),
        CommitFile::whole(&change(&snap, &git, "crates/fb-git/src/watch.rs")),
    ];
    let r = git.commit(&files, "Fix status refresh\n\nBody text.", false).unwrap();
    assert_eq!(r.subject, "Fix status refresh");
    assert_eq!(
        repo.git(&["log", "-1", "--format=%B"]).trim(),
        "Fix status refresh\n\nBody text."
    );
    let shown = repo.git(&["show", "--name-status", "--format=", "HEAD"]);
    assert_eq!(
        shown.lines().collect::<Vec<_>>(),
        vec!["M\tcrates/fb-git/src/status.rs", "A\tcrates/fb-git/src/watch.rs"]
    );
    // Everything else is untouched: diff.rs still modified, ROADMAP.md still staged.
    let st = repo.status();
    assert!(st.contains(&" M crates/fb-git/src/diff.rs".to_string()), "{st:?}");
    assert!(st.contains(&"A  ROADMAP.md".to_string()), "{st:?}");
    assert!(
        !st.iter().any(|l| l.contains("status.rs") || l.contains("watch.rs")),
        "{st:?}"
    );
}

#[test]
fn commits_deletions_renames_and_partial_content() {
    let repo = Fixture::git()
        .file("a.txt", "one\n")
        .file("b.txt", "two\n")
        .file("c.txt", "1\n2\n3\n")
        .commit("init")
        .run(&["mv", "a.txt", "renamed.txt"])
        .delete("b.txt")
        .modify("c.txt", "1\nTWO\n3\nfour\n")
        .build();
    let git = git_of(&repo);
    let snap = StatusSnapshot::load(&git).unwrap();
    let renamed = change(&snap, &git, "renamed.txt");
    assert_eq!(renamed.status, FileStatus::Renamed);
    let mut files = vec![
        CommitFile::whole(&renamed),
        CommitFile::whole(&change(&snap, &git, "b.txt")),
    ];
    files.push(CommitFile {
        path: git.toplevel.join("c.txt"),
        orig_path: None,
        content: CommitContent::Blob(b"1\nTWO\n3\n".to_vec()),
    });
    git.commit(&files, "Mixed", false).unwrap();
    assert_eq!(repo.git(&["show", "HEAD:c.txt"]), "1\nTWO\n3\n");
    assert!(!repo.try_git(&["cat-file", "-e", "HEAD:b.txt"]).0);
    assert_eq!(repo.git(&["show", "HEAD:renamed.txt"]), "one\n");
    // The rest of c.txt stays as a local change.
    assert_eq!(repo.status(), vec![" M c.txt".to_string()]);
    assert_eq!(repo.read("c.txt"), "1\nTWO\n3\nfour\n");
}

#[test]
fn amend_replaces_the_last_commit() {
    let repo = Fixture::rust_project().build();
    let git = git_of(&repo);
    assert_eq!(git.last_message().as_deref(), Some("Initial commit"));
    let snap = StatusSnapshot::load(&git).unwrap();
    let files = vec![CommitFile::whole(&change(&snap, &git, "Cargo.lock"))];
    git.commit(&files, "Initial commit, amended", true).unwrap();
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]).trim(), "1");
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]).trim(),
        "Initial commit, amended"
    );
    assert_eq!(repo.git(&["show", "HEAD:Cargo.lock"]), "# lock v2\n");
}

#[test]
fn first_commit_in_an_empty_repository() {
    let repo = Fixture::git().file("first.txt", "hi\n").build();
    let git = git_of(&repo);
    let snap = StatusSnapshot::load(&git).unwrap();
    let files = vec![CommitFile::whole(&change(&snap, &git, "first.txt"))];
    let r = git.commit(&files, "Root", false).unwrap();
    assert_eq!(r.subject, "Root");
    assert!(repo.status().is_empty());
}

#[test]
fn failing_hooks_abort_and_report_their_output() {
    let repo = Fixture::rust_project().build();
    let hook = repo.path().join(".git/hooks/pre-commit");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'lint failed: trailing whitespace' >&2\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let git = git_of(&repo);
    let snap = StatusSnapshot::load(&git).unwrap();
    let files = vec![CommitFile::whole(&change(&snap, &git, "Cargo.lock"))];
    let err = git.commit(&files, "Blocked", false).unwrap_err().to_string();
    assert!(err.contains("lint failed: trailing whitespace"), "{err}");
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]).trim(), "1");
    assert!(repo.status().contains(&" M Cargo.lock".to_string()));
}

#[test]
fn rollback_and_add() {
    let repo = Fixture::rust_project().build();
    let git = git_of(&repo);
    let snap = StatusSnapshot::load(&git).unwrap();
    git.rollback(&[change(&snap, &git, "Cargo.lock"), change(&snap, &git, "ROADMAP.md")])
        .unwrap();
    assert_eq!(repo.read("Cargo.lock"), "# lock v1\n");
    let st = repo.status();
    assert!(
        st.contains(&"?? ROADMAP.md".to_string()),
        "added → unversioned, file kept: {st:?}"
    );
    git.add(&[git.toplevel.join("ROADMAP.md")]).unwrap();
    assert!(repo.status().contains(&"A  ROADMAP.md".to_string()));
}

#[test]
fn push_to_a_remote() {
    let remote = Fixture::git().build();
    remote.git(&["config", "receive.denyCurrentBranch", "ignore"]);
    let repo = Fixture::git().file("x.txt", "1\n").commit("one").build();
    let url = remote.path().to_string_lossy().into_owned();
    repo.git(&["remote", "add", "origin", &url]);
    repo.git(&["push", "-q", "-u", "origin", "main"]);
    repo.write("x.txt", "2\n");
    let git = git_of(&repo);
    let snap = StatusSnapshot::load(&git).unwrap();
    git.commit(&[CommitFile::whole(&change(&snap, &git, "x.txt"))], "two", false)
        .unwrap();
    git.push().unwrap();
    assert_eq!(remote.git(&["log", "-1", "--format=%s", "main"]).trim(), "two");
}
