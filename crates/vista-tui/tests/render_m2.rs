//! M2 render snapshots: the viewer.

use vista_e2e::Fixture;
use vista_tui::testing::{app_at, render, snapshot, text};

#[test]
fn wrapped_long_line() {
    let long = format!("start {} end\n", "x".repeat(300));
    let repo = Fixture::plain().file("wide.txt", long).build();
    let mut app = app_at(repo.path(), repo.home(), None);
    let path = app.root.join("wide.txt");
    app.open_file(&path);
    let ev = app.rx.recv_timeout(std::time::Duration::from_secs(10)).expect("loaded");
    app.handle_event(ev);
    app.viewer.active_tab_mut().unwrap().wrap = true;
    let buf = render(&mut app, 100, 20);
    let t = text(&buf);
    assert!(t.contains("end"), "{t}");
}

#[test]
fn code_view_with_gutter_markers() {
    let repo = Fixture::rust_project().build();
    let mut app = app_at(repo.path(), repo.home(), None);
    app.load_status_now();
    let path = app.root.join("crates/fb-git/src/status.rs");
    app.open_file(&path);
    // Wait for highlighting + markers.
    let ev = app.rx.recv_timeout(std::time::Duration::from_secs(10)).expect("loaded");
    app.handle_event(ev);
    let buf = render(&mut app, 100, 30);
    let snap = snapshot(&buf, &app.theme).replace(&app.project_name.clone(), "<root>");
    insta::assert_snapshot!(snap);
}
