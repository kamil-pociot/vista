//! Repositories cloned inside the project (an ignored `repos/` folder, an
//! untracked `vendor/lib`): their files take their colours from their own
//! repository, and the Commit window keeps each repository's changes apart.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn project() -> Repo {
    Fixture::git()
        .file(".gitignore", "repos/\n")
        .file("main.txt", "main\n")
        .commit("init")
        .modify("main.txt", "main changed\n")
        .nested_repo("repos/foo")
        .file("repos/foo/x.txt", "first version\n")
        .file("repos/foo/plain.txt", "untouched\n")
        .file("repos/foo/.gitignore", "build/\n")
        .commit_in("repos/foo", "foo init")
        .run(&["-C", "repos/foo", "switch", "-q", "-c", "dev"])
        .modify("repos/foo/x.txt", "second version\n")
        .file("repos/foo/new.txt", "new\n")
        .file("repos/foo/build/out.o", "binary\n")
        .file("repos/notes.txt", "beside the clones\n")
        .nested_repo("vendor/lib")
        .file("vendor/lib/z.txt", "z\n")
        .build()
}

#[test]
fn files_take_the_colours_of_their_own_repository() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    // The ignored folder holding clones shows their changes.
    vista.expect_text_in("repos", 0..40).to_have_fg("git.modified");
    vista.expect_text_in("main.txt", 0..40).to_have_fg("git.modified");
    vista.press("down"); // repos
    vista.press("right");
    vista.expect_text_in("notes.txt", 0..40).to_have_fg("git.ignored");
    vista.expect_text_in("foo", 0..40).to_have_fg("git.modified");
    vista.press_all(&["down", "right"]); // foo
    vista.expect_text_in("x.txt", 0..40).to_have_fg("git.modified");
    vista.expect_text_in("new.txt", 0..40).to_have_fg("git.unversioned");
    vista.expect_text_in("plain.txt", 0..40).to_have_fg("ui.text");
    vista.expect_text_in("build", 0..40).to_have_fg("git.ignored");
    // Counts cover every repository.
    vista.expect_text("2 changed · 2 unversioned");
}

#[test]
fn the_commit_window_splits_changes_by_repository_and_commits_each() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    vista.expect_text("main.txt");
    vista.press("alt+2");
    vista.expect_text("Changes  2 files");
    // Each repository's row, with its branch.
    vista.expect_text("repos/foo  dev  1 file");
    vista.expect_text("vendor/lib  main  1 file");
    vista.expect_text("[x] x.txt").to_have_fg("ui.text");
    vista.expect_text("[ ] z.txt");

    vista.press("tab");
    vista.type_text("Change both");
    vista.press("ctrl+k");
    vista.expect_text("Committed 2 files in 2 repositories");
    let subject = |args: &[&str]| repo.git(args).trim().to_string();
    assert_eq!(subject(&["log", "-1", "--format=%s"]), "Change both");
    assert_eq!(subject(&["-C", "repos/foo", "log", "-1", "--format=%s"]), "Change both");
    assert_eq!(subject(&["-C", "repos/foo", "log", "-1", "--format=%D"]), "HEAD -> dev");
    // Only the untracked files are left.
    vista.expect_no_text("Changes  ");
    vista.expect_text("Unversioned Files  2 files");
}

#[test]
fn a_nested_file_diffs_against_its_own_repository() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(130, 40).start();
    vista.expect_text("main.txt");
    vista.press("alt+2");
    let (row, col) = vista.expect_text_in("x.txt", 0..40).position();
    vista.double_click(col, row);
    vista.expect_text("change 1 of 1");
    // HEAD of repos/foo on the left, not "everything is new".
    vista.expect_text("1 first version");
    vista.expect_text("1 second version");
}
