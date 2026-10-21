//! M8 render snapshots: every built-in look on the same screens, proving a
//! new design is just a theme file.

use std::time::Duration;

use vista_e2e::{Fixture, Repo};
use vista_tui::App;
use vista_tui::app::LeftPanel;
use vista_tui::event::AppEvent;
use vista_tui::testing::{app_at, render, snapshot};

fn wait_for(app: &mut App, pred: impl Fn(&AppEvent) -> bool) {
    loop {
        let ev = app.rx.recv_timeout(Duration::from_secs(10)).expect("event");
        let done = pred(&ev);
        app.handle_event(ev);
        if done {
            return;
        }
    }
}

fn project_with_code(repo: &Repo, look: &str) -> String {
    let mut app = app_at(repo.path(), repo.home(), Some(look));
    assert_eq!(app.theme.id, look);
    app.load_status_now();
    let path = app.root.join("crates/fb-git/src/status.rs");
    app.project.reveal(&path, &app.status.clone());
    app.open_file(&path);
    wait_for(&mut app, |e| matches!(e, AppEvent::Loaded { .. }));
    let buf = render(&mut app, 110, 30);
    snapshot(&buf, &app.theme).replace(&app.project_name.clone(), "<root>")
}

fn commit_with_diff(repo: &Repo, look: &str) -> String {
    let mut app = app_at(repo.path(), repo.home(), Some(look));
    app.load_status_now();
    app.left = Some(LeftPanel::Commit);
    let path = app.root.join("crates/fb-git/src/diff.rs");
    let change = app.status.change(&path).cloned().unwrap();
    app.open_diff(&change);
    wait_for(&mut app, |e| matches!(e, AppEvent::DiffLoaded { .. }));
    let buf = render(&mut app, 130, 30);
    snapshot(&buf, &app.theme).replace(&app.project_name.clone(), "<root>")
}

#[test]
fn looks_project_and_code() {
    let repo = Fixture::rust_project().build();
    for look in ["darcula", "quiet", "paper"] {
        insta::assert_snapshot!(format!("project_{look}"), project_with_code(&repo, look));
    }
}

#[test]
fn looks_commit_and_diff() {
    let repo = Fixture::rust_project().build();
    for look in ["darcula", "quiet", "paper"] {
        insta::assert_snapshot!(format!("commit_{look}"), commit_with_diff(&repo, look));
    }
}
