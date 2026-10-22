//! Tabs have a close button (×), like IntelliJ.

use vista_e2e::{Fixture, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// Opens `name` with Go to File.
fn open(vista: &mut Vista, name: &str, content: &str) {
    vista.press("ctrl+p");
    vista.type_text(name);
    // The popup (over the tree) has the file selected.
    vista
        .expect_text_in(&format!("{name}.txt"), 4..40)
        .to_have_bg("ui.selection");
    vista.press("enter");
    vista.expect_text(content);
}

#[test]
fn clicking_x_closes_that_tab_and_the_label_switches_to_it() {
    let repo = Fixture::git()
        .file("alpha.txt", "first file\n")
        .file("beta.txt", "second file\n")
        .file("gamma.txt", "third file\n")
        .commit("init")
        .build();
    let mut vista = Vista::launch(BIN, &repo).start();
    vista.expect_text("alpha.txt");
    open(&mut vista, "alpha", "first file");
    open(&mut vista, "beta", "second file");
    open(&mut vista, "gamma", "third file");
    // Every tab has a ×.
    vista.expect_text("alpha.txt × ");
    vista.expect_text("beta.txt × ");
    vista.expect_text("gamma.txt × ");

    // Clicking a label switches to it.
    let (row, col) = vista.expect_text_in("beta.txt ×", 30..200).position();
    vista.click(col, row);
    vista.expect_text("second file");

    // Clicking alpha's × closes alpha (not the active tab).
    let (row, col) = vista.expect_text_in("alpha.txt ×", 30..200).position();
    vista.click(col + "alpha.txt ".len() as u16, row);
    vista.expect_no_text("alpha.txt ×");
    vista.expect_text("second file");
    vista.expect_text("beta.txt × ");
    vista.expect_text("gamma.txt × ");

    // The active tab's × closes it; the next one shows.
    let (row, col) = vista.expect_text_in("beta.txt ×", 30..200).position();
    vista.click(col + "beta.txt ".len() as u16, row);
    vista.expect_no_text("beta.txt ×");
    vista.expect_text("third file");

    // Closing the last tab leaves the empty editor.
    let (row, col) = vista.expect_text_in("gamma.txt ×", 30..200).position();
    vista.click(col + "gamma.txt ".len() as u16, row);
    vista.expect_text("No file open");
}
