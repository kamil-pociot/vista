//! Terminal tabs: programs (`claude agents`, any command) full screen,
//! next to the project tabs.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");
const CONFIG: &str = "[terminal]\ncommands = [\"cat\"]\n";

fn repo() -> Repo {
    Fixture::git().file("alpha.txt", "in alpha\n").commit("init").build()
}

/// Clicks a tab in the top bar by its label.
fn click_tab(vista: &mut Vista, label: &str) {
    let (row, col) = vista.expect_text_in(label, 0..140).position();
    assert_eq!(row, 0);
    vista.click(col + 1, row);
}

#[test]
fn run_a_command_full_screen_and_switch_back_and_forth() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).config(CONFIG).size(140, 30).start();
    vista.expect_text("1: Project");

    // Ctrl-O lists the configured command; Enter runs it in a new tab.
    vista.press("ctrl+o");
    vista.type_text("cat");
    vista.expect_text("▷ cat");
    vista.press("enter");
    vista.expect_text("▶ cat × ");
    // Full screen: no tree.
    vista.expect_no_text("1: Project");
    // Keys go to the program (the terminal echoes, cat repeats).
    vista.type_text("hello there");
    vista.press("enter");
    vista.wait_until("cat echoed the line", |s| {
        s.contents().matches("hello there").count() >= 2
    });

    // Ctrl-\ goes back to the project; the terminal keeps its tab.
    vista.press("ctrl+\\");
    vista.expect_text("1: Project");
    vista.expect_text("▶ cat × ");
    // Clicking the tab shows it again, as it was.
    click_tab(&mut vista, "▶ cat");
    vista.expect_no_text("1: Project");
    vista.expect_text("hello there");

    // `>command` runs anything; when it ends, Enter closes its tab.
    vista.press("ctrl+\\");
    vista.press("ctrl+o");
    vista.type_text(">echo done-$((40+2))");
    vista.press("enter");
    vista.expect_text("done-42");
    vista.expect_text("exited — Enter closes the tab");
    vista.press("enter");
    vista.expect_no_text("▶ echo");
    vista.expect_text("1: Project");

    // × closes a terminal tab (and stops the program).
    let (row, col) = vista.expect_text_in("▶ cat × ", 0..140).position();
    vista.click(col + "▶ cat ".chars().count() as u16, row);
    vista.expect_no_text("▶ cat");
    vista.expect_text("1: Project");
}

#[test]
fn quitting_with_a_running_terminal_asks_first() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).config(CONFIG).size(140, 30).start();
    vista.expect_text("1: Project");
    vista.press("ctrl+o");
    vista.type_text("cat");
    vista.expect_text("▷ cat");
    vista.press("enter");
    vista.expect_text("▶ cat × ");
    vista.press("ctrl+\\");
    vista.expect_text("1: Project");
    vista.press("q");
    vista.expect_text("1 terminal tab(s) still running");
    vista.press("q");
    vista.expect_exit();
}
