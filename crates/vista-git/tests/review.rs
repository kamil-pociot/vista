//! Integration: what changed since a base revision, against real repos.

use vista_e2e::Fixture;
use vista_git::Git;
use vista_git::review::{Review, is_head, resolve};

#[test]
fn a_branchs_work_since_main() {
    let repo = Fixture::git()
        .file("a.txt", "one\n")
        .file("b.txt", "bee\n")
        .commit("init")
        .run(&["checkout", "-q", "-b", "work"])
        .file("a.txt", "one\ntwo\n")
        .run(&["mv", "b.txt", "c.txt"])
        .commit("work")
        .file("a.txt", "one\ntwo\nthree\n")
        .file("d.txt", "new\n")
        .build();
    let git = Git::discover(repo.path()).unwrap();
    let base = resolve(&git, "main").unwrap();
    assert_eq!(base.len(), 40);
    assert!(!is_head(&git, &base));
    let r = Review::load(&git, &base).unwrap();
    assert_eq!(r.commits, 1);
    assert_eq!(r.short, &base[..7]);
    let rel = |p: &std::path::Path| p.strip_prefix(&git.toplevel).unwrap().display().to_string();
    let got: Vec<(char, String)> = r.files.iter().map(|f| (f.status, rel(&f.path))).collect();
    assert_eq!(
        got,
        [('M', "a.txt".into()), ('R', "c.txt".into()), ('A', "d.txt".into())]
    );
    assert_eq!(r.files[1].old_path.as_deref().map(rel).as_deref(), Some("b.txt"));
    // HEAD is HEAD; a name that isn't a commit says so.
    let head = resolve(&git, "HEAD").unwrap();
    assert!(is_head(&git, &head));
    let err = resolve(&git, "nope").unwrap_err().to_string();
    assert!(err.contains("nope"), "{err}");
}
