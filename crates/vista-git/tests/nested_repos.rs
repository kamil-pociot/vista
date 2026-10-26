//! Integration: repositories cloned inside the project (an ignored `repos/`,
//! an untracked `vendor/lib`) keep their own status, and commits go to the
//! repository a file is in.

use vista_e2e::{Fixture, Repo};
use vista_git::write::CommitFile;
use vista_git::{FileStatus, Git, StatusSnapshot};

fn project() -> Repo {
    Fixture::git()
        .file(".gitignore", "repos/\n")
        .file("main.txt", "main\n")
        .commit("init")
        .modify("main.txt", "main changed\n")
        .nested_repo("repos/foo")
        .file("repos/foo/x.txt", "x\n")
        .file("repos/foo/.gitignore", "build/\n")
        .commit_in("repos/foo", "foo init")
        .run(&["-C", "repos/foo", "switch", "-q", "-c", "dev"])
        .modify("repos/foo/x.txt", "x changed\n")
        .file("repos/foo/new.txt", "new\n")
        .file("repos/foo/build/out.o", "binary\n")
        .file("repos/notes.txt", "beside the clones\n")
        .nested_repo("vendor/lib")
        .file("vendor/lib/z.txt", "z\n")
        .build()
}

#[test]
fn nested_repositories_have_their_own_status() {
    let repo = project();
    let git = Git::discover(repo.path()).unwrap();
    let s = StatusSnapshot::load(&git).unwrap();
    let top = &git.toplevel;
    let st = |p: &str| s.file_status(&top.join(p));
    assert_eq!(st("main.txt"), Some(FileStatus::Modified));
    assert_eq!(st("repos/foo/x.txt"), Some(FileStatus::Modified));
    assert_eq!(st("repos/foo/new.txt"), Some(FileStatus::Unversioned));
    assert_eq!(st("repos/foo/build/out.o"), Some(FileStatus::Ignored));
    assert_eq!(st("repos/notes.txt"), Some(FileStatus::Ignored));
    assert_eq!(st("vendor/lib/z.txt"), Some(FileStatus::Unversioned));
    assert!(s.change(&top.join("vendor/lib")).is_none(), "{:?}", s.changes());
    assert_eq!(s.changes().len(), 4, "{:?}", s.changes());

    let tops: Vec<_> = s.nested().iter().map(|n| n.toplevel.clone()).collect();
    assert_eq!(tops, vec![top.join("repos/foo"), top.join("vendor/lib")]);
    assert_eq!(s.repo(&top.join("repos/foo/x.txt")).branch.label(), "dev");
    assert_eq!(s.branch.label(), "main");
    assert!(!s.is_ignored(&top.join("repos")));
    assert_eq!(s.dir_status(&top.join("repos")), Some(FileStatus::Modified));
}

#[test]
fn worktrees_of_the_same_repository_are_not_nested_repositories() {
    let repo = Fixture::git()
        .file(".gitignore", "trees/\n")
        .file("a.txt", "a\n")
        .commit("init")
        .run(&["worktree", "add", "-q", "-b", "other", "trees/other"])
        .modify("trees/other/a.txt", "changed in the worktree\n")
        .nested_repo("trees/clone")
        .file("trees/clone/c.txt", "c\n")
        // Caches (Cargo's target/) aren't searched.
        .file(
            "trees/cache/CACHEDIR.TAG",
            "Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .nested_repo("trees/cache/in-a-cache")
        .build();
    let git = Git::discover(repo.path()).unwrap();
    let s = StatusSnapshot::load(&git).unwrap();
    let tops: Vec<_> = s.nested().iter().map(|n| n.toplevel.clone()).collect();
    assert_eq!(tops, vec![git.toplevel.join("trees/clone")]);
    assert_eq!(
        s.file_status(&git.toplevel.join("trees/other/a.txt")),
        Some(FileStatus::Ignored)
    );
    // Seen from the worktree, the main checkout isn't inside it either way.
    let wt = Git::discover(&repo.path().join("trees/other")).unwrap();
    assert!(StatusSnapshot::load(&wt).unwrap().nested().is_empty());
}

#[test]
fn for_path_finds_the_repository_a_file_is_in() {
    let repo = project();
    let git = Git::discover(repo.path()).unwrap();
    let top = git.toplevel.clone();
    assert_eq!(git.for_path(&top.join("main.txt")), git);
    assert_eq!(git.for_path(&top.join("repos/notes.txt")), git);
    let foo = git.for_path(&top.join("repos/foo/x.txt"));
    assert_eq!(foo.toplevel, top.join("repos/foo"));
    assert_eq!(git.for_path(&top.join("repos/foo")).toplevel, top.join("repos/foo"));
    // Gutter markers and diffs compare with that repository's HEAD.
    assert_eq!(foo.head_content(&top.join("repos/foo/x.txt")).unwrap(), b"x\n");
    assert_eq!(git.head_content(&top.join("repos/foo/x.txt")), None);
}

#[test]
fn a_commit_in_a_nested_repository_stays_there() {
    let repo = project();
    let git = Git::discover(repo.path()).unwrap();
    let s = StatusSnapshot::load(&git).unwrap();
    let x = s.change(&git.toplevel.join("repos/foo/x.txt")).unwrap().clone();
    let foo = Git {
        toplevel: s.repo(&x.path).toplevel.clone(),
    };
    foo.commit(&[CommitFile::whole(&x)], "Change x", false).unwrap();
    assert_eq!(
        repo.git(&["-C", "repos/foo", "log", "-1", "--format=%s"]).trim(),
        "Change x"
    );
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]).trim(),
        "init",
        "the outer repository has no new commit"
    );
    let s = StatusSnapshot::load(&git).unwrap();
    assert!(s.change(&x.path).is_none());
    assert!(s.change(&git.toplevel.join("main.txt")).is_some());
}
