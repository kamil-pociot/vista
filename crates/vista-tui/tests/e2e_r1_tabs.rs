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

/// Tab labels on screen row `row`, left to right.
fn order(vista: &Vista, row: u16, names: &[&str]) -> Vec<String> {
    let line = vista.row_text(row);
    let mut found: Vec<(usize, String)> = names
        .iter()
        .filter_map(|n| line.find(n).map(|i| (i, n.to_string())))
        .collect();
    found.sort();
    found.into_iter().map(|(_, n)| n).collect()
}

#[test]
fn dragging_a_tab_moves_it() {
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
    let names = ["alpha.txt ×", "beta.txt ×", "gamma.txt ×"];
    let (row, from) = vista.expect_text_in("alpha.txt ×", 30..200).position();
    vista.expect_text_in("gamma.txt ×", 30..200);
    assert_eq!(order(&vista, row, &names), names);
    // Drag alpha onto gamma: alpha goes last and stays the one shown.
    let (_, to) = vista.expect_text_in("gamma.txt ×", 30..200).position();
    vista.drag((from + 1, row), (to + 3, row));
    vista.wait_until("alpha moved to the end", move |s| {
        let line = s.contents().lines().nth(row as usize).unwrap_or_default().to_string();
        let (a, g) = (line.find("alpha.txt ×"), line.find("gamma.txt ×"));
        a.zip(g).is_some_and(|(a, g)| a > g)
    });
    assert_eq!(order(&vista, row, &names), ["beta.txt ×", "gamma.txt ×", "alpha.txt ×"]);
    vista.expect_text("first file");
    // A plain click still just switches.
    let (r, c) = vista.expect_text_in("beta.txt ×", 30..200).position();
    vista.click(c + 1, r);
    vista.expect_text("second file");
    assert_eq!(order(&vista, row, &names), ["beta.txt ×", "gamma.txt ×", "alpha.txt ×"]);
}

#[test]
fn dragging_a_project_tab_reorders_projects_and_is_remembered() {
    let a = Fixture::git().file("a.txt", "a\n").commit("init").build();
    let b = Fixture::git().file("b.txt", "b\n").commit("init").build();
    let name = |r: &vista_e2e::Repo| r.path().file_name().unwrap().to_string_lossy().into_owned();
    let (an, bn) = (name(&a), name(&b));
    let mut vista = Vista::launch(BIN, &a).size(160, 30).start();
    vista.expect_text_in("a.txt", 0..40);
    vista.press("ctrl+o");
    vista.type_text(&b.path().canonicalize().unwrap().display().to_string());
    vista.expect_text(&format!("+ {bn}"));
    vista.press("enter");
    vista.expect_text_in("b.txt", 0..40);
    let tabs = [format!(" {an} ×"), format!(" {bn} ×")];
    let tabs: Vec<&str> = tabs.iter().map(String::as_str).collect();
    assert_eq!(order(&vista, 0, &tabs), tabs);
    // Drag the first project's tab past the second.
    let (_, from) = vista.expect_text_in(&format!(" {an} ×"), 0..160).position();
    let (_, to) = vista.expect_text_in(&format!(" {bn} ×"), 0..160).position();
    vista.drag((from + 2, 0), (to + 4, 0));
    let (an2, bn2) = (an.clone(), bn.clone());
    vista.wait_until("projects swapped", move |s| {
        let line = s.contents().lines().next().unwrap_or_default().to_string();
        line.find(&bn2).zip(line.find(&an2)).is_some_and(|(b, a)| b < a)
    });
    // The dragged project is shown, and the order is saved.
    vista.expect_text_in("a.txt", 0..40);
    vista.press("ctrl+q");
    vista.expect_exit();
    let state = std::fs::read_to_string(a.home().join(".config/vista/state.toml")).unwrap();
    let (pa, pb) = (state.find(&an).unwrap(), state.find(&bn).unwrap());
    assert!(pb < pa, "saved order b, a:\n{state}");
}
