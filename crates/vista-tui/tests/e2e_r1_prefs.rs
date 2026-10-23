//! The Preferences page (`Ctrl-,`, `⚙`): settings change in place, apply at
//! once and are remembered.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn repo() -> Repo {
    Fixture::git().file("a.txt", "one\n").commit("init").build()
}

fn open_prefs(vista: &mut Vista) {
    let (row, col) = vista.expect_text("⚙").position();
    vista.click(col, row);
    vista.expect_text(" Preferences ");
}

/// Whether the row for `label` shows `value` (at its right end, inside the box).
fn shows(screen: &vt100::Screen, label: &str, value: &str) -> bool {
    screen.contents().lines().filter(|l| l.contains(label)).any(|l| {
        l.split('│')
            .any(|cell| cell.contains(label) && cell.trim_end().ends_with(value))
    })
}

fn state(repo: &Repo) -> String {
    std::fs::read_to_string(repo.home().join(".config/vista/state.toml")).unwrap_or_default()
}

#[test]
fn settings_change_in_place_and_are_remembered() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).size(150, 40).start();
    vista.expect_text("1: Project");
    open_prefs(&mut vista);
    vista.expect_text("Appearance");
    vista.expect_text("Find in Files");
    vista.wait_until("line numbers on", |s| shows(s, "Line numbers", "On"));
    // Keyboard: down to Line numbers, Space flips it.
    vista.press("down");
    vista.press("down");
    vista.expect_text("Line numbers in code").to_have_fg("ui.dim");
    vista.press("space");
    // Its own row: other settings show Off from the start.
    vista.wait_until("line numbers off", |s| shows(s, "Line numbers", "Off"));
    assert!(state(&repo).contains("line_numbers = false"), "{}", state(&repo));
    // A click on a row changes it.
    let (row, col) = vista.expect_text("Wrap code").position();
    vista.click(col, row);
    vista.wait_until("wrap code on", |s| shows(s, "Wrap code", "On"));
    assert!(state(&repo).contains("wrap_code = true"), "{}", state(&repo));
    // Esc closes; the setting stays.
    vista.press("esc");
    vista.expect_no_text(" Preferences ");
}

#[test]
fn fill_background_paints_the_terminal_background_areas() {
    let repo = repo();
    // Nord is one of herdr's looks: the editor shows the terminal's background.
    let mut vista = Vista::launch(BIN, &repo).theme("nord").size(150, 40).start();
    vista.expect_text("1: Project");
    let nord = vista_theme::ThemeLoader::new(None).load("nord").unwrap();
    let editor = |vista: &Vista| vista.bg_at(100, 20);
    assert_eq!(editor(&vista), None, "the terminal's own background");
    open_prefs(&mut vista);
    vista.press("down");
    vista.press("space");
    vista.press("esc");
    vista.expect_no_text(" Preferences ");
    assert_eq!(editor(&vista), Some(nord.color("ui.bg")), "Nord's own background");
    assert!(state(&repo).contains("fill_background = true"), "{}", state(&repo));
    // And back.
    open_prefs(&mut vista);
    vista.press("down");
    vista.press("space");
    vista.press("esc");
    vista.expect_no_text(" Preferences ");
    assert_eq!(editor(&vista), None);
}

#[test]
fn the_look_can_follow_herdr() {
    let repo = repo();
    let herdr = repo.home().join(".config/herdr/config.toml");
    std::fs::create_dir_all(herdr.parent().unwrap()).unwrap();
    std::fs::write(&herdr, "[theme]\nname = \"nord\"\n").unwrap();
    let mut vista = Vista::launch(BIN, &repo).theme("").size(150, 40).start();
    vista.expect_text("1: Project");
    open_prefs(&mut vista);
    vista.expect_text("◂ Darcula Classic ▸");
    // Back from Darcula, the first look: Follow herdr.
    vista.press("left");
    vista.expect_text("◂ Follow herdr (Nord) ▸");
    assert!(state(&repo).contains("look = \"herdr\""), "{}", state(&repo));
    vista.press("right");
    vista.expect_text("◂ Darcula Classic ▸");
}
