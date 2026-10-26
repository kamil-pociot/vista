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
