//! M5 end-to-end: Find in Files (IntelliJ-style) and the Find tool window.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn project() -> Repo {
    Fixture::rust_project()
        .file("docs/notes.md", "# Notes\n\nStatusMap caches git status per path.\n")
        .file("archive/old.md", "StatusMap legacy notes\n")
        .file(
            "src/app.rs",
            "fn main() {\n    let map = StatusMap::load();\n    let statusmap = 1;\n}\n",
        )
        .build()
}

fn open_find(vista: &mut Vista) {
    vista.press("ctrl+f");
    vista.expect_text("File mask:");
}

#[test]
fn searches_previews_and_opens_at_the_line() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(140, 44).start();
    vista.expect_text("README.md");
    open_find(&mut vista);
    vista.type_text("StatusMap");
    vista.expect_text("matches in");
    vista.expect_text("In Project");
    // Each hit shows the line with the match highlighted, and its file.
    vista.expect_text("pub struct StatusMap {");
    vista.expect_text("StatusMap {").to_have_bg("ui.match");
    vista.expect_text("status.rs 6");
    // Preview of the selected hit with line numbers.
    vista.expect_text("occurrence");
    // Navigate to src/app.rs line 2 and open it.
    for _ in 0..10 {
        vista.press("down");
        if vista.screen_text().contains("src/app.rs   ") {
            break;
        }
    }
    vista.press("home");
    vista.press("enter");
    vista.expect_no_text("File mask:");
    vista.expect_text(":1");
}

#[test]
fn toggles_mask_and_counts() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(140, 44).start();
    vista.expect_text("README.md");
    open_find(&mut vista);
    vista.type_text("statusmap");
    // Case-insensitive by default: both `StatusMap` and `statusmap` match.
    vista.expect_text("let statusmap = 1;");
    vista.expect_text("StatusMap legacy notes");
    // Alt-C: match case → only the lower-case one.
    vista.press("alt+c");
    vista.expect_text("1 matches in 1 files");
    vista.press("alt+c");
    // Mask: only Markdown, excluding archive/.
    vista.press("tab");
    vista.type_text("*.md, !archive/**");
    vista.expect_text("1 matches in 1 files");
    vista.expect_text("StatusMap caches");
    vista.expect_no_text("StatusMap legacy notes");
    // Regex.
    vista.press("tab");
    vista.press("ctrl+u");
    vista.type_text("Status[A-Z]ap caches");
    vista.expect_text("Nothing found");
    vista.press("alt+x");
    vista.expect_text("1 matches in 1 files");
    // Invalid regex reports an error instead of crashing.
    vista.type_text("(");
    vista.expect_text("regex parse error");
}

#[test]
fn whole_words_and_scopes() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(140, 44).open("docs/notes.md").start();
    vista.expect_text("Notes");
    open_find(&mut vista);
    vista.type_text("status");
    vista.expect_text("matches in");
    vista.press("alt+w");
    // `status` as a whole word: in notes.md ("git status per path") but not "StatusMap".
    vista.expect_text("caches git status per path");
    vista.expect_no_text("pub struct StatusMap");
    vista.press("alt+w");
    // Scope: Open files → only notes.md.
    for _ in 0..3 {
        vista.press("alt+s");
    }
    vista.expect_text("Open files");
    vista.expect_text("1 matches in 1 files");
    // Scope: Changed files (status.rs is modified) — cycle back to Changed.
    vista.press("alt+s"); // Project
    vista.press("alt+s"); // Directory
    vista.press("alt+s"); // Changed
    vista.expect_text("pub struct StatusMap");
    vista.expect_no_text("caches git status");
}

#[test]
fn find_window_and_next_occurrence() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(140, 44).start();
    vista.expect_text("README.md");
    open_find(&mut vista);
    vista.type_text("StatusMap");
    vista.expect_text("matches in");
    vista.press("alt+o");
    vista.expect_no_text("File mask:");
    // The docked tool window groups results by file.
    vista.expect_text("3: Find");
    vista.expect_text("StatusMap (");
    vista.expect_text("▾ status.rs");
    vista.expect_text("▾ notes.md");
    // Enter on the selected hit opens it.
    vista.press("enter");
    vista.expect_text("StatusMap caches");
    // Ctrl-Alt-Down walks through occurrences across files.
    vista.press("ctrl+alt+down");
    vista.expect_text(" status.rs ");
    // Alt-3 hides and shows the window.
    vista.press("tab");
    vista.press("alt+3");
    vista.press("alt+3");
    vista.expect_text("▾ status.rs");
}

#[test]
fn history_and_go_to_file_handover() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(140, 44).start();
    vista.expect_text("README.md");
    open_find(&mut vista);
    vista.type_text("legacy");
    vista.expect_text("StatusMap legacy notes");
    vista.press("esc");
    // Reopening prefills the last query.
    open_find(&mut vista);
    vista.expect_text("StatusMap legacy notes");
    vista.type_text("caches");
    vista.expect_text("StatusMap caches");
    vista.press("alt+up");
    vista.expect_text("StatusMap legacy notes");
    vista.press("esc");
    // Go to File's Text scope hands the query over.
    vista.press("ctrl+p");
    vista.type_text("caches");
    vista.press("tab");
    vista.expect_text("File mask:");
    vista.expect_text("StatusMap caches");
}
