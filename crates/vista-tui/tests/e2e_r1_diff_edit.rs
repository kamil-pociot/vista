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

#[test]
fn diff_editor_follows_wrap_and_collapse_both_ways() {
    if !vista_nvim::available("nvim") {
        eprintln!("nvim not installed; skipping");
        return;
    }
    // 40 lines, changes at 2 and 30, and a long unchanged line 10.
    let old: String = (1..=40)
        .map(|i| match i {
            10 => format!("{}TAIL\n", "long ".repeat(40)),
            _ => format!("l{i}\n"),
        })
        .collect();
    let new = old.replace("l2\n", "L2\n").replace("l30\n", "L30\n");
    let repo = Fixture::git()
        .file("notes.txt", &old)
        .file("other.txt", "x\n")
        .commit("init")
        .modify("notes.txt", new)
        .build();
    let mut vista = Vista::launch(BIN, &repo).config(MINIMAL).size(150, 40).start();
    open_diff(&mut vista);
    vista.expect_text("lines: scroll");
    vista.expect_text("collapse: off");

    // vista → Neovim: no wrap and no folding, like the diff it came from
    // (Neovim's diff mode would fold unchanged lines and turn wrap off).
    vista.press("e");
    vista.expect_text("HEAD ");
    vista.expect_text("l15");
    vista.expect_no_text("+--");
    vista.expect_no_text("TAIL");

    // Neovim → vista: `:set wrap` wraps both sides and the viewer.
    vista.type_text(":set wrap");
    vista.press("enter");
    expect_side_by_side(&mut vista, "TAIL", "TAIL");
    vista.type_text(":q");
    vista.press("enter");
    vista.expect_text("Diff editor closed");
    vista.expect_text("lines: wrap");

    // Collapse in vista → Neovim folds unchanged lines.
    vista.press("z");
    vista.expect_text("collapse: on");
    vista.press("e");
    vista.expect_text("HEAD ");
    vista.expect_text("+--");
    vista.expect_no_text("l15");
    // `zi` in Neovim unfolds both sides, and turns vista's collapse off.
    vista.type_text("zi");
    vista.expect_text("l15");
    vista.expect_no_text("+--");
    vista.type_text(":q");
    vista.press("enter");
    vista.expect_text("Diff editor closed");
    vista.expect_text("collapse: off");
}

#[test]
fn plain_editing_follows_the_viewer_wrap_both_ways() {
    if !vista_nvim::available("nvim") {
        eprintln!("nvim not installed; skipping");
        return;
    }
    let text = format!("short\n{}TAIL\n", "long ".repeat(40));
    let repo = Fixture::git().file("long.txt", text).commit("init").build();
    let mut vista = Vista::launch(BIN, &repo)
        .config(MINIMAL)
        .size(120, 30)
        .open("long.txt")
        .start();
    vista.expect_text("short");
    vista.expect_no_text("TAIL");
    // Wrap in the viewer, then edit: Neovim wraps too.
    vista.press("w");
    vista.expect_text("TAIL");
    vista.press("e");
    vista.expect_text("~ ");
    vista.expect_text("TAIL");
    // `:set nowrap` in Neovim carries back to the viewer.
    vista.type_text(":set nowrap");
    vista.press("enter");
    vista.expect_no_text("TAIL");
    vista.type_text(":q");
    vista.press("enter");
    vista.expect_text("Editor closed");
    vista.expect_text("short");
    vista.expect_no_text("TAIL");
}
