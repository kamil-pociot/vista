//! M3 end-to-end: editing with the embedded Neovim (real `nvim`).

use std::time::{Duration, Instant};

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");
const MINIMAL: &str = "[editor]\nconfig = \"minimal\"\n";

fn nvim_available() -> bool {
    vista_nvim::available("nvim")
}

fn wait_for_file(repo: &Repo, rel: &str, pred: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let s = repo.read(rel);
        if pred(&s) || Instant::now() > deadline {
            return s;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// Neovim (minimal profile) is showing a buffer: `~` marks lines past the end.
fn expect_editor(vista: &mut Vista) {
    vista.expect_text("~ ");
}

#[test]
fn edit_save_and_status_refresh() {
    if !nvim_available() {
        eprintln!("nvim not installed; skipping");
        return;
    }
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).config(MINIMAL).open("README.md").start();
    vista.expect_text("A sample project.");
    vista.expect_text_in("README.md", 0..34).to_have_fg("ui.text");
    vista.press("e");
    expect_editor(&mut vista);
    // The editor matches the look (theme sync): Normal background = ui.bg.
    let want = vista.theme().color("ui.bg");
    vista.wait_until("editor background synced with the theme", move |s| {
        vista_e2e::find(s, "~ ")
            .and_then(|(r, c)| s.cell(r, c))
            .is_some_and(|c| c.bgcolor() == vista_e2e::vt100::Color::Rgb(want.0, want.1, want.2))
    });

    vista.press_all(&["g", "g", "shift+o"]);
    vista.type_text("Hello from nvim");
    vista.press("esc");
    // Unsaved: the tab and the tree row get a marker.
    vista.expect_text("● README.md");
    vista.expect_text_in("README.md ●", 0..34);
    vista.type_text(":w");
    vista.press("enter");
    let content = wait_for_file(&repo, "README.md", |s| s.starts_with("Hello from nvim"));
    assert!(content.starts_with("Hello from nvim\n# file-browser"), "{content:?}");
    vista.expect_no_text("● README.md");
    // The tree colour flips to modified right away.
    vista.expect_text_in("README.md", 0..34).to_have_fg("git.modified");
}

#[test]
fn leave_editor_return_and_quit_nvim() {
    if !nvim_available() {
        return;
    }
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).config(MINIMAL).open("README.md").start();
    vista.expect_text("A sample project.");
    vista.press("e");
    expect_editor(&mut vista);
    // Ctrl-\ hands focus back to the tree.
    vista.press("ctrl+\\");
    vista.expect_text_in("crates", 0..34);
    vista.press("down");
    vista.expect_text_in("crates", 0..34).to_have_bg("ui.selection");
    // Tab focuses the editor again; :q closes Neovim and shows the viewer.
    vista.press("tab");
    vista.type_text(":q");
    vista.press("enter");
    vista.expect_text("Editor closed");
    vista.expect_no_text("~ ");
    vista.expect_text("A sample project.");
}

#[test]
fn edit_from_the_tree() {
    if !nvim_available() {
        return;
    }
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).config(MINIMAL).start();
    vista.expect_text("Cargo.toml");
    vista.click_text("Cargo.toml");
    // In the tree letters search: Alt-Enter edits.
    vista.press("alt+enter");
    expect_editor(&mut vista);
    vista.expect_text("[workspace]");
    vista.expect_text(" Cargo.toml ");
}

#[test]
fn quitting_with_unsaved_changes_asks_first() {
    if !nvim_available() {
        return;
    }
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).config(MINIMAL).open("README.md").start();
    vista.expect_text("A sample project.");
    vista.press("e");
    expect_editor(&mut vista);
    vista.press_all(&["shift+o"]);
    vista.type_text("draft");
    vista.press("esc");
    vista.expect_text("● README.md");
    vista.press("ctrl+\\");
    vista.press("ctrl+q");
    vista.expect_text("unsaved changes");
    vista.press("ctrl+q");
    assert_eq!(vista.expect_exit(), 0);
    assert!(
        !repo.read("README.md").contains("draft"),
        "changes were discarded, not saved"
    );
}

#[test]
fn falls_back_to_editor_when_nvim_is_missing() {
    let repo = Fixture::rust_project()
        .file("edit.sh", "#!/bin/sh\nprintf 'appended by editor\\n' >> \"$1\"\n")
        .build();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = repo.path().join("edit.sh");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let script = repo.path().join("edit.sh").display().to_string();
    let mut vista = Vista::launch(BIN, &repo)
        .config("[editor]\nnvim = \"definitely-not-nvim\"\n")
        .env("EDITOR", &script)
        .env("VISUAL", "")
        .open("README.md")
        .start();
    vista.expect_text("A sample project.");
    vista.press("e");
    vista.expect_text("appended by editor");
    vista.expect_text("using");
    assert!(repo.read("README.md").ends_with("appended by editor\n"));
}
