//! The Git tool window (Alt-9): commit log, files per commit, read-only
//! diffs, and one file's history (Alt-H).

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// Three commits: add a.txt and b.txt; change a.txt; rename b.txt → c.txt.
fn repo() -> Repo {
    Fixture::git()
        .file("a.txt", "one\n")
        .file("b.txt", "bee\n")
        .commit("first commit")
        .modify("a.txt", "one\ntwo\n")
        .commit("second: a grows")
        .run(&["mv", "b.txt", "c.txt"])
        .commit("third: rename b")
        .build()
}

#[test]
fn browse_the_log_and_open_a_read_only_diff() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
    vista.expect_text("a.txt");
    vista.press("alt+9");
    vista.expect_text("9: Git · Log");
    vista.expect_text("3 commits");
    // Newest first; the selected commit's files on the right.
    vista.expect_text("third: rename b");
    vista.expect_text("second: a grows");
    vista.expect_text("first commit");
    vista.expect_text("R c.txt  ← b.txt");

    // The second commit changed a.txt; Enter goes to its files, Enter opens.
    vista.press("down");
    vista.expect_text("M a.txt");
    vista.press("enter");
    vista.press("enter");
    let short = repo.git(&["rev-parse", "--short", "HEAD~1"]).trim().to_string();
    vista.expect_text(&format!("Δ a.txt @{short}"));
    vista.expect_text("parent (read-only)");
    vista.expect_text("second: a grows (read-only)");
    vista.expect_text("read-only   [ ◂ change 1 of 1 ▸ ]");
    vista.expect_text("two").to_have_bg("diff.inserted");
    // Nothing to tick: it's history.
    vista.press("space");
    vista.expect_text("History diffs are read-only");
    vista.expect_no_text("☑");

    // Alt-H: just a.txt's history (2 commits); `a` widens to all.
    vista.press("alt+h");
    vista.expect_text("History of a.txt");
    vista.expect_text("2 commits");
    vista.expect_no_text("third: rename b");
    vista.press("a");
    vista.expect_text("3 commits");
    vista.expect_text("third: rename b");
    // Esc hides the window.
    vista.press("esc");
    vista.expect_no_text("9: Git · Log");
}

#[test]
fn double_click_a_file_and_step_through_the_commits_files() {
    let repo = Fixture::git()
        .file("x.txt", "x1\n")
        .file("y.txt", "y1\n")
        .commit("start")
        .modify("x.txt", "x2\n")
        .modify("y.txt", "y2\n")
        .commit("both change")
        .build();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
    vista.expect_text("x.txt");
    vista.press("alt+9");
    vista.expect_text("both change");
    vista.expect_text("M x.txt");
    let (row, col) = vista.expect_text("M x.txt").position();
    vista.double_click(col + 2, row);
    vista.expect_text("Δ x.txt @");
    vista.expect_text("change 1 of 1");
    // J goes to the commit's next file, still in history.
    vista.press("J");
    vista.expect_text("Δ y.txt @");
    vista.expect_text("y2");
    vista.expect_text("read-only");
}
