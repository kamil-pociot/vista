//! First feedback round: long lines (scroll sideways / wrap), Markdown tables,
//! scrolling past the end, double clicks, and stepping through every change
//! across files with keys that aren't F-keys.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// A line far wider than any view: `start word word … TAILMARK`.
fn long_line(tail: &str) -> String {
    format!("start {}{tail}", "word ".repeat(60))
}

/// Opens the Commit window and shows the diff of `name`.
fn diff_of(vista: &mut Vista, name: &str) {
    vista.expect_text(name);
    vista.press("alt+0");
    vista.expect_text("Changes  ");
    let (row, col) = vista.expect_text_in(name, 0..44).position();
    vista.click(col, row);
    vista.press("enter");
    vista.expect_text(&format!("Δ {name}"));
}

fn long_line_repo() -> Repo {
    Fixture::git()
        .file(
            "wide.rs",
            format!("fn a() {{}}\n{}\nfn b() {{}}\n", long_line("OLDTAIL")),
        )
        .commit("init")
        .modify(
            "wide.rs",
            format!("fn a() {{}}\n{}\nfn b() {{}}\n", long_line("TAILMARK")),
        )
        .build()
}

#[test]
fn diff_scrolls_sideways_and_wraps() {
    let repo = long_line_repo();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
    diff_of(&mut vista, "wide.rs");
    vista.expect_text("lines: scroll");
    vista.expect_text("start word");
    vista.expect_no_text("TAILMARK");
    // Keys: both sides move together.
    for _ in 0..3 {
        vista.press("right");
    }
    vista.expect_no_text("start word");
    // Trackpad swipes reach the end of the line; it stops there.
    for _ in 0..80 {
        vista.scroll_right(100, 20);
    }
    vista.expect_text("TAILMARK");
    vista.expect_text("OLDTAIL");
    for _ in 0..80 {
        vista.scroll_left(100, 20);
    }
    vista.expect_text("start word");
    // `w` wraps instead: the start and the end are both on screen.
    vista.press("w");
    vista.expect_text("lines: wrap");
    vista.expect_text("start word");
    vista.expect_text("TAILMARK");
    vista.expect_text("OLDTAIL");
    // Ignore-whitespace moved to `i`.
    vista.press("i");
    vista.expect_text("whitespace: ignore");
}

#[test]
fn viewer_scrolls_sideways_with_shift_wheel_and_wraps() {
    let repo = Fixture::git()
        .file("wide.rs", format!("fn a() {{}}\n{}\n", long_line("TAILMARK")))
        .commit("init")
        .build();
    let mut vista = Vista::launch(BIN, &repo).size(120, 30).open("wide.rs").start();
    vista.expect_text("start word");
    vista.expect_no_text("TAILMARK");
    for _ in 0..80 {
        vista.shift_scroll_down(90, 10);
    }
    vista.expect_text("TAILMARK");
    vista.press("w");
    vista.expect_text("Long lines: wrap");
    vista.expect_text("start word");
    vista.expect_text("TAILMARK");
    vista.press("w");
    vista.expect_text("Long lines: scroll sideways");
}

#[test]
fn markdown_tables_wrap_cells_or_keep_full_width() {
    let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega";
    let repo = Fixture::git()
        .file(
            "README.md",
            format!("# Notes\n\n| Key | Description |\n|-----|-------------|\n| a | {long} |\n| b | short |\n"),
        )
        .commit("init")
        .build();
    let mut vista = Vista::launch(BIN, &repo).size(100, 30).open("README.md").start();
    vista.expect_text("Notes");
    // Wrapped (default): every word is shown, rows are separated by rules.
    vista.expect_text("omega");
    vista.expect_text("╞");
    vista.expect_text("├");
    vista.expect_no_text("…");
    // Full width: the row is one line, and the end of it is off to the right.
    vista.press("w");
    vista.expect_text("scroll sideways");
    vista.expect_text("alpha beta gamma delta");
    vista.expect_no_text("omega");
    for _ in 0..40 {
        vista.scroll_right(70, 10);
    }
    vista.expect_text("psi omega");
}

#[test]
fn scrolling_stops_a_little_past_the_end() {
    let text: String = (1..=100).map(|i| format!("L{i:03}\n")).collect();
    let repo = Fixture::git().file("long.txt", text).commit("init").build();
    let mut vista = Vista::launch(BIN, &repo).size(100, 30).open("long.txt").start();
    vista.expect_text("L001");
    for _ in 0..60 {
        vista.scroll_down(70, 10);
    }
    vista.expect_no_text("L001");
    let (row, _) = vista.expect_text("L100").position();
    // The last line rests well above the bottom edge, not at it…
    assert!(row + 6 < 28, "last line at row {row}:\n{}", vista.screen_text());
    // …and stays on screen however far you scroll.
    for _ in 0..20 {
        vista.scroll_down(70, 10);
    }
    let (again, _) = vista.expect_text("L100").position();
    assert_eq!(again, row);
}

#[test]
fn double_click_opens_from_the_commit_list() {
    let repo = Fixture::git()
        .file("notes.txt", "one\n")
        .commit("init")
        .modify("notes.txt", "two\n")
        .build();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
    vista.expect_text("notes.txt");
    vista.press("alt+0");
    vista.expect_text("Changes  1 file");
    let (row, col) = vista.expect_text_in("notes.txt", 0..44).position();
    vista.double_click(col, row);
    vista.expect_text("Δ notes.txt");
}

fn two_files() -> Repo {
    let b_old: String = (1..=30).map(|i| format!("b{i}\n")).collect();
    let b_new = b_old.replace("b2\n", "B2\n").replace("b25\n", "B25\n");
    let a_old: String = (1..=20).map(|i| format!("a{i}\n")).collect();
    Fixture::git()
        .file("a.txt", &a_old)
        .file("b.txt", &b_old)
        .commit("init")
        .modify("a.txt", a_old.replace("a5\n", "A5\n"))
        .modify("b.txt", b_new)
        .build()
}

#[test]
fn next_change_continues_into_the_next_file() {
    let repo = two_files();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
    vista.expect_text("a.txt");
    diff_of(&mut vista, "a.txt");
    vista.expect_text("change 1 of 1");
    vista.expect_text("[ ◂ change 1 of 1 ▸ ]");
    // At the last change: a popup offers the next file; pressing again goes.
    vista.press("]");
    vista.expect_text("Last change in this file. Press ] again to go to b.txt");
    vista.press("]");
    vista.expect_text("Δ b.txt");
    vista.expect_text("change 1 of 2");
    vista.expect_no_text("Press ] again");
    vista.press("]");
    vista.expect_text("change 2 of 2");
    // Going back past the first change offers the previous file…
    vista.press("[");
    vista.expect_text("change 1 of 2");
    vista.press("[");
    vista.expect_text("First change in this file. Press [ again to go to a.txt");
    // …any other key dismisses the offer…
    vista.press("j");
    vista.expect_no_text("Press [ again");
    vista.press("[");
    vista.press("[");
    vista.expect_text("Press [ again to go to a.txt");
    // …and a second press lands on that file's last change.
    vista.press("[");
    vista.expect_text("Δ a.txt");
    vista.expect_text("change 1 of 1");
}

#[test]
fn next_change_in_the_viewer_opens_the_next_changed_file() {
    let repo = two_files();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).open("a.txt").start();
    vista.expect_text("A5");
    // Wait for git: the gutter marks the change and the tree colours files.
    vista.expect_text("▌");
    vista.expect_text("b.txt").to_have_fg("git.modified");
    vista.press("]");
    vista.press("]");
    vista.expect_text("Press ] again to go to b.txt");
    vista.press("]");
    vista.expect_text("B2");
    vista.expect_no_text("Press ] again");
}
