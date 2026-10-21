//! M1 render snapshots: the Project tree with git colours.

use vista_e2e::Fixture;
use vista_tui::testing::{app_at, render, snapshot};

#[test]
fn project_tree_with_status_colours() {
    let repo = Fixture::rust_project().build();
    let mut app = app_at(repo.path(), repo.home(), Some("darcula"));
    app.load_status_now();
    let crates = app.root.join("crates");
    app.project
        .reveal(&crates.join("fb-git/src/status.rs"), &app.status.clone());
    let buf = render(&mut app, 90, 26);
    let root_name = app.project_name.clone();
    let snap = snapshot(&buf, &app.theme).replace(&root_name, "<root>");
    insta::assert_snapshot!(snap);
}
