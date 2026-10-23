//! The sidebar (Alt-B): projects on top, Claude sessions below with their
//! state; a click opens a session in a terminal tab.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

const SESSIONS: &str = r#"[
  {"id": "74ab178b", "name": "product-owner", "cwd": "/r/lms-context", "kind": "background", "state": "blocked"},
  {"id": "40008faf", "name": "tree-search", "cwd": "/r/file-browser", "kind": "background", "state": "working"}
]"#;

/// A config whose session list is a file and whose "attach" prints the id.
fn config(repo: &Repo) -> String {
    let list = repo.home().join("sessions.json");
    std::fs::write(&list, SESSIONS).unwrap();
    format!(
        "[sidebar]\nsessions = \"cat '{}'\"\nattach = \"echo attached-{{id}}; cat\"\n",
        list.display()
    )
}

/// Clicks the row showing `text` in the sidebar (the first 30 columns).
fn click_row(vista: &mut Vista, text: &str) {
    let (row, col) = vista.expect_text_in(text, 0..30).position();
    vista.click(col, row);
}

#[test]
fn sessions_show_their_state_and_open_on_click() {
    let repo = Fixture::git().file("a.txt", "one\n").commit("init").build();
    let name = repo.path().file_name().unwrap().to_string_lossy().into_owned();
    let mut vista = Vista::launch(BIN, &repo)
        .config(&config(&repo))
        .env("SHELL", "/bin/sh")
        .size(150, 36)
        .start();
    vista.expect_text("1: Project");
    vista.press("alt+b");
    vista.expect_text_in(" Projects", 0..30);
    vista.expect_text_in(&format!("▸ {name}"), 0..30);
    // The project's branch under it.
    vista.expect_text_in("⎇ main", 0..30);
    // Sessions with their state: product-owner waits for input.
    vista.expect_text_in("● product-owner", 0..30).to_have_fg("ui.warning");
    vista.expect_text_in("◐ tree-search", 0..30).to_have_fg("ui.accent");
    // Its folder on the row below.
    let (row, _) = vista.expect_text_in("● product-owner", 0..30).position();
    let (folder_row, _) = vista.expect_text_in("lms-context", 0..30).position();
    assert_eq!(folder_row, row + 1);

    // A click attaches to it in a terminal tab, full height beside the sidebar.
    click_row(&mut vista, "product-owner");
    vista.expect_text("attached-74ab178b");
    vista.expect_no_text("1: Project");
    vista
        .expect_text_in("product-owner", 0..30)
        .to_have_bg("ui.selection.inactive");
    // Back to the project, and to the same tab again (not a second one).
    click_row(&mut vista, &name);
    vista.expect_text("1: Project");
    click_row(&mut vista, "product-owner");
    vista.expect_text("attached-74ab178b");
    vista.wait_until("one attached tab", |s| s.contents().matches("attached-").count() == 1);

    // Alt-B hides it (from a project; keys in a terminal go to the program).
    click_row(&mut vista, &name);
    vista.expect_text("1: Project");
    vista.press("alt+b");
    vista.expect_no_text(" Claude");
    // The session's tab is back in the top bar.
    vista.expect_text("▶ product-owner ×");
}

#[test]
fn the_sidebar_is_remembered() {
    let repo = Fixture::git().file("a.txt", "one\n").commit("init").build();
    let cfg = config(&repo);
    let mut vista = Vista::launch(BIN, &repo).config(&cfg).size(150, 36).start();
    vista.expect_text("1: Project");
    vista.press("alt+b");
    vista.expect_text_in(" Claude", 0..30);
    vista.press("ctrl+q");
    vista.expect_exit();
    let state = std::fs::read_to_string(repo.home().join(".config/vista/state.toml")).unwrap();
    assert!(state.contains("sidebar = true"), "{state}");
    let mut vista = Vista::launch(BIN, &repo).config(&cfg).size(150, 36).start();
    vista.expect_text_in(" Claude", 0..30);
    vista.expect_text_in("● product-owner", 0..30);
}

#[test]
fn dragging_the_border_resizes_it_and_the_width_is_kept() {
    let repo = Fixture::git().file("a.txt", "one\n").commit("init").build();
    let cfg = config(&repo);
    let mut vista = Vista::launch(BIN, &repo).config(&cfg).size(150, 36).start();
    vista.expect_text("1: Project");
    vista.press("alt+b");
    vista.expect_text_in(" Claude", 0..30);
    let border = |vista: &Vista, col: u16| {
        vista
            .screen_text()
            .lines()
            .nth(3)
            .and_then(|l| l.chars().nth(col as usize))
            == Some('│')
    };
    assert!(border(&vista, 29), "{}", vista.screen_text());
    vista.drag((29, 3), (41, 3));
    vista.wait_until("the border moved to column 41", |s| {
        s.contents().lines().nth(3).and_then(|l| l.chars().nth(41)) == Some('│')
    });
    vista.press("ctrl+q");
    vista.expect_exit();
    let state = std::fs::read_to_string(repo.home().join(".config/vista/state.toml")).unwrap();
    assert!(state.contains("sidebar_width = 42"), "{state}");
}

/// Background sessions write nothing to vista: a session that finishes or
/// starts waiting is noticed in the polled list, and plays its sound.
#[test]
fn a_session_that_finishes_or_needs_input_plays_a_sound() {
    use std::os::unix::fs::PermissionsExt;
    let repo = Fixture::git().file("a.txt", "one\n").commit("init").build();
    let list = repo.home().join("sessions.json");
    let write = |status: &str| {
        let json =
            format!(r#"[{{"id": "w1", "name": "worker", "cwd": "/r/x", "state": "working", "status": "{status}"}}]"#);
        std::fs::write(&list, json).unwrap();
    };
    write("busy");
    let played = repo.home().join("played");
    let player = repo.home().join("player");
    std::fs::write(
        &player,
        format!("#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{}'\n", played.display()),
    )
    .unwrap();
    std::fs::set_permissions(&player, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = format!(
        "[sidebar]\nsessions = \"cat '{}'\"\n[terminal]\nnotify_sound = \"Ping\"\nnotify_sound_done = \"Hero\"\n",
        list.display()
    );
    let mut vista = Vista::launch(BIN, &repo)
        .config(&config)
        .env("VISTA_SOUND_PLAYER", &player.to_string_lossy())
        .size(150, 36)
        .start();
    vista.expect_text("1: Project");
    vista.press("alt+b");
    vista.expect_text_in("◐ worker", 0..30);
    // Already working when first seen: quiet.
    assert!(!played.exists());
    let sounds = || std::fs::read_to_string(&played).unwrap_or_default();
    write("idle");
    vista.expect_text_in("✓ worker", 0..30).to_have_fg("git.added");
    vista.wait_until("the done sound", |_| sounds().lines().count() == 1);
    assert_eq!(sounds().trim(), "/System/Library/Sounds/Hero.aiff");
    write("busy");
    vista.expect_text_in("◐ worker", 0..30);
    write("waiting");
    vista.expect_text_in("● worker", 0..30).to_have_fg("ui.warning");
    vista.wait_until("the needs-input sound", |_| sounds().lines().count() == 2);
    assert_eq!(sounds().lines().last(), Some("/System/Library/Sounds/Ping.aiff"));
}
