//! M7 end-to-end: the IntelliJ-style diff viewer.

use vista_e2e::{Fixture, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// Opens the Commit window and shows the diff of `name`.
fn diff_of(vista: &mut Vista, name: &str) {
    vista.expect_text("README.md");
    vista.press("alt+0");
    vista.expect_text("Unversioned Files");
    let (row, col) = vista.expect_text_in(name, 0..44).position();
    vista.click(col, row);
    vista.press("enter");
    vista.expect_text(&format!("Δ {name}"));
}

#[test]
fn side_by_side_with_word_highlights_and_navigation() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(160, 44).start();
    diff_of(&mut vista, "status.rs");
    vista.expect_text("(read-only)");
    vista.expect_text("Working tree");
    vista.expect_text("change 1 of 5");
    // Added line: right side green, left side filler.
    vista.expect_text("use anyhow::Context;").to_have_bg("diff.inserted");
    vista.expect_text("╱╱╱╱");
    // Modified line: the changed identifier is highlighted on both sides.
    vista.expect_text("work_dir").to_have_bg("diff.word.modified");
    vista.expect_text("workdir").to_have_bg("diff.word.modified");
    // F7 / Shift-F7 move between changes.
    vista.press("f7");
    vista.expect_text("change 2 of 5");
    vista.press("f7");
    vista.expect_text("change 3 of 5");
    vista.press("shift+f7");
    vista.expect_text("change 2 of 5");
}

#[test]
fn unified_whitespace_and_collapse_toggles() {
    let repo = Fixture::git()
        .file("a.rs", (0..60).map(|i| format!("    line {i}\n")).collect::<String>())
        .commit("init")
        .modify(
            "a.rs",
            (0..60)
                .map(|i| match i {
                    10 => "  line 10\n".to_string(),
                    40 => "    line forty\n".to_string(),
                    _ => format!("    line {i}\n"),
                })
                .collect::<String>(),
        )
        .file("README.md", "x")
        .build();
    let mut vista = Vista::launch(BIN, &repo).size(160, 44).start();
    diff_of(&mut vista, "a.rs");
    vista.expect_text("change 1 of 2");
    // Ignore whitespace: the indentation-only change disappears.
    vista.press("i");
    vista.expect_text("whitespace: ignore");
    vista.expect_text("change 1 of 1");
    vista.press("i");
    // Collapse unchanged regions.
    vista.press("z");
    vista.expect_text("unchanged lines");
    // Unified view: old line marked -, new line +.
    vista.press("u");
    vista.expect_text("unified");
    vista.expect_text("- ").to_have_bg("diff.deleted");
    vista.expect_text("line forty");
}

#[test]
fn new_deleted_and_image_files() {
    let repo = Fixture::rust_project().delete("docs/guide.md").build();
    let mut vista = Vista::launch(BIN, &repo).size(160, 44).start();
    // Added file: everything on the right.
    diff_of(&mut vista, "ROADMAP.md");
    vista.expect_text("# Roadmap").to_have_bg("diff.inserted");
    // Deleted file: everything on the left.
    vista.press("alt+0");
    let (row, col) = vista.expect_text_in("guide.md", 0..44).position();
    vista.click(col, row);
    vista.press("enter");
    vista.expect_text("Δ guide.md");
    vista.expect_text("# Guide").to_have_bg("diff.deleted");
    // Image: both sides as pictures (HEAD absent for a new image).
    vista.press("alt+0");
    let (row, col) = vista.expect_text_in("screenshot.png", 0..44).position();
    vista.click(col, row);
    vista.press("enter");
    vista.expect_text("Δ screenshot.png");
    vista.expect_text("(not present)");
    vista.expect_text("▀");
}

#[test]
fn next_file_open_line_and_live_updates() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(160, 44).start();
    diff_of(&mut vista, "diff.rs");
    vista.expect_text("pub fn diff() -> usize { 0 }");
    // J: next file in the Commit list (status.rs follows diff.rs).
    vista.press("shift+j");
    vista.expect_text("Δ status.rs");
    vista.expect_text("use anyhow::Context;");
    vista.press("shift+k");
    vista.expect_text("Δ diff.rs");
    // The diff follows edits on disk.
    repo.write("crates/fb-git/src/diff.rs", "pub fn diff() -> usize { 42 }\n");
    vista.expect_text("{ 42 }");
    // Enter opens the working file at the cursor's line.
    vista.press("enter");
    vista.expect_text(" diff.rs ");
    vista.expect_text("1:1");
    // `d` in the viewer shows the diff again.
    vista.press("d");
    vista.expect_text("Δ diff.rs");
}
