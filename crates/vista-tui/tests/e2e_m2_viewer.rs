//! M2 end-to-end: viewing code, Markdown, images and binaries.

use vista_e2e::fixture::TINY_PNG;
use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

const DOC_MD: &str = "# Guide\n\nIntro with **bold** and `code`.\n\n## Steps\n\n- first item\n- second item\n\n| Name | Size |\n|------|-----:|\n| a.rs | 10 |\n\n```rust\nfn demo() {}\n```\n\n> a quote\n\n![shot](shot.png)\n";

fn project() -> Repo {
    Fixture::rust_project()
        .file("docs/doc.md", DOC_MD)
        .file("docs/shot.png", TINY_PNG)
        .file("data.bin", b"\x7fELF\x00\x01binary\x00data".as_slice())
        .file("long.txt", (1..=200).map(|i| format!("line {i}\n")).collect::<String>())
        .build()
}

/// Open a file through the tree: select it by name and press Enter.
fn open_from_tree(vista: &mut Vista, name: &str) {
    vista.click_text(name);
    vista.press("enter");
}

#[test]
fn opens_code_with_highlighting_and_gutter_markers() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).open("crates/fb-git/src/status.rs").start();
    vista.expect_text("use anyhow::Context;");
    // Tab label coloured by status, language and position in the status bar.
    vista.expect_text(" status.rs ").to_have_fg("git.modified");
    vista.expect_text("Rust");
    vista.expect_text("1:1");
    // Highlighting (async) arrives.
    vista.expect_text("pub struct").to_have_fg("syntax.keyword");
    vista.expect_text("\"bare repository\"").to_have_fg("syntax.string");
    // Gutter: line 3 was added, line 13 modified (vs HEAD).
    let (row, _) = vista.expect_text("use anyhow::Context;").position();
    let text = vista.row_text(row);
    assert!(text.contains("3 ▌"), "line 3 marker: {text:?}");
    let (r13, _) = vista.expect_text(".context(").position();
    let col = vista.row_text(r13).find('▌').expect("marker on line 13");
    let col = vista.row_text(r13)[..col].chars().count() as u16;
    assert_eq!(vista.fg_at(col, r13), Some(vista.theme().color("gutter.modified")));
    assert_eq!(vista.fg_at(col, row), Some(vista.theme().color("gutter.added")));
    // F7 jumps between changes.
    vista.press("f7");
    vista.expect_text("3:1");
    vista.press("f7");
    vista.expect_text("13:1");
    vista.press("shift+f7");
    vista.expect_text("3:1");
}

#[test]
fn renders_markdown_and_toggles_raw() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).open("docs/doc.md").start();
    vista.expect_text("Guide").to_have_fg("syntax.heading");
    vista.expect_text("━━━━━━━━");
    vista.expect_text("• first item");
    vista.expect_text("┌─");
    vista.expect_text("│ a quote");
    vista.expect_text("fn demo() {}");
    // Inline image: halfblocks draw the picture (▀ cells) in reserved rows.
    vista.expect_text("▀");
    vista.press("m");
    vista.expect_text("# Guide");
    vista.expect_text("![shot](shot.png)");
    vista.press("m");
    vista.expect_no_text("# Guide");
}

#[test]
fn shows_images_with_halfblocks() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).open("docs/shot.png").start();
    vista.expect_text("PNG 2×2");
    vista.expect_text("▀");
}

#[test]
fn sends_kitty_graphics_when_forced() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo)
        .env("VISTA_IMAGE_PROTOCOL", "kitty")
        .open("docs/shot.png")
        .start();
    vista.expect_text("PNG 2×2");
    vista.expect_raw(b"\x1b_Gq=2,", "a kitty graphics transmission");
    let raw = String::from_utf8_lossy(&vista.raw_output()).into_owned();
    assert!(raw.contains("a=T,U=1"), "virtual placement for unicode placeholders");
}

#[test]
fn images_can_be_turned_off() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo)
        .env("VISTA_IMAGE_PROTOCOL", "off")
        .open("docs/shot.png")
        .start();
    vista.expect_text("Images are off");
}

#[test]
fn binary_and_too_large_files() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).open("data.bin").start();
    vista.expect_text("Binary file");
    vista.expect_text("7f 45 4c 46");
    drop(vista);

    repo.write("huge.log", vec![b'x'; 21 * 1024 * 1024]);
    let mut vista = Vista::launch(BIN, &repo).open("huge.log").start();
    vista.expect_text("File too large to view (21.0 MB)");
}

#[test]
fn tabs_recent_files_and_reveal() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("README.md");
    open_from_tree(&mut vista, "README.md");
    vista.expect_text("A sample project.");
    // Back to the tree, open another file.
    vista.press("alt+1");
    open_from_tree(&mut vista, "Cargo.toml");
    vista.expect_text("[workspace]");
    vista.expect_text(" README.md ");
    vista.expect_text(" Cargo.toml ");
    // Alt-Left / Alt-Right cycle tabs.
    vista.press("alt+left");
    vista.expect_text("A sample project.");
    vista.press("alt+right");
    vista.expect_text("[workspace]");
    // Ctrl-E preselects the previous file.
    vista.press("ctrl+e");
    vista.expect_text("Recent Files");
    vista.press("enter");
    vista.expect_no_text("Recent Files");
    vista.expect_text("A sample project.");
    // Clicking a tab activates it.
    vista.click_text(" Cargo.toml ");
    vista.expect_text("[workspace]");
    // Ctrl-W closes the active tab.
    vista.press("ctrl+w");
    vista.wait_until("Cargo.toml tab closed", |s| {
        let strip = s.contents().lines().nth(1).unwrap_or("").to_string();
        strip.contains("README.md") && !strip.contains("Cargo.toml")
    });
    vista.expect_text("A sample project.");
    // Alt-F1 reveals the file in the tree and focuses it.
    vista.press("alt+f1");
    vista.expect_text_in("README.md", 0..30).to_have_bg("ui.selection");
}

#[test]
fn search_in_file_and_go_to_line() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).open("long.txt").start();
    vista.expect_text("line 1");
    vista.press("/");
    vista.type_text("line 42");
    vista.expect_text("1/1 ");
    vista.expect_text("42:1");
    // The current match is highlighted with the accent colour.
    vista.expect_text("line 42").to_have_bg("ui.accent");
    vista.press("enter");
    vista.press("esc");
    vista.expect_no_text("Find:");
    vista.press("home");
    vista.press("/");
    vista.type_text("line 1");
    // 111 matches: line 1, 10–19, 100–199; incremental search stays on line 1.
    vista.expect_text("1/111");
    vista.expect_text("1:1");
    vista.press("enter");
    vista.expect_text("10:1");
    vista.press("n");
    vista.expect_text("11:1");
    vista.press("shift+n");
    vista.expect_text("10:1");
    vista.press("esc");
    // Ctrl-G: go to line (more in e2e_r1_goto_line).
    vista.press("ctrl+g");
    vista.expect_text("Go to Line:Column");
    vista.type_text("150");
    vista.press("enter");
    vista.expect_text("150:1");
    vista.expect_text("line 150");
    // End / Home.
    vista.press("end");
    vista.expect_text("200:1");
    vista.press("home");
    vista.expect_text("1:1");
}

#[test]
fn reloads_open_files_changed_on_disk() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).open("README.md").start();
    vista.expect_text("A sample project.");
    repo.write("README.md", "# file-browser\n\nRewritten outside.\n");
    vista.expect_text("Rewritten outside.");
}

#[test]
fn wrap_and_horizontal_scroll() {
    let long = format!("start {} end\n", "x".repeat(300));
    let repo = Fixture::plain().file("wide.txt", long).build();
    let mut vista = Vista::launch(BIN, &repo).size(100, 20).open("wide.txt").start();
    vista.expect_text("start");
    vista.expect_no_text("end");
    vista.press("w");
    vista.expect_text("end");
    vista.press("w");
    vista.expect_no_text("end");
    for _ in 0..80 {
        vista.press("l");
    }
    vista.expect_text("end");
    vista.expect_no_text("start");
}

#[test]
fn mouse_wheel_scrolls_the_viewer() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).open("long.txt").start();
    vista.expect_text("line 1 ");
    for _ in 0..5 {
        vista.scroll_down(80, 10);
    }
    vista.expect_no_text("  1 ▏");
    vista.expect_text("line 16");
    vista.expect_no_text("line 2\n");
}
