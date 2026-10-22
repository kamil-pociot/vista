//! Editing from a diff: `e` keeps the diff, side by side (HEAD read-only on
//! the left, the working file editable on the right), like IntelliJ.

use std::time::{Duration, Instant};

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");
const MINIMAL: &str = "[editor]\nconfig = \"minimal\"\n";

const OLD: &str = "one\ntwo\nthree\ngone\nfour\n";
const NEW: &str = "one\nTWO\nthree\nfour\n";

fn wait_for_file(repo: &Repo, rel: &str, pred: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let s = repo.read(rel);
        if pred(&s) || Instant::now() > deadline {
            return s;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn open_diff(vista: &mut Vista) {
    vista.expect_text("notes.txt");
    vista.press("alt+0");
    vista.expect_text("Changes  ");
    let (row, col) = vista.expect_text_in("notes.txt", 0..44).position();
    vista.click(col, row);
    vista.press("enter");
    vista.expect_text("Δ notes.txt");
    vista.expect_text("change 1 of 2");
}

/// A screen row holding both `left` and `right` (the two sides line up).
fn expect_side_by_side(vista: &mut Vista, left: &'static str, right: &'static str) {
    vista.wait_until(&format!("{left} | {right} on one row"), move |s| {
        s.contents()
            .lines()
            .any(|l| l.find(left).zip(l.rfind(right)).is_some_and(|(a, b)| a < b))
    });
}

#[test]
fn edit_in_the_diff_side_by_side() {
    if !vista_nvim::available("nvim") {
        eprintln!("nvim not installed; skipping");
        return;
    }
    let repo = Fixture::git()
        .file("notes.txt", OLD)
        .file("other.txt", "x\n")
        .commit("init")
        .modify("notes.txt", NEW)
        .build();
    let mut vista = Vista::launch(BIN, &repo).config(MINIMAL).size(150, 40).start();
    open_diff(&mut vista);

    // `e`: Neovim's diff mode, HEAD on the left, the working file on the right.
    vista.press("e");
    vista.expect_text("HEAD ");
    expect_side_by_side(&mut vista, "two", "TWO");
    expect_side_by_side(&mut vista, "three", "three");
    // vista's diff colours: the changed word, and the line only in HEAD
    // reads as deleted (not as added).
    vista.expect_text("TWO").to_have_bg("diff.word.modified");
    vista.expect_text("gone").to_have_bg("diff.deleted");

    // The cursor starts on the change; edit and save the working file.
    vista.type_text("ccEDITED");
    vista.press("esc");
    expect_side_by_side(&mut vista, "two", "EDITED");
    vista.type_text(":w");
    vista.press("enter");
    let content = wait_for_file(&repo, "notes.txt", |s| s.contains("EDITED"));
    assert_eq!(content, "one\nEDITED\nthree\nfour\n");

    // `:q` on the working side closes both, back to vista's diff, which
    // shows the saved edit.
    vista.type_text(":q");
    vista.press("enter");
    vista.expect_text("Diff editor closed");
    vista.expect_text("change 1 of 2");
    expect_side_by_side(&mut vista, "two", "EDITED");

    // And again: `e` reopens the side-by-side editor.
    vista.press("e");
    vista.expect_text("HEAD ");
    expect_side_by_side(&mut vista, "two", "EDITED");
}

#[test]
fn editing_a_plain_file_closes_the_diff_editor() {
    if !vista_nvim::available("nvim") {
        eprintln!("nvim not installed; skipping");
        return;
    }
    let repo = Fixture::git()
        .file("notes.txt", OLD)
        .file("other.txt", "plain file\n")
        .commit("init")
        .modify("notes.txt", NEW)
        .build();
    let mut vista = Vista::launch(BIN, &repo).config(MINIMAL).size(150, 40).start();
    open_diff(&mut vista);
    vista.press("e");
    expect_side_by_side(&mut vista, "two", "TWO");
    // Back to the tree, edit another file: a single window, no HEAD side.
    vista.press("ctrl+\\");
    vista.press("alt+1");
    let (row, col) = vista.expect_text_in("other.txt", 0..44).position();
    vista.click(col, row);
    vista.press("e");
    vista.expect_text("plain file");
    vista.expect_no_text("HEAD ");
    vista.expect_no_text("TWO");
}
