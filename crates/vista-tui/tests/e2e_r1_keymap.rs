//! The Keymap page of Preferences: keys added by pressing them, taken from
//! another action, removed and reset; applied at once and remembered.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn repo() -> Repo {
    let long: String = (1..=100).map(|i| format!("line {i}\n")).collect();
    Fixture::git().file("a.txt", long).commit("init").build()
}

fn state(repo: &Repo) -> String {
    std::fs::read_to_string(repo.home().join(".config/vista/state.toml")).unwrap_or_default()
}

fn open_prefs(vista: &mut Vista) {
    let (row, col) = vista.expect_text("⚙").position();
    vista.click(col, row);
    vista.expect_text(" Preferences ");
}

/// Preferences › Keymap, filtered down to `filter`.
fn keymap_page(vista: &mut Vista, filter: &str) {
    open_prefs(vista);
    vista.press("tab");
    vista.expect_text("R reset all");
    vista.press("/");
    vista.type_text(filter);
    vista.press("enter");
    vista.expect_text(&format!("Filter: {filter}"));
}

#[test]
fn keys_are_added_taken_removed_and_reset() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(150, 44).open("a.txt").start();
    vista.expect_text("line 1");
    vista.expect_text("Ctrl-P Go to File");
    keymap_page(&mut vista, "go to");
    vista.expect_text("Go to Line:Column");
    vista.expect_text("Ctrl-Shift-N");
    vista.press("down");
    vista.expect_text("goto-line · default");
    // A new key: pressed, shown, remembered.
    vista.press("enter");
    vista.expect_text("press a key…");
    vista.press("alt+g");
    vista.expect_text(" Alt-G ");
    vista.expect_text("goto-line · changed; r resets it to Ctrl-G");
    vista.expect_text("Go to Line:Column").to_have_fg("ui.accent");
    assert!(state(&repo).contains("\"alt+g\" = \"goto-line\""), "{}", state(&repo));
    // A key another action has: asked first, then taken from it.
    vista.press("enter");
    vista.press("ctrl+p");
    vista.expect_text("Ctrl-P runs Go to File. Enter: use it for Go to Line:Column instead");
    vista.press("enter");
    vista.expect_text("Ctrl-P ");
    assert!(state(&repo).contains("\"ctrl+p\" = \"goto-line\""), "{}", state(&repo));
    // Applied at once: the top bar's hint and the keys themselves.
    vista.press("esc");
    vista.press("esc");
    vista.expect_no_text(" Preferences ");
    vista.expect_text("Ctrl-Shift-N Go to File");
    vista.press("ctrl+p");
    vista.expect_text("[Line] [:column]:");
    vista.press("esc");
    vista.press("alt+g");
    vista.expect_text("[Line] [:column]:");
    vista.press("esc");
    // Removing a key (clicked to select it), then resetting the action.
    keymap_page(&mut vista, "go to line");
    vista.expect_text("Ctrl-G   Ctrl-P   Alt-G");
    let (row, col) = vista.expect_text("Alt-G").position();
    vista.click(col, row);
    vista.press("delete");
    vista.expect_text("Ctrl-G   Ctrl-P   │");
    vista.expect_no_text("Alt-G");
    assert!(!state(&repo).contains("alt+g"), "{}", state(&repo));
    vista.press("r");
    vista.expect_text("goto-line · default");
    // Ctrl-P goes back to Go to File too.
    let (row, _) = vista.expect_text("Go to Line:Column").position();
    assert!(
        vista
            .row_text(row)
            .contains("Column                                               Ctrl-G   │"),
        "{}",
        vista.row_text(row)
    );
    vista.expect_text("Ctrl-P Go to File");
    assert!(!state(&repo).contains("[keys]"), "{}", state(&repo));
}

#[test]
fn removed_keys_stay_removed_after_a_restart_and_reset_all_asks() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(150, 44).open("a.txt").start();
    vista.expect_text("line 1");
    keymap_page(&mut vista, "go to line");
    vista.press("delete");
    vista.expect_text("goto-line · changed; r resets it to Ctrl-G");
    vista.press("esc");
    vista.press("esc");
    vista.expect_no_text(" Preferences ");
    // Ctrl-G does nothing now.
    vista.press("ctrl+g");
    vista.expect_no_text("[Line] [:column]:");
    vista.press("ctrl+q");
    vista.expect_exit();
    assert!(state(&repo).contains("\"ctrl+g\" = \"none\""), "{}", state(&repo));

    let mut vista = Vista::launch(BIN, &repo).size(150, 44).open("a.txt").start();
    vista.expect_text("line 1");
    vista.press("ctrl+g");
    vista.expect_no_text("[Line] [:column]:");
    // Preferences says how many were changed; R resets them all after asking.
    open_prefs(&mut vista);
    vista.expect_text("1 changed");
    vista.press("tab");
    vista.press("R");
    vista.expect_text("Reset every key to its default");
    vista.press("enter");
    vista.expect_no_text("Reset every key");
    assert!(!state(&repo).contains("[keys]"), "{}", state(&repo));
    vista.press("esc");
    vista.press("ctrl+g");
    vista.expect_text("[Line] [:column]:");
}

#[test]
fn the_keymap_page_is_reached_by_click_and_rows_click() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(150, 44).start();
    vista.expect_text("1: Project");
    open_prefs(&mut vista);
    let (row, col) = vista.expect_text(" Keymap ").position();
    vista.click(col + 1, row);
    vista.expect_text("R reset all");
    // A key of a row: selected (its name below the list).
    let (row, col) = vista.expect_text("Ctrl-Shift-N").position();
    vista.click(col, row);
    vista.expect_text("goto-file · default");
    vista.press("delete");
    vista.expect_text("goto-file · changed; r resets it to Ctrl-P, Ctrl-Shift-N");
    assert!(!vista.row_text(row).contains("Ctrl-Shift-N"), "{}", vista.row_text(row));
    assert!(state(&repo).contains("\"ctrl+shift+n\" = \"none\""), "{}", state(&repo));
    // Back to Settings with its tab.
    let (row, col) = vista.expect_text(" Settings ").position();
    vista.click(col + 1, row);
    vista.expect_text("Appearance");
    vista.expect_text("1 changed");
}
