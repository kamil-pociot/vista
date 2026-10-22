//! M0 end-to-end: the app shell in a real PTY.

use vista_e2e::{Fixture, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

#[test]
fn launches_into_design_a_layout_and_quits_cleanly() {
    let repo = Fixture::plain().file("hello.txt", "hi").build();
    let mut vista = Vista::launch(BIN, &repo).size(120, 36).start();

    vista.expect_text("No file open");
    vista.expect_text("1: Project").to_be_bold();
    vista.expect_text("2: Commit");
    vista.expect_text("Go to File");
    // The top bar sits on the panel background.
    vista.expect_text("Go to File").to_have_bg("ui.panel");

    vista.press("q");
    assert_eq!(vista.expect_exit(), 0);
    let raw = vista.raw_output();
    let s = String::from_utf8_lossy(&raw);
    assert!(s.contains("\x1b[?1049h"), "entered the alternate screen");
    assert!(s.contains("\x1b[?1049l"), "left the alternate screen");
    assert!(s.contains("\x1b[?1000l"), "disabled mouse capture");
}

#[test]
fn ctrl_q_also_quits() {
    let repo = Fixture::plain().build();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("No file open");
    vista.press("ctrl+q");
    assert_eq!(vista.expect_exit(), 0);
}

#[test]
fn redraws_on_resize() {
    let repo = Fixture::plain().build();
    let mut vista = Vista::launch(BIN, &repo).size(120, 36).start();
    vista.expect_row(35, "1: Project");
    vista.resize(80, 24);
    vista.expect_row(23, "1: Project");
    vista.expect_text("No file open");
    vista.resize(140, 40);
    vista.expect_row(39, "1: Project");
}

#[test]
fn help_overlay_opens_and_any_key_closes_it() {
    let repo = Fixture::plain().build();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("No file open");
    vista.press("?");
    vista.expect_text("Keyboard shortcuts");
    vista.expect_text("Go to File");
    vista.expect_text("Ctrl-P");
    vista.press("x");
    vista.expect_no_text("Keyboard shortcuts");
    // `q` after closing help quits; while help is open it would only close help.
    vista.press("f1");
    vista.expect_text("Keyboard shortcuts");
    vista.press("q");
    vista.expect_no_text("Keyboard shortcuts");
    vista.press("q");
    vista.expect_exit();
}

#[test]
fn alt_1_hides_and_shows_the_project_tool_window() {
    let repo = Fixture::plain().build();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_row(1, "1: Project");
    // Project is visible and focused, so Alt-1 hides it (IntelliJ semantics).
    vista.press("alt+1");
    vista.wait_until("project panel hidden", |s| {
        !s.contents().lines().nth(1).unwrap_or("").contains("1: Project")
    });
    vista.press("alt+1");
    vista.expect_row(1, "1: Project");
    // Alt-0 swaps the left side to the Commit tool window.
    vista.press("alt+2");
    vista.expect_row(1, "2: Commit");
}

#[test]
fn hot_reloads_an_edited_theme_file() {
    let repo = Fixture::plain().build();
    let theme = "name = \"Mine\"\ninherits = \"darcula\"\n[palette]\n\"ui.panel\" = \"#112233\"\n";
    let mut vista = Vista::launch(BIN, &repo)
        .theme("mine")
        .config_file("themes/mine.toml", theme)
        .start();
    vista.expect_text("No file open");
    assert_eq!(vista.bg_at(0, 0).map(|c| c.to_hex()), Some("#112233".into()));

    // Ensure a different mtime, then edit the file.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let path = vista.config_dir().join("themes/mine.toml");
    std::fs::write(&path, theme.replace("#112233", "#445566")).unwrap();
    vista.expect_text("Reloaded look `mine`");
    vista.wait_until("top bar recoloured", |s| {
        matches!(
            s.cell(0, 0).map(|c| c.bgcolor()),
            Some(vista_e2e::vt100::Color::Rgb(0x44, 0x55, 0x66))
        )
    });

    // A broken edit keeps the old look and reports the error.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(&path, "inherits = \"darcula\"\n[palette]\n\"ui.panel\" = \"nope\"\n").unwrap();
    vista.expect_text("not a #RRGGBB colour");
}

#[test]
fn unknown_theme_falls_back_to_darcula_with_a_message() {
    let repo = Fixture::plain().build();
    let mut vista = Vista::launch(BIN, &repo).theme("does-not-exist").start();
    vista.expect_text("theme `does-not-exist` not found; using darcula");
    vista.expect_text("No file open");
}

#[test]
fn config_key_overrides_apply() {
    let repo = Fixture::plain().build();
    let mut vista = Vista::launch(BIN, &repo)
        .config("[keys]\n\"ctrl+o\" = \"help\"\n\"q\" = \"none\"\n")
        .start();
    vista.expect_text("No file open");
    vista.press("ctrl+o");
    vista.expect_text("Keyboard shortcuts");
    vista.press("esc");
    vista.expect_no_text("Keyboard shortcuts");
    vista.press("q");
    vista.settle(std::time::Duration::from_millis(300));
    vista.expect_text("No file open");
    vista.press("ctrl+q");
    vista.expect_exit();
}

#[test]
fn probe_reports_capabilities_without_a_kitty_terminal() {
    let repo = Fixture::plain().build();
    let mut vista = Vista::launch(BIN, &repo)
        .arg("--probe")
        .env("VISTA_KEYBOARD", "")
        .start();
    vista.expect_text("vista --probe");
    vista.expect_text("kitty graphics  no");
    vista.expect_text("images would use: halfblocks");
    assert_eq!(vista.expect_exit(), 0);
    // The graphics query really was sent to the terminal.
    let raw = String::from_utf8_lossy(&vista.raw_output()).into_owned();
    assert!(raw.contains("\x1b_Gi=31,s=1,v=1,a=q"));
}
