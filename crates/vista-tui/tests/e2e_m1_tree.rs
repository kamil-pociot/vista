//! M1 end-to-end: the Project tree with git colours.

use vista_e2e::{Fixture, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// Expands crates → fb-git → src by keyboard.
fn open_src(vista: &mut Vista) {
    vista.expect_text("▸ crates");
    // Root is selected; move to `crates` (first child) and expand down to src.
    vista.press("down");
    vista.press("right");
    vista.expect_text("▸ fb-core");
    vista.press_all(&["down", "down"]); // fb-core, fb-git
    vista.press("right");
    vista.expect_text("▸ src");
    vista.press_all(&["down", "right"]); // folders first: src, then Cargo.toml
    vista.expect_text("status.rs");
}

#[test]
fn colours_files_and_folders_by_git_status() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("README.md");
    // Root-level entries.
    vista.expect_text("ROADMAP.md").to_have_fg("git.added");
    vista.expect_text("Cargo.lock").to_have_fg("git.modified");
    vista.expect_text("screenshot.png").to_have_fg("git.unversioned");
    vista.expect_text("README.md").to_have_fg("ui.text");
    vista.expect_text("target").to_have_fg("git.ignored");
    vista.expect_text("build.log").to_have_fg("git.ignored");
    // Folder roll-up.
    vista.expect_text("crates").to_have_fg("git.modified");
    vista.expect_text("docs").to_have_fg("ui.text");

    open_src(&mut vista);
    vista.expect_text("status.rs").to_have_fg("git.modified");
    vista.expect_text("diff.rs").to_have_fg("git.modified");
    vista.expect_text("watch.rs").to_have_fg("git.unversioned");
    vista.expect_text("lib.rs").to_have_fg("ui.text");

    // Branch and change counts.
    vista.expect_text("⎇ main");
    vista.expect_text("4 changed · 2 unversioned");
}

#[test]
fn keyboard_navigation_and_collapse_all() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).start();
    open_src(&mut vista);
    // Selection is on `src`; the selected row uses the focused selection colour.
    vista.expect_text("▾ src").to_have_bg("ui.selection");
    vista.press("left");
    vista.expect_text("▸ src");
    vista.press("left");
    vista.expect_text("▾ fb-git").to_have_bg("ui.selection");
    vista.press("-");
    vista.expect_text("▸ crates");
    vista.expect_no_text("fb-git");
    // Enter on a folder toggles it too.
    vista.press("down");
    vista.press("enter");
    vista.expect_text("fb-core");
    vista.press("enter");
    vista.expect_no_text("fb-core");
}

#[test]
fn mouse_selects_toggles_and_scrolls() {
    let mut f = Fixture::git();
    for i in 0..60 {
        f = f.file(&format!("many/file{i:02}.txt"), "x");
    }
    let repo = f.file("a.txt", "x").commit("init").build();
    let mut vista = Vista::launch(BIN, &repo).size(100, 24).start();
    vista.expect_text("▸ many");
    // Click the arrow of `many` (row 3 = header 1 + root row 2 → `many` at row 3; arrow at col 3).
    let (row, col) = vista.expect_text("▸ many").position();
    vista.click(col, row);
    vista.expect_text("file00.txt");
    // Clicking a row selects it.
    vista.click_text("file03.txt");
    vista.expect_text("file03.txt").to_have_bg("ui.selection");
    // Wheel scrolls the view.
    vista.scroll_down(10, 10);
    vista.scroll_down(10, 10);
    vista.scroll_down(10, 10);
    vista.expect_no_text("file00.txt");
    vista.expect_text("file12.txt");
    vista.scroll_up(10, 10);
    vista.scroll_up(10, 10);
    vista.scroll_up(10, 10);
    vista.expect_text("file00.txt");
    // Double-click on a folder collapses it.
    let (row, col) = vista.expect_text("▾ many").position();
    vista.double_click(col + 4, row);
    vista.expect_no_text("file00.txt");
}

#[test]
fn reflects_external_changes_without_a_keypress() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("README.md").to_have_fg("ui.text");

    // A new file appears, unversioned.
    repo.write("NEW_NOTES.md", "hello");
    vista.expect_text("NEW_NOTES.md").to_have_fg("git.unversioned");

    // A committed file is modified.
    repo.write("README.md", "# changed\n");
    vista.expect_text("README.md").to_have_fg("git.modified");

    // Committing outside vista resets the colours (watched via .git/index).
    repo.git(&["add", "README.md", "NEW_NOTES.md"]);
    repo.git(&["commit", "-q", "-m", "external"]);
    vista.expect_text("README.md").to_have_fg("ui.text");
    vista.expect_text("NEW_NOTES.md").to_have_fg("ui.text");

    // A deleted file disappears.
    repo.remove("NEW_NOTES.md");
    vista.expect_no_text("NEW_NOTES.md");
}

#[test]
fn ignored_files_can_be_hidden() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("build.log");
    vista.press("i");
    vista.expect_text("Ignored files hidden");
    vista.expect_no_text("build.log");
    vista.expect_no_text("▸ target");
    vista.press("i");
    vista.expect_text("build.log");
    vista.expect_text("▸ target");
}

#[test]
fn ignored_files_hidden_by_config() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo)
        .config("[tree]\nshow_ignored = false\n")
        .start();
    vista.expect_text("README.md");
    vista.settle(std::time::Duration::from_millis(300));
    vista.expect_no_text("build.log");
}

#[test]
fn works_outside_git() {
    let repo = Fixture::plain().file("docs/a.md", "x").file("notes.txt", "y").build();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("notes.txt").to_have_fg("ui.text");
    vista.expect_text("▸ docs");
    vista.expect_no_text("⎇");
}

#[test]
fn refresh_action_rescans() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).env("VISTA_NO_WATCH", "1").start();
    vista.expect_text("README.md");
    repo.write("LATE.txt", "x");
    vista.settle(std::time::Duration::from_millis(300));
    vista.expect_no_text("LATE.txt");
    vista.press("f5");
    vista.expect_text("LATE.txt").to_have_fg("git.unversioned");
}
