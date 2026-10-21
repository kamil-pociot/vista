//! M7 render snapshot: the mockup-A diff of `status.rs` as a golden screen.

use std::time::Duration;

use vista_e2e::Fixture;
use vista_tui::event::AppEvent;
use vista_tui::testing::{app_at, render, snapshot};

#[test]
fn side_by_side_diff_of_status_rs() {
    let repo = Fixture::rust_project().build();
    let mut app = app_at(repo.path(), repo.home(), Some("darcula"));
    app.load_status_now();
    let path = app.root.join("crates/fb-git/src/status.rs");
    let change = app.status.change(&path).cloned().expect("status.rs is modified");
    app.open_diff(&change);
    // Wait for the background diff load.
    loop {
        let ev = app.rx.recv_timeout(Duration::from_secs(10)).expect("diff loaded");
        let done = matches!(ev, AppEvent::DiffLoaded { .. });
        app.handle_event(ev);
        if done {
            break;
        }
    }
    app.left = None;
    let buf = render(&mut app, 150, 40);
    let snap = snapshot(&buf, &app.theme).replace(&app.project_name.clone(), "<root>");
    insta::assert_snapshot!(snap);
}
