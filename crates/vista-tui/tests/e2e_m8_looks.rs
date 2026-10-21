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
    vista.press("ctrl+t");
    vista.expect_text("Look: Darcula Classic");
}
