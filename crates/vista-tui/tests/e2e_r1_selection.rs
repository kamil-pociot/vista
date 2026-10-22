//! Selecting text in the viewer and copying it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vista_e2e::{Fixture, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

const CODE: &str = "fn main() {\n    let answer = 42;\n}\n";

/// The clipboard, as vista wrote it (tests copy into a file).
fn clipboard(path: &Path, want: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let got = std::fs::read_to_string(path).unwrap_or_default();
        if got == want || Instant::now() > deadline {
            return got;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn launch(repo: &vista_e2e::Repo, open: &str, clip: &Path) -> Vista {
    Vista::launch(BIN, repo)
        .env("VISTA_CLIPBOARD_FILE", &clip.display().to_string())
        .size(120, 30)
        .open(open)
        .start()
}

#[test]
fn drag_double_click_lines_and_all_then_copy() {
    let repo = Fixture::git().file("main.rs", CODE).commit("init").build();
    let clip: PathBuf = repo.home().join("clipboard.txt");
    let mut vista = launch(&repo, "main.rs", &clip);
    vista.expect_text("let answer = 42;");

    // Drag over "let answer = 42" (up to the ;), then y.
    let (row, col) = vista.expect_text("let answer = 42;").position();
    vista.drag((col, row), (col + 15, row));
    vista.expect_text("answer").to_have_bg("ui.selection");
    vista.press("y");
    assert_eq!(clipboard(&clip, "let answer = 42"), "let answer = 42");
    vista.expect_text("Copied 15 characters");

    // Double-click a word, Ctrl-C.
    let (row, col) = vista.expect_text("answer").position();
    vista.double_click(col + 2, row);
    vista.press("ctrl+c");
    assert_eq!(clipboard(&clip, "answer"), "answer");

    // Esc drops the selection.
    vista.press("esc");
    vista.expect_text("answer").to_have_bg("ui.cursorline");

    // Shift-↓ selects whole lines from the cursor.
    vista.press("home");
    vista.press("shift+down");
    vista.press("y");
    let two = "fn main() {\n    let answer = 42;";
    assert_eq!(clipboard(&clip, two), two);
    vista.expect_text("Copied 2 lines");

    // Ctrl-A: everything.
    vista.press("ctrl+a");
    vista.press("y");
    let all = "fn main() {\n    let answer = 42;\n}";
    assert_eq!(clipboard(&clip, all), all);
}

#[test]
fn select_in_rendered_markdown() {
    let repo = Fixture::git()
        .file("README.md", "# Title\n\nSome paragraph text here.\n")
        .commit("init")
        .build();
    let clip: PathBuf = repo.home().join("clipboard.txt");
    let mut vista = launch(&repo, "README.md", &clip);
    vista.expect_text("Some paragraph text here.");
    let (row, col) = vista.expect_text("paragraph").position();
    vista.drag((col, row), (col + 9, row));
    vista.press("y");
    assert_eq!(clipboard(&clip, "paragraph"), "paragraph");
}
