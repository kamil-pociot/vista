//! Pictures: drawn partly when scrolled half out of view, and opened full
//! screen (click or `v`) to zoom and move around.

use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// A PNG of `w`×`h` pixels, in stripes so every part has something to draw.
fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |_, y| {
        if (y / 4) % 2 == 0 {
            image::Rgb([200, 60, 60])
        } else {
            image::Rgb([60, 60, 200])
        }
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn project() -> Repo {
    Fixture::git()
        .file("pic.png", png(64, 32))
        .file("tall.png", png(40, 800))
        .file("doc.md", "# Doc\n\nOne.\n\nTwo.\n\n![tall](tall.png)\n\nend\n")
        .commit("init")
        .build()
}

#[test]
fn an_image_half_in_view_shows_its_part() {
    // Half-blocks (the harness's default): the image is 20 rows, the view less.
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(100, 16).open("doc.md").start();
    vista.expect_text("Two.");
    // Cut at the bottom of the view, not left out.
    vista.expect_text("▀");
    let (row, col) = vista.expect_text("Doc").position();
    for _ in 0..4 {
        vista.scroll_down(col, row);
    }
    vista.expect_no_text("Two.");
    // Its top above the view: the rest still drawn.
    vista.expect_text("▀");
}

#[test]
fn an_image_opens_full_screen_to_zoom() {
    let repo = project();
    let mut vista = Vista::launch(BIN, &repo).size(100, 30).open("pic.png").start();
    vista.expect_text("PNG 64×32");
    vista.press("v");
    vista.expect_text("Esc close");
    vista.expect_no_text("PNG 64×32");
    let percent = |vista: &mut Vista| -> u32 {
        let top = vista.row_text(0);
        let word = top.split_whitespace().find(|w| w.ends_with('%')).expect("a zoom level");
        word.trim_end_matches('%').parse().unwrap()
    };
    vista.expect_text("%");
    let fit = percent(&mut vista);
    assert!(fit > 100, "a small image is enlarged to fill the screen: {fit}%");
    vista.press("+");
    vista.settle(std::time::Duration::from_millis(100));
    assert!(percent(&mut vista) > fit);
    vista.press("esc");
    vista.expect_text("PNG 64×32");
    // A click on the image opens it too.
    let (row, col) = vista.expect_text("PNG 64×32").position();
    vista.click(col + 4, row + 3);
    vista.expect_text("Esc close");
    vista.press("q");
    vista.expect_text("PNG 64×32");
}
