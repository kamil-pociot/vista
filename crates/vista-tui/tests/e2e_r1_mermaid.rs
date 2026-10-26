//! Mermaid diagrams in Markdown: a picture where the terminal shows real
//! pixels (kitty graphics), else drawn with box-drawing text.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

fn project() -> Repo {
    Fixture::git()
        .file(
            "flow.md",
            "# Flow\n\n```mermaid\nsequenceDiagram\n  User->>vista: open README.md\n  vista-->>User: a diagram\n```\n\n\
             ```mermaid\npie title Pets\n  \"Dogs\" : 3\n  \"Cats\" : 1\n```\n\nThe end.\n",
        )
        .commit("init")
        .build()
}

#[test]
fn diagrams_are_drawn_as_text_without_kitty_graphics() {
    // The harness uses half-blocks: too coarse for a diagram's labels.
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(120, 40).open("flow.md").start();
    vista.expect_text("open README.md");
    vista.expect_text("┌──────┐");
    vista.expect_no_text("sequenceDiagram");
    // A pie chart has no text form: its source, as a code block.
    vista.expect_text("pie title Pets").to_have_bg("ui.panel.alt");
    vista.expect_text("The end.");
    // `m` shows the source; back again.
    vista.press("m");
    vista.expect_text("sequenceDiagram");
    vista.press("m");
    vista.expect_text("open README.md");
}

#[test]
fn diagrams_become_pictures_with_kitty_graphics() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo)
        .env("VISTA_IMAGE_PROTOCOL", "kitty")
        .size(120, 60)
        .open("flow.md")
        .start();
    vista.expect_text("Flow");
    // The picture is made in the background, then takes the text's place.
    vista.expect_raw(b"\x1b_Gq=2,", "a kitty graphics transmission");
    vista.expect_no_text("open README.md");
    vista.expect_no_text("pie title Pets");
    vista.expect_text("The end.");
}

/// Kitty's placeholder character: cells showing part of a picture.
const PLACEHOLDER: char = '\u{10EEEE}';

/// Rows (from the top) that show part of a kitty picture.
fn picture_rows(vista: &Vista) -> Vec<u16> {
    vista
        .screen_text()
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains(PLACEHOLDER))
        .map(|(i, _)| i as u16)
        .collect()
}

fn wait_for_picture(vista: &mut Vista) -> Vec<u16> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let rows = picture_rows(vista);
        if !rows.is_empty() || std::time::Instant::now() > deadline {
            return rows;
        }
        vista.settle(std::time::Duration::from_millis(50));
    }
}

#[test]
fn a_diagram_scrolled_half_out_of_view_still_shows_its_part() {
    let tall = "# Tall\n\n```mermaid\nflowchart TD\n  A[One] --> B[Two]\n  B --> C[Three]\n  C --> D[Four]\n  D --> E[Five]\n```\n\nbelow\n";
    let repo = Fixture::git().file("tall.md", tall).commit("init").build();
    let mut vista = Vista::launch(BIN, &repo)
        .env("VISTA_IMAGE_PROTOCOL", "kitty")
        .size(100, 20)
        .open("tall.md")
        .start();
    let rows = wait_for_picture(&mut vista);
    assert!(!rows.is_empty(), "the diagram is shown, cut at the bottom");
    // Scrolled down: its top is above the view, the rest still drawn.
    let (row, col) = vista.expect_text("Tall").position();
    for _ in 0..3 {
        vista.scroll_down(col, row);
    }
    vista.settle(std::time::Duration::from_millis(200));
    vista.expect_no_text("Tall");
    let after = picture_rows(&vista);
    assert!(!after.is_empty(), "part of the diagram is still drawn");
    assert!(after[0] < rows[0], "moved up: {rows:?} → {after:?}");
    assert!(after[0] <= 3, "from the top of the view: {after:?}");
}

#[test]
fn a_click_opens_a_diagram_full_screen_to_zoom_and_move() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo)
        .env("VISTA_IMAGE_PROTOCOL", "kitty")
        .size(120, 50)
        .open("flow.md")
        .start();
    let rows = wait_for_picture(&mut vista);
    let row = *rows.first().expect("a picture");
    let col = vista.row_text(row).chars().position(|c| c == PLACEHOLDER).unwrap() as u16;
    vista.click(col + 2, row + 1);
    vista.expect_text("Mermaid diagram");
    vista.expect_text("Esc close");
    vista.expect_no_text("flow.md ×");
    let percent = |vista: &mut Vista| -> u32 {
        let top = vista.row_text(0);
        let word = top.split_whitespace().find(|w| w.ends_with('%')).expect("a zoom level");
        word.trim_end_matches('%').parse().unwrap()
    };
    vista.expect_text("%");
    let fit = percent(&mut vista);
    vista.press("+");
    vista.settle(std::time::Duration::from_millis(100));
    // (Shown rounded: within one.)
    assert!(percent(&mut vista).abs_diff((fit as f64 * 1.25).round() as u32) <= 1);
    // The wheel zooms too; `0` fits again.
    vista.scroll_up(60, 25);
    vista.settle(std::time::Duration::from_millis(100));
    assert!(percent(&mut vista) > fit);
    vista.press("0");
    vista.settle(std::time::Duration::from_millis(100));
    assert_eq!(percent(&mut vista), fit);
    vista.press("esc");
    vista.expect_text("flow.md ×");
    // `v` opens the first picture in view without the mouse.
    vista.press("v");
    vista.expect_text("Mermaid diagram");
    vista.press("q");
    vista.expect_text("flow.md ×");
}
