//! "Select Opened File" (⌖) in the Project panel header, like IntelliJ.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn nested() -> Repo {
    Fixture::git()
        .file("README.md", "x\n")
        .file("src/deep/inner/target_file.rs", "fn main() {}\n")
        .file("src/other.rs", "\n")
        .commit("init")
        .build()
}

/// Clicks the ⌖ button in the Project header.
fn click_reveal(vista: &mut Vista) {
    let (row, col) = vista.expect_text_in("⌖", 0..60).position();
    vista.click(col, row);
}

#[test]
fn reveal_button_shows_the_opened_file_in_the_tree() {
    let repo = nested();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("README.md");
    // Nothing open: the button is dimmed and says so when clicked.
    vista.expect_text_in("⌖", 0..60).to_have_fg("ui.dim");
    click_reveal(&mut vista);
    vista.expect_text("No file open");
    // Open a file deep in the tree (folders stay collapsed).
    vista.press("ctrl+p");
    vista.type_text("target_file");
    vista.expect_text("src/deep/inner");
    vista.press("enter");
    vista.expect_text("fn main");
    vista.expect_no_text("inner/");
    vista.expect_text_in("⌖", 0..60).to_have_fg("ui.accent");
    // ⌖ expands the folders and selects the file.
    click_reveal(&mut vista);
    vista.expect_text_in("inner", 0..32);
    vista.expect_text_in("target_file.rs", 0..32).to_have_bg("ui.selection");
}

#[test]
fn reveal_button_in_boxed_and_inline_looks() {
    for look in ["paper", "quiet"] {
        let repo = nested();
        let mut vista = Vista::launch(BIN, &repo)
            .theme(look)
            .open("src/deep/inner/target_file.rs")
            .start();
        vista.expect_text("fn main");
        click_reveal(&mut vista);
        vista.expect_text_in("target_file.rs", 0..32).to_have_bg("ui.selection");
    }
}
