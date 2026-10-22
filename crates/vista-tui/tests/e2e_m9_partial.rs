//! M9 end-to-end: partial commits (per-change checkboxes in the diff).

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

const OLD: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\neleven\ntwelve\n";
const NEW: &str = "one\nTWO\nthree\nfour\nfive\nSIX\nseven\neight\nnine\nten\nELEVEN\ntwelve\n";

fn repo() -> Repo {
    Fixture::git()
        .file("notes.txt", OLD)
        .file("other.txt", "x\n")
        .commit("init")
        .modify("notes.txt", NEW)
        .build()
}

fn open_diff(vista: &mut Vista) {
    vista.expect_text("notes.txt");
    vista.press("alt+2");
    vista.expect_text("Changes  1 file");
    let (row, col) = vista.expect_text_in("notes.txt", 0..44).position();
    vista.click(col, row);
    vista.press("enter");
    vista.expect_text("Δ notes.txt");
    vista.expect_text("3 of 3 in commit");
}

#[test]
fn commit_one_of_three_changes() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
    open_diff(&mut vista);
    // The diff opens on change 1; untick changes 2 and 3.
    vista.press("f7");
    vista.press("space");
    vista.expect_text("2 of 3 in commit");
    vista.press("f7");
    vista.press("space");
    vista.expect_text("1 of 3 in commit");
    vista.expect_text("☐");
    vista.press("home");
    vista.expect_text("☑");
    // The Commit list shows the file as partly included.
    vista.press("alt+2");
    vista.expect_text("[~] notes.txt");
    vista.press("tab");
    vista.type_text("Only the first change");
    vista.press("ctrl+k");
    vista.expect_text("Committed 1 file");
    // HEAD has exactly change 1; the working tree still has all three.
    assert_eq!(repo.git(&["show", "HEAD:notes.txt"]), OLD.replace("two", "TWO"));
    assert_eq!(repo.read("notes.txt"), NEW);
    let diff = repo.git(&["diff", "--unified=0", "HEAD", "--", "notes.txt"]);
    assert!(diff.contains("-six\n+SIX"), "{diff}");
    assert!(diff.contains("-eleven\n+ELEVEN"), "{diff}");
    assert!(!diff.contains("TWO"), "{diff}");
    // The remaining two changes are still waiting in the diff.
    vista.expect_text("2 of 2 in commit");
}

#[test]
fn ticking_a_change_of_an_unticked_file() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
    open_diff(&mut vista);
    // Untick the whole file in the Commit list, then tick only change 3.
    vista.press("alt+2");
    vista.press("space");
    vista.expect_text("[ ] notes.txt");
    vista.press("enter");
    vista.expect_text("0 of 3 in commit");
    vista.press("f7");
    vista.press("f7");
    vista.press("space");
    vista.expect_text("1 of 3 in commit");
    vista.press("alt+2");
    vista.expect_text("[~] notes.txt");
    vista.press("tab");
    vista.type_text("Just eleven");
    vista.press("ctrl+k");
    vista.expect_text("Committed 1 file");
    assert_eq!(repo.git(&["show", "HEAD:notes.txt"]), OLD.replace("eleven", "ELEVEN"));
    assert_eq!(repo.read("notes.txt"), NEW);
}

#[test]
fn picking_is_disabled_while_ignoring_whitespace() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
    open_diff(&mut vista);
    vista.press("i");
    vista.press("space");
    vista.expect_text("Turn off ignore-whitespace");
    vista.press("i");
    vista.press("space");
    vista.expect_text("2 of 3 in commit");
}
