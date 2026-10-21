//! Integration: our status must agree with git's own porcelain v1 output.

use std::collections::BTreeMap;

use proptest::prelude::*;
use vista_e2e::{Fixture, Repo};
use vista_git::{FileStatus, Git, StatusSnapshot};

/// Maps `git status --porcelain=v1` lines to the status we expect.
fn oracle(repo: &Repo) -> BTreeMap<String, FileStatus> {
    let mut m = BTreeMap::new();
    let out = repo.git(&[
        "-c",
        "core.quotepath=off",
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
    ]);
    let mut recs = out.split('\0').filter(|r| !r.is_empty());
    while let Some(r) = recs.next() {
        let (xy, path) = r.split_at(2);
        let path = path.trim_start().to_string();
        let (x, y) = (xy.as_bytes()[0], xy.as_bytes()[1]);
        let st = match (x, y) {
            (b'?', b'?') => FileStatus::Unversioned,
            (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => FileStatus::Conflicted,
            (b'R', _) | (_, b'R') => {
                recs.next(); // original path
                FileStatus::Renamed
            }
            (b'A', _) | (b'C', _) => FileStatus::Added,
            (b'D', _) | (_, b'D') => FileStatus::Deleted,
            _ => FileStatus::Modified,
        };
        m.insert(path, st);
    }
    m
}

fn ours(repo: &Repo) -> BTreeMap<String, FileStatus> {
    let git = Git::discover(repo.path()).unwrap();
    let snap = StatusSnapshot::load(&git).unwrap();
    snap.changes()
        .iter()
        .map(|c| (git.relative(&c.path).unwrap(), c.status))
        .collect()
}

#[test]
fn rust_project_matches_git() {
    let repo = Fixture::rust_project().build();
    assert_eq!(ours(&repo), oracle(&repo));
    let git = Git::discover(repo.path()).unwrap();
    let snap = StatusSnapshot::load(&git).unwrap();
    assert!(snap.is_ignored(&git.toplevel.join("target/debug/app")));
    assert!(snap.is_ignored(&git.toplevel.join("build.log")));
    assert_eq!(snap.branch.label(), "main");
    assert_eq!(
        snap.dir_status(&git.toplevel.join("crates/fb-git/src")),
        Some(FileStatus::Modified)
    );
}

#[test]
fn merge_conflicts_are_reported() {
    let repo = Fixture::git()
        .file("a.txt", "base\n")
        .commit("base")
        .run(&["checkout", "-q", "-b", "other"])
        .modify("a.txt", "other\n")
        .commit("other")
        .run(&["checkout", "-q", "main"])
        .modify("a.txt", "main\n")
        .commit("main")
        .build();
    let (ok, _) = repo.try_git(&["merge", "-q", "other"]);
    assert!(!ok, "merge must conflict");
    assert_eq!(ours(&repo), oracle(&repo));
    assert_eq!(ours(&repo)["a.txt"], FileStatus::Conflicted);
}

#[test]
fn not_a_repository() {
    let repo = Fixture::plain().file("x", "y").build();
    assert!(Git::discover(repo.path()).is_err());
}

#[derive(Debug, Clone)]
enum Op {
    Modify(usize),
    Delete(usize),
    Create(usize),
    Stage(usize),
    Rename(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..6usize).prop_map(Op::Modify),
        (0..6usize).prop_map(Op::Delete),
        (0..6usize).prop_map(Op::Create),
        (0..6usize).prop_map(Op::Stage),
        (0..6usize).prop_map(Op::Rename),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    /// Random edits on a small repo: our status always equals git's.
    #[test]
    fn random_edits_match_git(ops in proptest::collection::vec(op(), 1..8)) {
        let mut f = Fixture::git();
        for i in 0..6 {
            f = f.file(&format!("dir{}/f{i}.txt", i % 2), format!("line {i}\n"));
        }
        let repo = f.commit("init").build();
        for o in ops {
            let p = |i: usize| format!("dir{}/f{i}.txt", i % 2);
            let exists = |rel: &str| repo.path().join(rel).exists();
            match o {
                Op::Modify(i) if exists(&p(i)) => repo.write(&p(i), format!("changed {i}\n")),
                Op::Delete(i) if exists(&p(i)) => repo.remove(&p(i)),
                Op::Create(i) => repo.write(&format!("new/n{i}.txt"), "new\n"),
                Op::Stage(i) => { let _ = repo.try_git(&["add", "-A", "--", &format!("dir{}", i % 2)]); }
                Op::Rename(i) if exists(&p(i)) => { let _ = repo.try_git(&["mv", &p(i), &format!("dir{}/moved{i}.txt", i % 2)]); }
                _ => {}
            }
        }
        prop_assert_eq!(ours(&repo), oracle(&repo));
    }
}
