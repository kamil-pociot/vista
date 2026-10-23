//! Clickable labels: tool-window names, hints, toggles and buttons do what
//! their keys do.

use vista_e2e::{Fixture, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// Clicks the first cell of `text` (the first place it's on screen).
fn click_text(vista: &mut Vista, text: &str) {
    let (row, col) = vista.expect_text(text).position();
    vista.click(col, row);
}

#[test]
fn tool_windows_and_top_bar_hints_open_on_click() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(140, 40).start();
    vista.expect_text("README.md");

    // Bottom bar: the Commit window, then back to the Project.
    click_text(&mut vista, "2: Commit");
    vista.expect_text("Unversioned Files");
    click_text(&mut vista, "1: Project");
    vista.expect_no_text("Unversioned Files");

    // Top bar hints.
    click_text(&mut vista, "Go to File");
    vista.expect_text("include ignored (Alt-I)");
    vista.press("esc");
    vista.expect_no_text("include ignored");
    click_text(&mut vista, "Find in Files");
    vista.expect_text("File mask:");
    vista.press("esc");
    vista.expect_no_text("File mask:");

    // The branch opens the Git window; Help opens and any click closes it.
    click_text(&mut vista, "⎇ main");
    vista.expect_text("9: Git · Log");
    click_text(&mut vista, "?: Help");
    vista.expect_text("any key to close");
    vista.click(5, 5);
    vista.expect_no_text("any key to close");
}

#[test]
fn find_popup_options_toggle_on_click() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(140, 40).start();
    vista.expect_text("README.md");
    vista.press("ctrl+f");
    vista.expect_text("File mask:");
    vista.expect_text(" Cc ").to_have_fg("ui.dim");
    click_text(&mut vista, " Cc ");
    vista.expect_text(" Cc ").to_have_bg("ui.button");
    click_text(&mut vista, " .* ");
    vista.expect_text(" .* ").to_have_bg("ui.button");
    // A scope is picked directly, not stepped through.
    click_text(&mut vista, "Changed files");
    vista.expect_text("Changed files").to_have_fg("ui.text.strong");
    vista.expect_text("In Project").to_have_fg("ui.text");
    // The mask field takes the typing after a click on it.
    click_text(&mut vista, "File mask:");
    vista.type_text("*.rs");
    vista.expect_text(" *.rs▏");
    vista.press("esc");
    vista.expect_no_text("File mask:");
}

#[test]
fn go_to_file_tabs_and_toggle_on_click() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(140, 40).start();
    vista.expect_text("README.md");
    vista.press("ctrl+p");
    vista.expect_text("[ ] include ignored (Alt-I)");
    click_text(&mut vista, "include ignored");
    vista.expect_text("[x] include ignored (Alt-I)");
    click_text(&mut vista, " Recent ");
    vista.expect_text(" Recent ").to_have_fg("ui.text.strong");
    vista.expect_text(" All ").to_have_fg("ui.text");
}

#[test]
fn commit_window_amend_and_diff_options_on_click() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).size(140, 40).start();
    vista.expect_text("README.md");
    vista.press("alt+2");
    vista.expect_text("Unversioned Files");
    click_text(&mut vista, "[ ] Amend");
    vista.expect_text("[x] Amend");
    click_text(&mut vista, "[x] Amend");
    vista.expect_text("[ ] Amend");

    // A diff: its hint line switches to unified, ▸ steps changes.
    let (row, col) = vista.expect_text_in("status.rs", 0..40).position();
    vista.double_click(col, row);
    vista.expect_text("change 1 of");
    vista.expect_text("side-by-side ·");
    click_text(&mut vista, "u unified");
    vista.expect_text("unified ·");
    vista.expect_no_text("side-by-side ·");
}
