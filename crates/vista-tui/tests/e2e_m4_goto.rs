//! M4 end-to-end: Go to File.

use std::time::{Duration, Instant};

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn project() -> Repo {
    Fixture::rust_project()
        .file("long.txt", (1..=200).map(|i| format!("line {i}\n")).collect::<String>())
        .file("src/controllers/FooBarController.java", "class FooBarController {}\n")
        .build()
}

#[test]
fn finds_and_opens_files_by_fuzzy_name() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("README.md");
    vista.press("ctrl+p");
    vista.expect_text("include ignored");
    vista.expect_text("Files");
    vista.type_text("stat");
    // The best match is status.rs; matched characters are highlighted.
    vista.expect_text("status.rs");
    vista.expect_text("crates/fb-git/src");
    vista.expect_text("status.rs").to_have_fg("syntax.function");
    // Selected row and a preview of the file.
    vista.expect_text("status.rs").to_have_bg("ui.selection");
    vista.expect_text("use std::path::{Path, PathBuf};");
    vista.press("enter");
    vista.expect_no_text("include ignored");
    vista.expect_text(" status.rs ");
    vista.expect_text("pub struct StatusMap");
}

#[test]
fn camel_humps_and_line_suffix() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("README.md");
    vista.press("ctrl+p");
    vista.type_text("FoBaCo");
    vista.expect_text("FooBarController.java");
    vista.press("enter");
    vista.expect_text("class FooBarController");
    vista.press("ctrl+p");
    vista.type_text("long:150");
    vista.expect_text("long.txt");
    vista.press("enter");
    vista.expect_text("150:1");
    vista.expect_text("line 150");
}

#[test]
fn navigation_scopes_escape_and_mouse() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).open("README.md").start();
    vista.expect_text("A sample project.");
    vista.press("ctrl+p");
    vista.type_text("rs");
    vista.expect_text("diff.rs");
    // Arrow keys move the selection.
    vista.press("down");
    vista.press("down");
    vista.press("up");
    // Recent scope: README.md was opened, so it is listed.
    vista.press("ctrl+u");
    vista.press("shift+tab");
    vista.press("shift+tab");
    vista.expect_text_in("README.md", 0..30).to_have_bg("ui.selection");
    vista.press("esc");
    vista.expect_no_text("include ignored");
    // Mouse: click a result to select it, Enter opens it.
    vista.press("ctrl+p");
    vista.expect_text("include ignored");
    vista.type_text("cargo");
    // Wait for the filtered list before clicking (not the unfiltered one).
    vista.expect_text("⌕ cargo");
    vista.click_text("Cargo.lock");
    vista.expect_text("Cargo.lock").to_have_bg("ui.selection");
    vista.press("enter");
    vista.expect_text("# lock v2");
}

#[test]
fn include_ignored_toggle() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("README.md");
    vista.press("ctrl+p");
    vista.type_text("build.lo");
    vista.expect_text("Nothing found");
    vista.press("alt+i");
    vista.expect_text("[x] include ignored");
    vista.expect_text_in("build.log", 0..200).to_have_bg("ui.selection");
}

#[test]
fn new_files_show_up_in_the_index() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("README.md");
    repo.write("fresh_module.rs", "// new\n");
    vista.expect_text("fresh_module.rs");
    vista.press("ctrl+p");
    vista.type_text("freshmod");
    vista.expect_text("fresh_module.rs").to_have_bg("ui.selection");
}

#[test]
fn edit_from_go_to_file() {
    if !vista_nvim::available("nvim") {
        return;
    }
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo)
        .config("[editor]\nconfig = \"minimal\"\n")
        .start();
    vista.expect_text("README.md");
    vista.press("ctrl+p");
    vista.type_text("long:7");
    // The result is selected in the popup (not just the tree row showing).
    vista.expect_text_in("long.txt", 6..60).to_have_bg("ui.selection");
    // Alt-Enter (or Shift/Ctrl-Enter with the kitty keyboard protocol) opens
    // straight in the editor.
    vista.press("alt+enter");
    // Neovim's ruler (minimal profile) shows the cursor on line 7.
    vista.expect_text("7,1");
    vista.expect_text("line 7");
}

#[test]
fn keystrokes_stay_fast_on_a_large_project() {
    let mut f = Fixture::plain();
    for i in 0..50_000 {
        f = f.file(&format!("pkg{}/mod{}/file_{i}.txt", i % 50, i % 20), "");
    }
    let repo = f.build();
    let mut vista = Vista::launch(BIN, &repo).env("VISTA_NO_WATCH", "1").start();
    vista.set_timeout(Duration::from_secs(30));
    vista.expect_text("pkg0");
    vista.press("ctrl+p");
    vista.expect_text("of 50000 files");
    vista.type_text("file_4999");
    vista.expect_text("file_4999.txt");
    // Each keystroke re-ranks 50k paths; measure keystroke → updated top result.
    fn top_is(want: &'static str) -> impl Fn(&vista_e2e::vt100::Screen) -> bool {
        move |s| {
            let text = s.contents();
            let lines: Vec<&str> = text.lines().collect();
            lines
                .iter()
                .position(|l| l.contains("⌕"))
                .and_then(|i| lines.get(i + 1))
                .is_some_and(|l| l.contains(want))
        }
    }
    let mut worst = Duration::ZERO;
    for (q, want) in [
        ("2", "file_49992.txt"),
        ("\x7f", "file_4999.txt"),
        ("1", "file_49991.txt"),
    ] {
        let start = Instant::now();
        vista.type_text(q);
        vista.wait_until(&format!("top result {want}"), top_is(want));
        worst = worst.max(start.elapsed());
    }
    eprintln!("worst keystroke-to-screen latency: {worst:?}");
    assert!(worst < Duration::from_millis(400), "too slow: {worst:?}");
}
