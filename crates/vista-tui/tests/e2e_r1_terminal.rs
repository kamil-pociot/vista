//! Terminal tabs: programs (`claude agents`, any command) full screen,
//! next to the project tabs.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");
const CONFIG: &str = "[terminal]\ncommands = [\"cat\"]\n";

fn repo() -> Repo {
    Fixture::git().file("alpha.txt", "in alpha\n").commit("init").build()
}

/// Clicks a tab in the top bar by its label.
fn click_tab(vista: &mut Vista, label: &str) {
    let (row, col) = vista.expect_text_in(label, 0..140).position();
    assert_eq!(row, 0);
    vista.click(col + 1, row);
}

#[test]
fn run_a_command_full_screen_and_switch_back_and_forth() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).config(CONFIG).size(140, 30).start();
    vista.expect_text("1: Project");

    // Ctrl-O lists the configured command; Enter runs it in a new tab.
    vista.press("ctrl+o");
    vista.type_text("cat");
    vista.expect_text("▷ cat");
    vista.press("enter");
    vista.expect_text("▶ cat × ");
    // Full screen: no tree.
    vista.expect_no_text("1: Project");
    // Keys go to the program (the terminal echoes, cat repeats).
    vista.type_text("hello there");
    vista.press("enter");
    vista.wait_until("cat echoed the line", |s| {
        s.contents().matches("hello there").count() >= 2
    });

    // Ctrl-\ goes back to the project; the terminal keeps its tab.
    vista.press("ctrl+\\");
    vista.expect_text("1: Project");
    vista.expect_text("▶ cat × ");
    // Clicking the tab shows it again, as it was.
    click_tab(&mut vista, "▶ cat");
    vista.expect_no_text("1: Project");
    vista.expect_text("hello there");

    // `>command` runs anything; when it ends, Enter closes its tab.
    vista.press("ctrl+\\");
    vista.press("ctrl+o");
    vista.type_text(">echo done-$((40+2))");
    vista.press("enter");
    vista.expect_text("done-42");
    vista.expect_text("exited — Enter closes the tab");
    vista.press("enter");
    vista.expect_no_text("▶ echo");
    vista.expect_text("1: Project");

    // × closes a terminal tab (and stops the program).
    let (row, col) = vista.expect_text_in("▶ cat × ", 0..140).position();
    vista.click(col + "▶ cat ".chars().count() as u16, row);
    vista.expect_no_text("▶ cat");
    vista.expect_text("1: Project");
}

#[test]
fn quitting_with_a_running_terminal_asks_first() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo).config(CONFIG).size(140, 30).start();
    vista.expect_text("1: Project");
    vista.press("ctrl+o");
    vista.type_text("cat");
    vista.expect_text("▷ cat");
    vista.press("enter");
    vista.expect_text("▶ cat × ");
    vista.press("ctrl+\\");
    vista.expect_text("1: Project");
    vista.press("ctrl+q");
    vista.expect_text("1 terminal tab(s) still running");
    vista.press("ctrl+q");
    vista.expect_exit();
}

#[test]
fn a_lone_angle_bracket_opens_a_shell_tab() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo)
        .config(CONFIG)
        .env("SHELL", "/bin/sh")
        .size(140, 30)
        .start();
    vista.expect_text("1: Project");
    vista.press("ctrl+o");
    vista.type_text(">");
    vista.expect_text("▷ shell");
    vista.press("enter");
    vista.expect_text("▶ sh × ");
    vista.type_text("echo hi-$((1+2))");
    vista.press("enter");
    vista.expect_text("hi-3");
}

#[test]
fn terminal_tool_window_runs_a_shell_in_the_project() {
    let repo = repo();
    let name = repo.path().file_name().unwrap().to_string_lossy().into_owned();
    let mut vista = Vista::launch(BIN, &repo)
        .config(CONFIG)
        .env("SHELL", "/bin/sh")
        .size(140, 36)
        .start();
    vista.expect_text("1: Project");
    // Alt-T: a shell under the editor, in the project folder; the tree stays.
    vista.press("alt+t");
    vista.expect_text("T: Terminal");
    vista.type_text("echo where=$(basename \"$PWD\")");
    vista.press("enter");
    vista.expect_text(&format!("where={name}"));
    vista.expect_text("1: Project");
    // Ctrl-\ back to the editor; Alt-T returns to the same shell.
    vista.press("ctrl+\\");
    vista.press("alt+t");
    vista.type_text("echo again");
    vista.press("enter");
    vista.expect_text("again");
    vista.expect_text(&format!("where={name}"));
    // The shell ends: Enter starts a new one.
    vista.type_text("exit");
    vista.press("enter");
    vista.expect_text("Enter starts a new shell");
    vista.press("enter");
    vista.expect_no_text("Enter starts a new shell");
    vista.type_text("echo fresh");
    vista.press("enter");
    vista.expect_text("fresh");
    // Alt-T while it has focus hides it.
    vista.press("alt+t");
    vista.expect_no_text("back to the editor");
}

#[test]
fn terminal_tabs_can_be_dragged_into_a_new_order() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo)
        .config("[terminal]\ncommands = [\"cat\", \"sleep 1000\"]\n")
        .size(140, 30)
        .start();
    vista.expect_text("1: Project");
    for cmd in ["cat", "sleep"] {
        // Back to the project first: a terminal tab takes every key but Ctrl-\.
        vista.press("ctrl+\\");
        vista.press("ctrl+o");
        vista.type_text(cmd);
        vista.expect_text(&format!("▷ {cmd}"));
        vista.press("enter");
        vista.expect_text(&format!("▶ {cmd}"));
    }
    let (row, from) = vista.expect_text_in("▶ cat ×", 0..140).position();
    let (_, to) = vista.expect_text_in("▶ sleep 1000 ×", 0..140).position();
    assert!(from < to);
    vista.drag((from + 2, row), (to + 4, row));
    vista.wait_until("cat after sleep", |s| {
        let top = s.contents().lines().next().unwrap_or_default().to_string();
        top.find("▶ sleep").zip(top.find("▶ cat")).is_some_and(|(s, c)| s < c)
    });
    // The dragged terminal is the one shown.
    vista.type_text("still here");
    vista.press("enter");
    vista.wait_until("cat echoes", |s| s.contents().matches("still here").count() >= 2);
}

/// Claude Code asks the terminal for a desktop notification when it waits
/// for input; vista passes it on and marks the tab until it's looked at.
#[test]
fn a_notification_reaches_the_outer_terminal_and_marks_the_tab() {
    let repo = Fixture::git()
        .file(
            "notify.sh",
            "while [ ! -e go ]; do sleep 0.1; done\nprintf '\\033]777;notify;Claude Code;needs input\\007'\ncat\n",
        )
        .commit("init")
        .build();
    let mut vista = Vista::launch(BIN, &repo)
        .config(CONFIG)
        .env("SHELL", "/bin/sh")
        .size(140, 30)
        .start();
    vista.expect_text("1: Project");
    vista.press("ctrl+o");
    vista.type_text(">sh notify.sh");
    vista.press("enter");
    vista.expect_text("▶ sh notify.sh × ");
    // Back to the project before the script notifies.
    vista.press("ctrl+\\");
    vista.expect_text("1: Project");
    repo.write("go", "");
    vista.expect_output(b"\x1b]777;notify;Claude Code;needs input\x07");
    vista.expect_text("● sh notify.sh").to_have_fg("ui.warning");
    // Shown once: the mark goes.
    click_tab(&mut vista, "● sh notify.sh");
    vista.expect_text("▶ sh notify.sh");
    vista.expect_no_text("● sh notify.sh");
    vista.press("ctrl+\\");
    vista.expect_text("1: Project");
    vista.expect_text("▶ sh notify.sh");
}

/// The same for the Terminal tool window's shell: its project's tab is
/// marked while something else is on screen.
#[test]
fn a_notification_from_the_tool_window_marks_the_project_tab() {
    let repo = repo();
    let name = repo.path().file_name().unwrap().to_string_lossy().into_owned();
    let mut vista = Vista::launch(BIN, &repo)
        .config(CONFIG)
        .env("SHELL", "/bin/sh")
        .size(140, 36)
        .start();
    vista.expect_text("1: Project");
    vista.press("alt+t");
    vista.expect_text("T: Terminal");
    vista.type_text("while [ ! -e go ]; do sleep 0.1; done; printf '\\033]9;shell wants you\\007'");
    vista.press("enter");
    // A terminal tab covers the project.
    vista.press("ctrl+\\");
    vista.press("ctrl+o");
    vista.type_text("cat");
    vista.expect_text("▷ cat");
    vista.press("enter");
    vista.expect_text("▶ cat × ");
    vista.expect_no_text("1: Project");
    repo.write("go", "");
    vista.expect_output(b"\x1b]9;shell wants you\x07");
    vista.expect_text(&format!("● {name}")).to_have_fg("ui.warning");
    vista.press("ctrl+\\");
    vista.expect_text("1: Project");
    vista.expect_no_text(&format!("● {name}"));
}

/// Shift-Enter is a newline in Claude Code: it reaches the program as
/// ESC CR, not as a plain Enter.
#[test]
fn shift_enter_reaches_the_program_as_esc_cr() {
    let repo = repo();
    let mut vista = Vista::launch(BIN, &repo)
        .config(CONFIG)
        .env("SHELL", "/bin/sh")
        .size(140, 30)
        .start();
    vista.expect_text("1: Project");
    vista.press("ctrl+o");
    vista.type_text(">stty raw -echo; echo ready-$((1+1)); dd bs=1 count=2 2>/dev/null | od -An -tx1");
    vista.press("enter");
    vista.expect_text("ready-2");
    // Shift-Enter as a kitty-keyboard terminal (Ghostty) reports it.
    vista.send_raw(b"\x1b[13;2u");
    vista.wait_until("the program read ESC CR", |s| {
        let contents = s.contents();
        let words: Vec<&str> = contents.split_whitespace().collect();
        words.windows(2).any(|w| w == ["1b", "0d"])
    });
}

/// Inside herdr, which doesn't pass notifications on, vista hands them to
/// `herdr notification show` as well.
#[test]
fn inside_herdr_a_notification_goes_to_herdr() {
    use std::os::unix::fs::PermissionsExt;
    let repo = repo();
    let bin = tempfile::tempdir().unwrap();
    let herdr = bin.path().join("herdr");
    let out = bin.path().join("args");
    std::fs::write(
        &herdr,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}.tmp' && mv '{0}.tmp' '{0}'\n",
            out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&herdr, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut vista = Vista::launch(BIN, &repo)
        .config(CONFIG)
        .env("SHELL", "/bin/sh")
        .env("HERDR_ENV", "1")
        .env("HERDR_PANE_ID", "w1:p1")
        .env("HERDR_BIN_PATH", &herdr.to_string_lossy())
        .size(140, 30)
        .start();
    vista.expect_text("1: Project");
    vista.press("ctrl+o");
    vista.type_text(">printf '\\033]777;notify;Claude Code;needs input\\007'; cat");
    vista.press("enter");
    vista.expect_output(b"\x1b]777;notify;Claude Code;needs input\x07");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while !out.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let args = std::fs::read_to_string(&out).expect("herdr was run");
    let title = "Claude Code · printf '\\033]777;notify;Claude Code;needs input\\007'; cat";
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        [
            "notification",
            "show",
            title,
            "--body",
            "needs input",
            "--sound",
            "request"
        ]
    );
}

/// A notification plays `[terminal] notify_sound`, also with its tab on screen
/// (where Ghostty shows nothing); `none` keeps quiet.
#[test]
fn a_notification_plays_the_notify_sound() {
    use std::os::unix::fs::PermissionsExt;
    for (setting, want) in [("Ping", Some("/System/Library/Sounds/Ping.aiff")), ("none", None)] {
        let repo = repo();
        let bin = tempfile::tempdir().unwrap();
        let player = bin.path().join("player");
        let out = bin.path().join("played");
        std::fs::write(
            &player,
            format!("#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{}'\n", out.display()),
        )
        .unwrap();
        std::fs::set_permissions(&player, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut vista = Vista::launch(BIN, &repo)
            .config(&format!(
                "[terminal]\ncommands = [\"cat\"]\nnotify_sound = \"{setting}\"\n"
            ))
            .env("SHELL", "/bin/sh")
            .env("VISTA_SOUND_PLAYER", &player.to_string_lossy())
            .size(140, 30)
            .start();
        vista.expect_text("1: Project");
        vista.press("ctrl+o");
        vista.type_text(">printf '\\033]9;Claude needs you\\007'; cat");
        vista.press("enter");
        vista.expect_output(b"\x1b]9;Claude needs you\x07");
        match want {
            Some(file) => {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
                while !out.exists() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                let played = std::fs::read_to_string(&out).expect("a sound was played");
                assert_eq!(played.lines().collect::<Vec<_>>(), [file]);
            }
            None => {
                vista.settle(std::time::Duration::from_millis(300));
                assert!(!out.exists(), "no sound with notify_sound = none");
            }
        }
    }
}
