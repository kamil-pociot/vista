//! M0 render snapshots: the empty layout in each built-in look.

use vista_tui::testing::{app_at, render, snapshot};

fn project() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("sample-project");
    std::fs::create_dir_all(&root).unwrap();
    (tmp, root)
}

#[test]
fn empty_layout_darcula() {
    let (tmp, root) = project();
    let mut app = app_at(&root, tmp.path(), Some("darcula"));
    let buf = render(&mut app, 100, 24);
    insta::assert_snapshot!(snapshot(&buf, &app.theme));
}

#[test]
fn help_overlay_darcula() {
    let (tmp, root) = project();
    let mut app = app_at(&root, tmp.path(), Some("darcula"));
    app.show_help = true;
    let buf = render(&mut app, 100, 34);
    insta::assert_snapshot!(vista_tui::testing::text(&buf));
}
