//! Several projects: tabs in the top bar, the Projects popup (Ctrl-O),
//! switching, closing, and reopening the open projects after a restart.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn project(file: &str, content: &str) -> Repo {
    Fixture::git().file(file, content).commit("init").build()
}

fn name(repo: &Repo) -> String {
    repo.path().file_name().unwrap().to_string_lossy().into_owned()
}

/// Adds `repo` as a project by typing its path in the Projects popup.
fn add(vista: &mut Vista, repo: &Repo) {
    vista.press("ctrl+o");
    vista.expect_text("Projects");
    let path = repo.path().canonicalize().unwrap();
    vista.type_text(&path.display().to_string());
    vista.expect_text(&format!("+ {}", name(repo)));
    vista.press("enter");
}

/// Clicks a project's tab in the top bar (or its ×).
fn click_tab(vista: &mut Vista, project: &str, close: bool) {
    let (row, col) = vista.expect_text_in(&format!(" {project} "), 0..120).position();
    let col = if close { col + project.len() as u16 + 2 } else { col + 1 };
    assert_eq!(row, 0, "project tabs are in the top bar");
    vista.click(col, row);
}

#[test]
fn add_switch_and_close_projects() {
    let a = project("alpha.txt", "in alpha\n");
    let b = project("beta.txt", "in beta\n");
    let (an, bn) = (name(&a), name(&b));
    let mut vista = Vista::launch(BIN, &a).size(160, 40).start();
    vista.expect_text_in("alpha.txt", 0..40);
    vista.expect_text(&format!(" {an} "));

    // Ctrl-O, type a folder path: the project opens and becomes active.
    add(&mut vista, &b);
    vista.expect_text_in("beta.txt", 0..40);
    vista.expect_no_text("alpha.txt");
    vista.expect_text(&format!(" {an} × "));
    vista.expect_text(&format!(" {bn} × "));
    // Work in beta: open its file.
    vista.press("ctrl+p");
    vista.type_text("beta");
    vista.expect_text_in("beta.txt", 20..100).to_have_bg("ui.selection");
    vista.press("enter");
    vista.expect_text("in beta");

    // Clicking alpha's tab switches back; beta keeps its open file.
    click_tab(&mut vista, &an, false);
    vista.expect_text_in("alpha.txt", 0..40);
    vista.expect_no_text("in beta");
    vista.press("alt+.");
    vista.expect_text("in beta");
    vista.press("alt+,");
    vista.expect_text_in("alpha.txt", 0..40);

    // The popup switches too (it starts on the next project).
    vista.press("ctrl+o");
    vista.expect_text("Projects");
    vista.press("enter");
    vista.expect_text("in beta");

    // × closes beta; alpha is left, without ×.
    click_tab(&mut vista, &bn, true);
    vista.expect_text_in("alpha.txt", 0..40);
    vista.expect_no_text(&format!(" {bn} "));
    vista.expect_text(&format!(" {an}   + "));
    // The last project can't be closed, only quit.
    vista.press("ctrl+o");
    vista.press("ctrl+w");
    vista.expect_text("This is the only open project");

    // beta is now a recent project: the popup reopens it.
    vista.press("ctrl+o");
    vista.type_text(&bn[bn.len() - 6..]);
    vista.expect_text(&format!("  {bn}"));
    vista.press("enter");
    vista.expect_text_in("beta.txt", 0..40);
}

#[test]
fn open_projects_come_back_after_a_restart() {
    let a = project("alpha.txt", "in alpha\n");
    let b = project("beta.txt", "in beta\n");
    let (an, bn) = (name(&a), name(&b));
    let mut vista = Vista::launch(BIN, &a).size(160, 40).start();
    vista.expect_text_in("alpha.txt", 0..40);
    add(&mut vista, &b);
    vista.expect_text_in("beta.txt", 0..40);
    vista.press("q");
    vista.expect_exit();

    // Same home (settings), started on alpha again: beta is back as a tab.
    let mut vista = Vista::launch(BIN, &a).size(160, 40).start();
    vista.expect_text_in("alpha.txt", 0..40);
    vista.expect_text(&format!(" {an} × "));
    vista.expect_text(&format!(" {bn} × "));
    click_tab(&mut vista, &bn, false);
    vista.expect_text_in("beta.txt", 0..40);
}

#[test]
fn folders_complete_while_typing_a_path_or_a_name() {
    let base = tempfile::tempdir().unwrap();
    let base = base.path().canonicalize().unwrap();
    for (dir, file) in [
        ("project-one", "one.txt"),
        ("project-two", "two.txt"),
        ("other", "o.txt"),
    ] {
        std::fs::create_dir(base.join(dir)).unwrap();
        std::fs::write(base.join(dir).join(file), "x\n").unwrap();
    }
    let a = project("alpha.txt", "in alpha\n");
    let mut vista = Vista::launch(BIN, &a).size(160, 40).start();
    vista.expect_text_in("alpha.txt", 0..40);

    // Part of a path: the matching folders are listed; Enter opens one.
    vista.press("ctrl+o");
    vista.type_text(&format!("{}/proj", base.display()));
    vista.expect_text("+ project-one");
    vista.expect_text("+ project-two");
    vista.expect_no_text("+ other");
    vista.press("enter");
    vista.expect_text_in("one.txt", 0..40);

    // Just a few letters: folders next to open projects are found too.
    vista.press("ctrl+o");
    vista.type_text("two");
    vista.expect_text("+ project-two");
    vista.press("enter");
    vista.expect_text_in("two.txt", 0..40);
    vista.expect_text(" project-one × ");
    vista.expect_text(" project-two × ");
}
