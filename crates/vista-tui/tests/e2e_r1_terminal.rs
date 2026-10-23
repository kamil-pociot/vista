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
