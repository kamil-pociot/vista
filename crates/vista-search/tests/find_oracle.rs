//! Integration: Find in Files must agree with an independent implementation.
//! Standalone ripgrep isn't installed on every machine, so the oracle is
//! `git grep` (tracked + untracked, respects .gitignore, skips binaries).

use std::collections::BTreeSet;

use vista_e2e::{Fixture, Repo};
use vista_search::find::search_all;
use vista_search::{FindOptions, Scope};

fn repo() -> Repo {
    Fixture::rust_project()
        .file(
            "docs/notes.md",
            "StatusMap caches status.\nThe status map is refreshed.\nfn main is not code here\n",
        )
        .file(
            "src/app.rs",
            "fn main() {\n    let statusMap = StatusMap::new();\n    run(statusMap);\n}\n",
        )
        .file(
            "src/util.rs",
            "pub fn helper() -> u32 { 42 }\n// TODO: status cleanup\n",
        )
        .build()
}

fn oracle(repo: &Repo, args: &[&str], pattern: &str) -> BTreeSet<String> {
    oracle_in(repo, args, pattern, &[])
}

fn oracle_in(repo: &Repo, args: &[&str], pattern: &str, pathspec: &[&str]) -> BTreeSet<String> {
    let mut a = vec!["grep", "-n", "-I", "--untracked"];
    a.extend_from_slice(args);
    a.extend_from_slice(&["-e", pattern]);
    if !pathspec.is_empty() {
        a.push("--");
        a.extend_from_slice(pathspec);
    }
    let (_, out) = repo.try_git(&a);
    out.lines()
        .filter_map(|l| {
            let mut parts = l.splitn(3, ':');
            Some(format!("{}:{}", parts.next()?, parts.next()?))
        })
        .collect()
}

fn ours(repo: &Repo, opts: &FindOptions) -> BTreeSet<String> {
    let root = repo.path().canonicalize().unwrap();
    let (hits, _) = search_all(&root, opts).unwrap();
    hits.iter()
        .map(|h| format!("{}:{}", h.path.strip_prefix(&root).unwrap().display(), h.line))
        .collect()
}

fn opts(q: &str) -> FindOptions {
    FindOptions {
        query: q.into(),
        ..FindOptions::default()
    }
}

#[test]
fn literal_queries_match_the_oracle() {
    let r = repo();
    for q in ["StatusMap", "status", "fn ", "{", "TODO:"] {
        assert_eq!(ours(&r, &opts(q)), oracle(&r, &["-i", "-F"], q), "query {q:?}");
    }
}

#[test]
fn case_sensitive_queries_match_the_oracle() {
    let r = repo();
    for q in ["StatusMap", "status", "Status"] {
        let o = FindOptions {
            case_sensitive: true,
            ..opts(q)
        };
        assert_eq!(ours(&r, &o), oracle(&r, &["-F"], q), "query {q:?}");
    }
}

#[test]
fn whole_word_queries_match_the_oracle() {
    let r = repo();
    for q in ["status", "map", "fn"] {
        let o = FindOptions {
            whole_word: true,
            ..opts(q)
        };
        assert_eq!(ours(&r, &o), oracle(&r, &["-i", "-w", "-F"], q), "query {q:?}");
    }
}

#[test]
fn regex_queries_match_the_oracle() {
    let r = repo();
    for q in ["Status[A-Z][a-z]+", "fn [a-z]+", "[0-9]+", "^pub"] {
        let o = FindOptions {
            regex: true,
            case_sensitive: true,
            ..opts(q)
        };
        assert_eq!(ours(&r, &o), oracle(&r, &["-E"], q), "query {q:?}");
    }
}

#[test]
fn masks_and_directory_scope_match_the_oracle() {
    let r = repo();
    let o = FindOptions {
        mask: "*.rs".into(),
        ..opts("status")
    };
    assert_eq!(ours(&r, &o), oracle_in(&r, &["-i", "-F"], "status", &["*.rs"]));
    let root = r.path().canonicalize().unwrap();
    let o = FindOptions {
        scope: Scope::Directory(root.join("crates")),
        ..opts("status")
    };
    let expected: BTreeSet<String> = oracle(&r, &["-i", "-F"], "status")
        .into_iter()
        .filter(|p| p.starts_with("crates/"))
        .collect();
    assert_eq!(ours(&r, &o), expected);
}

/// Opt-in: `VISTA_PERF_REPO=... cargo test --release --test find_oracle bench -- --ignored --nocapture`
#[test]
#[ignore = "needs VISTA_PERF_REPO"]
fn bench_engine_on_a_real_repo() {
    let Some(dir) = std::env::var_os("VISTA_PERF_REPO") else {
        return;
    };
    let root = std::path::PathBuf::from(dir).canonicalize().unwrap();
    for q in ["deploy", "TODO", "agent"] {
        let mut times = Vec::new();
        let mut n = 0;
        for _ in 0..5 {
            let t = std::time::Instant::now();
            let (hits, _) = search_all(&root, &opts(q)).unwrap();
            times.push(t.elapsed());
            n = hits.len();
        }
        times.sort();
        println!(
            "{q}: {n} hits, best {:?}, median {:?}, worst {:?}",
            times[0], times[2], times[4]
        );
    }
}
