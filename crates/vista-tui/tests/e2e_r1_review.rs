//! The Review panel (`vista --review <rev>`): everything that differs
//! between a base and the working tree, each file a read-only diff.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// `main` has a.txt and b.txt; `work` commits a change to a.txt and renames
/// b.txt → c.txt, then changes a.txt again (uncommitted) and adds d.txt.
fn repo() -> Repo {
    Fixture::git()
        .file("a.txt", "one\n")
        .file("b.txt", "bee\n")
        .commit("start")
        .run(&["switch", "-q", "-c", "work"])
        .modify("a.txt", "one\ntwo\n")
        .run(&["mv", "b.txt", "c.txt"])
        .commit("agent: a grows, b becomes c")
        .modify("a.txt", "one\ntwo\nthree\n")
        .file("d.txt", "dee\n")
        .build()
}

#[test]
fn review_a_branch_since_main() {
    let repo = repo();
    let short = repo.git(&["rev-parse", "--short", "main"]).trim().to_string();
    let mut vista = Vista::launch(BIN, &repo)
        .arg("--review")
        .arg("main")
        .arg("--review-label")
        .arg("fix auth")
        .size(150, 40)
        .start();
    vista.expect_text("R: Review");
    vista.expect_text("fix auth");
    vista.expect_text(&format!("Review · 3 files since {short} · 1 commit"));
    // Committed and uncommitted alike, with the Commit window's colours.
    vista.expect_text_in("a.txt", 0..40).to_have_fg("git.modified");
    vista.expect_text_in("c.txt ← b.txt", 0..40);
    vista.expect_text_in("c.txt", 0..40).to_have_fg("git.renamed");
    vista.expect_text_in("d.txt", 0..40).to_have_fg("git.added");

    // Enter: main's a.txt ⇄ the working tree (both edits).
    vista.press("enter");
    vista.expect_text(&format!("Δ a.txt ⇠ {short}"));
    vista.expect_text("review base (read-only)");
    vista.expect_text("change 1 of 1");
    vista.expect_text("two").to_have_bg("diff.inserted");
    vista.expect_text("three").to_have_bg("diff.inserted");
    // Nothing to tick.
    vista.press("space");
    vista.expect_text("read-only");
    vista.expect_no_text("☑");

    // J: the next review file (the rename), K back.
    vista.press("J");
    vista.expect_text(&format!("Δ c.txt ⇠ {short}"));
    vista.press("K");
    vista.expect_text(&format!("Δ a.txt ⇠ {short}"));
    vista.expect_text("change 1 of 1");

    // The agent keeps working: the diff and the list follow; the base stays.
    repo.write("a.txt", "one\ntwo\nthree\nfour\n");
    repo.write("e.txt", "ee\n");
    vista.expect_text("four").to_have_bg("diff.inserted");
    vista.expect_text(&format!("Review · 4 files since {short} · 1 commit"));
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "agent: more"]);
    // Too long for the panel: the count goes below.
    vista.expect_text(&format!("Review · 4 files since {short}"));
    vista.expect_text_in("2 commits", 0..40);
}

#[test]
fn leave_the_review_and_bring_it_back() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo)
        .arg("--review")
        .arg("main")
        .size(150, 40)
        .start();
    vista.expect_text("R: Review");
    vista.expect_text_in("d.txt", 0..40);
    // Esc: the Project tree; Alt-R: the review again.
    vista.press("esc");
    vista.expect_text("1: Project");
    vista.press("alt+r");
    vista.expect_text("R: Review");
    vista.press("alt+2");
    vista.expect_text("2: Commit");
    vista.press("alt+r");
    vista.expect_text("R: Review");
    // Double-click opens a file's diff.
    let (row, col) = vista.expect_text_in("d.txt", 0..40).position();
    vista.double_click(col + 1, row);
    vista.expect_text("Δ d.txt ⇠");
    vista.expect_text("dee").to_have_bg("diff.inserted");
}

#[test]
fn review_of_head_is_the_commit_window_and_a_bad_rev_says_why() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo)
        .arg("--review")
        .arg("HEAD")
        .size(150, 40)
        .start();
    vista.expect_text("2: Commit");
    vista.expect_no_text("R: Review");
    // Nothing to bring back.
    vista.press("alt+r");
    vista.expect_text("No review: open one with `vista --review <rev>`");
    drop(vista);

    let mut vista = Vista::launch(BIN, &repo)
        .arg("--review")
        .arg("nope")
        .size(150, 40)
        .start();
    vista.expect_text("`nope` is not a commit here");
    vista.expect_text("1: Project");
}
