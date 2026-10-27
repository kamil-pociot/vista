//! The Terminal tool window (Alt-T): a shell under the editor, in the
//! project's folder.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn repo() -> Repo {
    Fixture::git().file("alpha.txt", "in alpha\n").commit("init").build()
}

#[test]
fn terminal_tool_window_runs_a_shell_in_the_project() {
    let repo = repo();
    let name = repo.path().file_name().unwrap().to_string_lossy().into_owned();
    let mut vista = Vista::launch(BIN, &repo).env("SHELL", "/bin/sh").size(140, 36).start();
    vista.expect_text("1: Project");
    // Alt-T: a shell under the editor, in the project folder; the tree stays.
    vista.press("alt+t");
    vista.expect_text("T: Terminal");
    vista.type_text("echo where=$(basename \"$PWD\")");
    vista.press("enter");
    vista.expect_text(&format!("where={name}"));
    vista.expect_text("1: Project");
    // Ctrl-\ back to the editor; Alt-T returns to the same shell.
    vista.press("ctrl+\\");
    vista.press("alt+t");
    vista.type_text("echo again");
    vista.press("enter");
    vista.expect_text("again");
    vista.expect_text(&format!("where={name}"));
    // The shell ends: Enter starts a new one.
    vista.type_text("exit");
    vista.press("enter");
    vista.expect_text("Enter starts a new shell");
    vista.press("enter");
    vista.expect_no_text("Enter starts a new shell");
    vista.type_text("echo fresh");
    vista.press("enter");
    vista.expect_text("fresh");
    // Alt-T while it has focus hides it.
    vista.press("alt+t");
    vista.expect_no_text("back to the editor");
}

#[test]
fn shift_enter_reaches_the_program_as_esc_cr() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).env("SHELL", "/bin/sh").size(140, 36).start();
    vista.expect_text("1: Project");
    vista.press("alt+t");
    vista.expect_text("T: Terminal");
    vista.type_text("stty raw -echo; echo ready-$((1+1)); dd bs=1 count=2 2>/dev/null | od -An -tx1");
    vista.press("enter");
    vista.expect_text("ready-2");
    // Shift-Enter as a kitty-keyboard terminal (Ghostty) reports it.
    vista.send_raw(b"\x1b[13;2u");
    vista.wait_until("the program read ESC CR", |s| {
        let contents = s.contents();
        let words: Vec<&str> = contents.split_whitespace().collect();
        words.windows(2).any(|w| w == ["1b", "0d"])
    });
}

#[test]
fn the_wheel_and_shift_page_keys_scroll_back_through_the_output() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).env("SHELL", "/bin/sh").size(140, 36).start();
    vista.expect_text("1: Project");
    vista.press("alt+t");
    vista.expect_text("T: Terminal");
    vista.type_text("i=1; while [ $i -le 200 ]; do echo row-$i; i=$((i+1)); done");
    vista.press("enter");
    vista.expect_text("row-200");
    vista.expect_no_text("row-150 ");
    // The wheel over the shell: back through what scrolled off.
    let (row, col) = vista.expect_text("row-200").position();
    for _ in 0..10 {
        vista.scroll_up(col, row);
    }
    vista.expect_text("lines back");
    vista.expect_no_text("row-200");
    vista.scroll_down(col, row);
    vista.expect_text("lines back");
    // Shift-PgUp further back; typing returns to the prompt and goes to the shell.
    vista.press("shift+pageup");
    vista.expect_text("lines back");
    vista.type_text("echo bac");
    vista.expect_no_text("lines back");
    vista.type_text("k-at-the-bottom");
    vista.press("enter");
    vista.expect_text("back-at-the-bottom");
    // Shift-PgDn after Shift-PgUp: back down to the live screen.
    vista.press("shift+pageup");
    vista.expect_text("lines back");
    vista.press("shift+pagedown");
    vista.expect_no_text("lines back");
    vista.expect_text("back-at-the-bottom");
}

#[test]
fn the_bottom_tool_window_resizes_and_remembers_its_height() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).env("SHELL", "/bin/sh").size(140, 40).start();
    vista.expect_text("1: Project");
    vista.press("alt+t");
    // Its title row (the status bar has a "T: Terminal" label too).
    let title = "back to the editor";
    let (row, _) = vista.expect_text(title).position();
    // Dragged up by its title row (away from the labels on it).
    vista.drag((70, row), (70, row - 8));
    vista.settle(std::time::Duration::from_millis(100));
    let (moved, _) = vista.expect_text(title).position();
    assert_eq!(moved, row - 8, "the title row follows the mouse");
    let state = || std::fs::read_to_string(repo.home().join(".config/vista/state.toml")).unwrap_or_default();
    assert!(state().contains("bottom_height = "), "saved on release: {}", state());
    // Ctrl-Shift-↓ / ↑ from the shell itself (the shell doesn't get them).
    vista.press("ctrl+shift+down");
    vista.wait_until("two rows shorter", |s| {
        s.contents()
            .lines()
            .nth(moved as usize + 2)
            .is_some_and(|l| l.contains(title))
    });
    vista.press("ctrl+shift+up");
    vista.wait_until("back", |s| {
        s.contents()
            .lines()
            .nth(moved as usize)
            .is_some_and(|l| l.contains(title))
    });
    // Remembered: the next run opens it at that height. (Out of the shell
    // first: it gets every other key.)
    vista.press("ctrl+\\");
    vista.press("ctrl+q");
    vista.expect_exit();
    let mut vista = Vista::launch(BIN, &repo)
        .env("SHELL", "/bin/sh")
        .size(140, 40)
        .theme("")
        .start();
    vista.expect_text("1: Project");
    vista.press("alt+t");
    let (again, _) = vista.expect_text(title).position();
    assert_eq!(again, moved);
}
