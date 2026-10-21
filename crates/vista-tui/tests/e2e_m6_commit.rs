//! M6 end-to-end: the Commit tool window.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn open_commit(vista: &mut Vista) {
    vista.expect_text("README.md");
    vista.press("alt+0");
    vista.expect_text("Unversioned Files");
}

/// Selects a file row in the commit list by clicking its name.
fn select(vista: &mut Vista, name: &str) {
    let (row, col) = vista.expect_text_in(name, 0..40).position();
    vista.click(col, row);
}

fn subject(repo: &Repo) -> String {
    repo.git(&["log", "-1", "--format=%s"]).trim().to_string()
}

#[test]
fn shows_groups_counts_and_default_ticks() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    open_commit(&mut vista);
    vista.expect_text("Changes  4 files");
    vista.expect_text("Unversioned Files  2 files");
    vista.expect_text("[x] status.rs").to_have_fg("ui.text");
    vista.expect_text("status.rs").to_have_fg("git.modified");
    vista.expect_text("ROADMAP.md").to_have_fg("git.added");
    vista.expect_text("[ ] watch.rs");
    vista.expect_text("4 of 6 files");
    // Group by directory.
    vista.press("g");
    vista.expect_text("crates/fb-git/src");
}

#[test]
fn commits_only_ticked_files() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    open_commit(&mut vista);
    select(&mut vista, "diff.rs");
    vista.press("space");
    vista.expect_text("[ ] diff.rs");
    vista.expect_text("3 of 6 files");
    vista.press("tab");
    vista.type_text("Fix status refresh");
    vista.press("ctrl+k");
    vista.expect_text("Committed 3 files");
    assert_eq!(subject(&repo), "Fix status refresh");
    let st = repo.status();
    assert!(st.contains(&" M crates/fb-git/src/diff.rs".to_string()), "{st:?}");
    assert!(
        !st.iter()
            .any(|l| l.ends_with("status.rs") || l.ends_with("ROADMAP.md") || l.ends_with("Cargo.lock")),
        "{st:?}"
    );
    // The list refreshes: only diff.rs remains in Changes.
    vista.expect_text("Changes  1 file");
}

#[test]
fn amend_prefills_the_last_message() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    open_commit(&mut vista);
    vista.press("m");
    vista.expect_text("[x] Amend");
    vista.expect_text("Initial commit");
    vista.press("ctrl+k");
    vista.expect_text("Committed 4 files");
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]).trim(), "1");
    assert_eq!(subject(&repo), "Initial commit");
    assert_eq!(repo.git(&["show", "HEAD:Cargo.lock"]), "# lock v2\n");
}

#[test]
fn failing_hook_output_is_shown() {
    let repo = Fixture::rust_project().build();
    let hook = repo.path().join(".git/hooks/pre-commit");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'lint failed: trailing whitespace in status.rs' >&2\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    open_commit(&mut vista);
    vista.press("tab");
    vista.type_text("Blocked by hook");
    vista.press("ctrl+k");
    vista.expect_text("Commit failed");
    vista
        .expect_text("lint failed: trailing whitespace")
        .to_have_fg("ui.error");
    assert_eq!(subject(&repo), "Initial commit");
}

#[test]
fn rollback_asks_for_confirmation() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    open_commit(&mut vista);
    select(&mut vista, "Cargo.lock");
    vista.press("r");
    vista.expect_text("Rollback 1 file? Local changes will be lost.");
    vista.press("n");
    vista.expect_no_text("Rollback 1 file?");
    assert_eq!(repo.read("Cargo.lock"), "# lock v2\n");
    vista.press("r");
    vista.press("y");
    vista.expect_text("Rolled back 1 file(s)");
    vista.expect_text("Changes  3 files");
    assert_eq!(repo.read("Cargo.lock"), "# lock v1\n");
}

#[test]
fn add_unversioned_to_vcs() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    open_commit(&mut vista);
    select(&mut vista, "watch.rs");
    vista.press("a");
    vista.expect_text("Added 1 file(s) to VCS");
    vista.expect_text("Changes  5 files");
    vista.expect_text("Unversioned Files  1 file");
    vista.expect_text_in("watch.rs", 0..40).to_have_fg("git.added");
}

#[test]
fn guards_empty_messages_and_ctrl_k_opens_the_window() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    vista.expect_text("README.md");
    // Ctrl-K from the tree opens the Commit window with the message focused.
    vista.press("ctrl+k");
    vista.expect_text("Unversioned Files");
    vista.press("ctrl+k");
    vista.expect_text("Commit message is empty");
    vista.type_text("Now with a message");
    vista.press("ctrl+k");
    vista.expect_text("Committed 4 files");
}

#[test]
fn commit_and_push() {
    let remote = Fixture::git().build();
    remote.git(&["config", "receive.denyCurrentBranch", "ignore"]);
    let repo = Fixture::rust_project().build();
    let url = remote.path().to_string_lossy().into_owned();
    repo.git(&["remote", "add", "origin", &url]);
    repo.git(&["push", "-q", "-u", "origin", "main"]);
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    open_commit(&mut vista);
    vista.press("tab");
    vista.type_text("Pushed from vista");
    vista.press("esc");
    vista.press("shift+p");
    vista.expect_text("Pushed to remote");
    assert_eq!(
        remote.git(&["log", "-1", "--format=%s", "main"]).trim(),
        "Pushed from vista"
    );
}
