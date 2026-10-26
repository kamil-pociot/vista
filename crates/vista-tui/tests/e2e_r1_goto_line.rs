//! Go to Line:Column (`Ctrl-G`): in code, diffs, rendered Markdown and the editor.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");
const MINIMAL: &str = "[editor]\nconfig = \"minimal\"\n";

/// 200 lines; line 150 has a `Z` at column 6.
fn long_txt() -> String {
    (1..=200)
        .map(|i| {
            if i == 150 {
                "aaaaaZbbbbb\n".to_string()
            } else {
                format!("line {i}\n")
            }
        })
        .collect()
}

fn repo() -> Repo {
    let big: String = (1..=300).map(|i| format!("line {i}\n")).collect();
    // Five lines inserted at the top: working-tree line N is HEAD's N-5.
    let changed = format!("{}{big}", (1..=5).map(|i| format!("new {i}\n")).collect::<String>());
    let doc: String = (1..=60).map(|i| format!("## Section {i}\n\ntext {i}\n\n")).collect();
    Fixture::git()
        .file("long.txt", long_txt())
        .file("big.txt", big)
        .file("doc.md", doc)
        .commit("init")
        .modify("big.txt", changed)
        .build()
}

#[test]
fn goes_to_a_line_and_column_from_any_panel() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(120, 40).open("long.txt").start();
    vista.expect_text("line 1");
    // From the tree: the file in front is the one it goes in.
    vista.click_text("big.txt");
    vista.press("ctrl+g");
    vista.expect_text("Go to Line:Column");
    vista.expect_text("Lines 1–200");
    // Prefilled with where the cursor is; typing replaces it.
    vista.expect_text("[Line] [:column]: 1:1");
    vista.type_text("150:6");
    vista.press("enter");
    vista.expect_no_text("Go to Line:Column");
    vista.expect_text("150:6");
    // In the viewer: the project's random folder name can have a `Z` too.
    vista.expect_text_in("Z", 40..120).to_have_bg("ui.selection");
    // The viewer has the focus now: keys move its cursor.
    vista.press("down");
    vista.expect_text("151:1");
    // `:col` stays on the line; past the end goes to the last line.
    vista.press("ctrl+g");
    vista.expect_text("[Line] [:column]: 151:1");
    vista.type_text(":3");
    vista.press("enter");
    vista.expect_text("151:3");
    vista.press("ctrl+g");
    vista.type_text("999");
    vista.press("enter");
    vista.expect_text("200:1");
    vista.expect_text("line 200");
    // Esc leaves the cursor where it was.
    vista.press("ctrl+g");
    vista.type_text("5");
    vista.press("esc");
    vista.expect_no_text("Go to Line:Column");
    vista.expect_text("200:1");
}

#[test]
fn needs_a_file_and_follows_the_keymap() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo)
        .config("[keys]\n\"alt+g\" = \"goto-line\"\n\"ctrl+g\" = \"none\"\n")
        .size(120, 40)
        .start();
    vista.expect_text("long.txt");
    vista.press("alt+g");
    vista.expect_text("Open a file first");
    vista.click_text("long.txt");
    vista.press("enter");
    vista.expect_text("line 1");
    vista.press("ctrl+g");
    vista.press("alt+g");
    vista.expect_text("Go to Line:Column");
    vista.type_text("42");
    vista.press("enter");
    vista.expect_text("42:1");
}

#[test]
fn in_a_diff_goes_to_the_working_tree_line_and_unfolds() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(160, 44).start();
    vista.expect_text("big.txt");
    vista.press("alt+2");
    vista.expect_text("Changes  ");
    let (row, col) = vista.expect_text_in("big.txt", 0..44).position();
    vista.click(col, row);
    vista.press("enter");
    vista.expect_text("Δ big.txt");
    vista.expect_text("new 1");
    // Unchanged lines folded away.
    vista.press("z");
    vista.expect_no_text("line 20 ");
    vista.press("ctrl+g");
    vista.expect_text("Lines 1–305");
    vista.type_text("100");
    vista.press("enter");
    // Working-tree line 100 is HEAD's line 95.
    vista.expect_text("100:1");
    vista.expect_text("line 95 ");
}

#[test]
fn in_rendered_markdown_shows_the_block_of_that_source_line() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(120, 40).open("doc.md").start();
    vista.expect_text("Section 1");
    vista.expect_no_text("Section 40");
    vista.press("ctrl+g");
    vista.expect_text("Lines 1–240");
    // "## Section 40" is source line 157.
    vista.type_text("158");
    vista.press("enter");
    vista.expect_text("Section 40");
    vista.expect_no_text("Section 1 ");
}

#[test]
fn in_the_editor_moves_neovims_cursor() {
    if !vista_nvim::available("nvim") {
        eprintln!("nvim not installed; skipping");
        return;
    }
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo)
        .config(MINIMAL)
        .size(120, 40)
        .open("long.txt")
        .start();
    vista.expect_text("line 1");
    // The viewer's line and column carry over to the editor.
    vista.press("ctrl+g");
    vista.type_text("150:6");
    vista.press("enter");
    vista.expect_text("150:6");
    vista.press("e");
    vista.expect_text("150,6");
    // In the editor, Ctrl-G opens vista's Go to Line, prefilled from Neovim.
    vista.press("ctrl+g");
    vista.expect_text("[Line] [:column]: 150:6");
    vista.type_text("30:4");
    vista.press("enter");
    vista.expect_text("30,4");
    vista.expect_text("line 30");
}
