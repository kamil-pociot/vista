//! M8 end-to-end: built-in looks and switching between them.

use vista_e2e::{Fixture, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

#[test]
fn quiet_look_has_key_hints_and_status_letters() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).theme("quiet").start();
    vista.expect_text("1: project");
    vista.expect_text("find in files");
    // Status letters next to changed files.
    vista.expect_text("Cargo.lock").to_have_fg("git.modified");
    vista.wait_until("letter M for Cargo.lock", |s| {
        s.contents()
            .lines()
            .any(|l| l.contains("Cargo.lock") && l.trim_end().ends_with('M'))
    });
    // The background is the look's own.
    assert_eq!(vista.bg_at(60, 10).map(|c| c.to_hex()), Some("#16181D".into()));
}

#[test]
fn paper_look_is_light_with_rounded_panels() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).theme("paper").start();
    vista.expect_text("╭ 1: Project");
    vista.expect_text("╰");
    vista.expect_text("◫");
    vista.wait_until("white editor background", |s| {
        matches!(
            s.cell(10, 80).map(|c| c.bgcolor()),
            Some(vista_e2e::vt100::Color::Rgb(0xFF, 0xFF, 0xFF))
        )
    });
}

#[test]
fn ctrl_t_cycles_looks() {
    let repo = Fixture::rust_project().build();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("1: Project");
    vista.press("ctrl+t");
    vista.expect_text("Look: Paper Light");
    vista.expect_text("╭ 1: Project");
    vista.press("ctrl+t");
    vista.expect_text("Look: Quiet Modal");
    // Then herdr's themes, and round to the start.
    vista.press("ctrl+t");
    vista.expect_text("Look: Catppuccin Mocha");
    for name in ["Catppuccin Latte", "Tokyo Night"] {
        vista.press("ctrl+t");
        vista.expect_text(&format!("Look: {name}"));
    }
    for _ in 0..14 {
        vista.press("ctrl+t");
    }
    vista.expect_text("Look: Vesper");
    vista.press("ctrl+t");
    vista.expect_text("Look: Darcula Classic");
}

/// `theme = "herdr"` takes herdr's theme, and follows it while vista runs.
#[test]
fn the_look_follows_herdrs_theme() {
    let repo = Fixture::git().file("a.txt", "one\n").commit("init").build();
    let herdr = repo.home().join(".config/herdr/config.toml");
    std::fs::create_dir_all(herdr.parent().unwrap()).unwrap();
    let set = |theme: &str| std::fs::write(&herdr, format!("onboarding = false\n[theme]\n{theme}\n")).unwrap();
    set("name = \"nord\"");
    // `.theme("")`: no VISTA_THEME, so the config's `theme = "herdr"` counts.
    let mut vista = Vista::launch(BIN, &repo)
        .theme("")
        .config("theme = \"herdr\"\n")
        .start();
    vista.expect_text("Nord");
    let nord = vista_theme::ThemeLoader::new(None).load("nord").unwrap();
    let bg = nord.color("ui.bg");
    assert_eq!(vista.bg_at(60, 10), Some(bg), "Nord's background");
    // Changed in herdr: vista follows.
    set("name = \"dracula\"");
    vista.expect_text("Look: Dracula (following herdr)");
    // auto_switch on a Linux / dark machine: the dark name.
    set("name = \"nord\"\nauto_switch = true\ndark_name = \"kanagawa\"\nlight_name = \"kanagawa-lotus\"");
    if !cfg!(target_os = "macos") {
        vista.expect_text("Look: Kanagawa Wave (following herdr)");
    } else {
        vista.expect_text("(following herdr)");
    }
    // herdr's "terminal" theme has no vista look: the look stays.
    set("name = \"terminal\"");
    vista.expect_text("herdr's theme \"terminal\" has no vista look");
}
