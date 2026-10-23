//! The tree / Commit panel can be resized: drag the divider, or
//! Ctrl-Shift-←/→; the width is remembered.

use vista_e2e::{Fixture, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// Column of the divider (│) on screen row `row`, right of the tree.
fn divider(vista: &Vista, row: u16) -> u16 {
    let line: Vec<char> = vista.row_text(row).chars().collect();
    (10..line.len()).find(|&i| line[i] == '│').expect("divider") as u16
}

#[test]
fn drag_the_divider_or_use_keys_and_the_width_is_kept() {
    let repo = Fixture::git()
        .file("a_rather_long_file_name_that_needs_room.txt", "x\n")
        .commit("init")
        .build();
    let mut vista = Vista::launch(BIN, &repo).size(140, 30).start();
    vista.expect_text("1: Project");
    let start = divider(&vista, 5);

    // Drag it 15 columns to the right.
    vista.drag((start, 5), (start + 15, 5));
    vista.wait_until("divider moved right", move |s| {
        s.contents()
            .lines()
            .nth(5)
            .is_some_and(|l| l.chars().nth(start as usize + 15) == Some('│'))
    });
    vista.expect_text("a_rather_long_file_name_that_needs_room.txt");

    // Ctrl-Shift-← narrows by 4, Ctrl-Shift-→ widens back.
    vista.press("ctrl+shift+left");
    vista.wait_until("4 narrower", move |s| {
        s.contents()
            .lines()
            .nth(5)
            .is_some_and(|l| l.chars().nth(start as usize + 11) == Some('│'))
    });
    vista.press("ctrl+shift+right");
    vista.press("ctrl+shift+right");
    vista.wait_until("4 wider than the drag", move |s| {
        s.contents()
            .lines()
            .nth(5)
            .is_some_and(|l| l.chars().nth(start as usize + 19) == Some('│'))
    });
    // It can't squeeze the editor away.
    for _ in 0..40 {
        vista.press("ctrl+shift+right");
    }
    vista.wait_until("editor keeps 20 columns", |s| {
        s.contents().lines().nth(5).is_some_and(|l| {
            let c: Vec<char> = l.chars().collect();
            (10..c.len()).find(|&i| c[i] == '│').is_some_and(|i| i <= 140 - 20)
        })
    });
    for _ in 0..40 {
        vista.press("ctrl+shift+left");
    }
    vista.wait_until("narrowest tree", |s| {
        s.contents()
            .lines()
            .nth(5)
            .is_some_and(|l| l.chars().nth(16) == Some('│'))
    });
    vista.press("ctrl+q");
    vista.expect_exit();

    // Remembered: the narrowest width (16) after a restart.
    let state = std::fs::read_to_string(repo.home().join(".config/vista/state.toml")).unwrap();
    assert!(state.contains("left_width = 16"), "{state}");
    let mut vista = Vista::launch(BIN, &repo).size(140, 30).start();
    vista.expect_text("1: Project");
    vista.wait_until("narrow tree after restart", |s| {
        s.contents()
            .lines()
            .nth(5)
            .is_some_and(|l| l.chars().nth(16) == Some('│'))
    });
}
