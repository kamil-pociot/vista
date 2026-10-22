//! Line numbers everywhere (rendered Markdown too), and view settings that
//! survive a restart (`state.toml`).

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn repo() -> Repo {
    Fixture::git()
        .file("README.md", "# Title\n\nPara one.\n\n- item\n")
        .file("src/main.rs", "fn main() {}\n")
        .file("src/lib.rs", "pub fn a() {}\n")
        .file(".gitignore", "build/\n")
        .commit("init")
        .modify("src/lib.rs", "pub fn a() {\n    1\n}\n")
        .file("build/out.txt", "x\n")
        .build()
}

/// Opens `rel` with Go to File.
fn open(vista: &mut Vista, query: &str, name: &str) {
    vista.press("ctrl+p");
    vista.type_text(query);
    vista.expect_text_in(name, 4..40).to_have_bg("ui.selection");
    vista.press("enter");
}

#[test]
fn line_numbers_in_code_and_rendered_markdown_toggle_with_shift_l() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).open("README.md").start();
    // Rendered Markdown: each block shows its source line.
    vista.expect_text("1  Title");
    vista.expect_text("3  Para one.");
    vista.expect_text("5  • item");
    vista.press("L");
    vista.expect_text("Line numbers off");
    vista.expect_text("Para one.");
    vista.expect_no_text("3  Para one.");
    // Code: the gutter keeps change marks but drops the numbers.
    open(&mut vista, "main", "main.rs");
    vista.expect_text("fn main");
    vista.expect_no_text("1   fn main");
    vista.press("L");
    vista.expect_text("Line numbers on");
    vista.expect_text("1   fn main");
}

#[test]
fn view_settings_survive_a_restart() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).open("README.md").start();
    vista.expect_text("Para one.");
    // Viewer: line numbers off, Markdown stops wrapping.
    vista.press("L");
    vista.press("w");
    vista.expect_text("Long lines scroll sideways (Markdown)");
    // Tree: hide ignored files (shown by default).
    vista.expect_text_in("build", 0..40);
    let (row, col) = vista.expect_text_in("README.md", 0..40).position();
    vista.click(col, row);
    vista.press("i");
    vista.expect_text("Ignored files hidden");
    // Commit: group by folder; diff: unified, collapsed, ignoring whitespace.
    vista.press("alt+2");
    vista.expect_text("Changes  ");
    vista.press("g");
    vista.expect_text("[x] src");
    let (row, col) = vista.expect_text_in("lib.rs", 0..44).position();
    vista.click(col, row);
    vista.press("enter");
    vista.expect_text("Δ lib.rs");
    // The tab shows at once; the diff keys need the loaded diff.
    vista.expect_text("change 1 of 1");
    vista.press("u");
    vista.press("z");
    vista.press("i");
    vista.expect_text("unified · whitespace: ignore");
    // Find in Files: match case.
    vista.press("ctrl+f");
    vista.expect_text("Find in Files");
    vista.press("alt+c");
    vista.press("esc");
    // Look.
    vista.press("ctrl+t");
    vista.expect_text("Look: Paper Light");
    vista.press("q");
    vista.expect_exit();

    let state = std::fs::read_to_string(repo.home().join(".config/vista/state.toml")).unwrap();
    for want in [
        "look = \"paper\"",
        "line_numbers = false",
        "wrap_markdown = false",
        "unified = true",
        "collapse = true",
        "ignore_whitespace = true",
        "group_by_folder = true",
        "show_ignored = false",
        "case = true",
    ] {
        assert!(state.contains(want), "{want} missing from:\n{state}");
    }

    // A new run picks everything up (no VISTA_THEME, so the look is the
    // remembered one).
    let mut vista = Vista::launch(BIN, &repo)
        .theme("")
        .size(150, 40)
        .open("README.md")
        .start();
    vista.expect_text("╭ 1: Project");
    vista.expect_text("Para one.");
    vista.expect_no_text("3  Para one.");
    vista.expect_text_in("README.md", 0..40);
    vista.expect_no_text("build");
    vista.press("alt+2");
    vista.expect_text("Changes  ");
    vista.expect_text("[x] src");
    let (row, col) = vista.expect_text_in("lib.rs", 0..44).position();
    vista.click(col, row);
    vista.press("enter");
    vista.expect_text("unified · whitespace: ignore");
    vista.expect_text("collapse: on");
    vista.press("ctrl+f");
    vista.expect_text("Find in Files");
    // Match case is on: its toggle has the (remembered) Paper look's button
    // colour; the harness assumes Darcula here, so compare directly.
    let paper = vista_theme::ThemeLoader::new(None).load("paper").unwrap();
    let want = paper.color("ui.button");
    vista.wait_until("Cc highlighted", move |s| {
        vista_e2e::find(s, "Cc")
            .and_then(|(r, c)| s.cell(r, c))
            .is_some_and(|cell| cell.bgcolor() == vista_e2e::vt100::Color::Rgb(want.0, want.1, want.2))
    });
}
