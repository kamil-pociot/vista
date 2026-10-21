//! "Find in Files": parallel, streaming text search with the ripgrep engine.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::sinks::Lossy;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};

/// Where to search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// The whole project.
    Project,
    /// One directory (recursively).
    Directory(PathBuf),
    /// An explicit list of files (changed files, open files).
    Files(Vec<PathBuf>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindOptions {
    pub query: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    /// Comma-separated globs; `!` excludes: `*.md, !archive/**`.
    pub mask: String,
    pub scope: Scope,
    pub include_ignored: bool,
    /// Stop after this many matching lines.
    pub max_results: usize,
}

impl Default for FindOptions {
    fn default() -> Self {
        FindOptions {
            query: String::new(),
            case_sensitive: false,
            whole_word: false,
            regex: false,
            mask: String::new(),
            scope: Scope::Project,
            include_ignored: false,
            max_results: 1000,
        }
    }
}

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: PathBuf,
    /// 1-based.
    pub line: usize,
    /// The line, without its line terminator.
    pub text: String,
    /// Matches as char ranges into `text`.
    pub ranges: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub files_searched: usize,
    pub matched_lines: usize,
    pub truncated: bool,
    pub cancelled: bool,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindEvent {
    /// Hits from one file.
    Hits(Vec<Hit>),
    Done(Summary),
    /// The query or mask is invalid.
    Error(String),
}

/// Builds the line matcher for the options.
pub fn matcher(opts: &FindOptions) -> Result<RegexMatcher, String> {
    RegexMatcherBuilder::new()
        .case_insensitive(!opts.case_sensitive)
        .word(opts.whole_word)
        .fixed_strings(!opts.regex)
        .build(&opts.query)
        .map_err(|e| e.to_string())
}

fn byte_to_char_ranges(text: &str, ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let to_char = |b: usize| text[..b.min(text.len())].chars().count();
    ranges.iter().map(|(a, b)| (to_char(*a), to_char(*b))).collect()
}

fn search_file(searcher: &mut Searcher, m: &RegexMatcher, path: &Path, budget: &AtomicUsize, max: usize) -> Vec<Hit> {
    let mut hits = Vec::new();
    let _ = searcher.search_path(
        m,
        path,
        Lossy(|lnum, line| {
            let text = line.trim_end_matches(['\n', '\r']).to_string();
            let mut ranges = Vec::new();
            let _ = m.find_iter(text.as_bytes(), |mm| {
                if mm.start() < mm.end() {
                    ranges.push((mm.start(), mm.end()));
                }
                true
            });
            let ranges = byte_to_char_ranges(&text, &ranges);
            hits.push(Hit {
                path: path.to_path_buf(),
                line: lnum as usize,
                text,
                ranges,
            });
            Ok(budget.fetch_add(1, Ordering::SeqCst) + 1 < max)
        }),
    );
    hits
}

/// Runs a search, streaming results through `sink`. Returns early when
/// `cancel` is set. Blocks; run it on a worker thread.
pub fn search(root: &Path, opts: &FindOptions, cancel: &AtomicBool, sink: &(dyn Fn(FindEvent) + Sync)) {
    let start = Instant::now();
    if opts.query.is_empty() {
        sink(FindEvent::Done(Summary {
            files_searched: 0,
            matched_lines: 0,
            truncated: false,
            cancelled: false,
            elapsed: start.elapsed(),
        }));
        return;
    }
    let m = match matcher(opts) {
        Ok(m) => m,
        Err(e) => {
            sink(FindEvent::Error(e));
            return;
        }
    };
    let mut ob = OverrideBuilder::new(root);
    for glob in opts.mask.split(',').map(str::trim).filter(|g| !g.is_empty()) {
        if let Err(e) = ob.add(glob) {
            sink(FindEvent::Error(format!("file mask `{glob}`: {e}")));
            return;
        }
    }
    let overrides = match ob.build() {
        Ok(o) => o,
        Err(e) => {
            sink(FindEvent::Error(format!("file mask: {e}")));
            return;
        }
    };
    let budget = AtomicUsize::new(0);
    let files = AtomicUsize::new(0);
    let max = opts.max_results.max(1);
    let new_searcher = || {
        SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(true)
            .build()
    };
    match &opts.scope {
        Scope::Files(list) => {
            let mut s = new_searcher();
            for p in list {
                if cancel.load(Ordering::SeqCst) || budget.load(Ordering::SeqCst) >= max {
                    break;
                }
                let rel = p.strip_prefix(root).unwrap_or(p);
                if overrides.matched(rel, false).is_ignore() || !p.is_file() {
                    continue;
                }
                files.fetch_add(1, Ordering::SeqCst);
                let hits = search_file(&mut s, &m, p, &budget, max);
                if !hits.is_empty() {
                    sink(FindEvent::Hits(hits));
                }
            }
        }
        Scope::Project | Scope::Directory(_) => {
            let base = match &opts.scope {
                Scope::Directory(d) => d.clone(),
                _ => root.to_path_buf(),
            };
            let include = opts.include_ignored;
            let walker = WalkBuilder::new(&base)
                .hidden(false)
                .git_ignore(!include)
                .git_global(!include)
                .git_exclude(!include)
                .ignore(!include)
                .parents(true)
                .overrides(overrides)
                .filter_entry(|e| e.file_name() != ".git")
                .build_parallel();
            let sink = Mutex::new(sink);
            walker.run(|| {
                let mut s = new_searcher();
                let (m, budget, files, sink) = (&m, &budget, &files, &sink);
                Box::new(move |entry| {
                    if cancel.load(Ordering::SeqCst) || budget.load(Ordering::SeqCst) >= max {
                        return WalkState::Quit;
                    }
                    let Ok(entry) = entry else { return WalkState::Continue };
                    if !entry.file_type().is_some_and(|t| t.is_file()) {
                        return WalkState::Continue;
                    }
                    files.fetch_add(1, Ordering::SeqCst);
                    let hits = search_file(&mut s, m, entry.path(), budget, max);
                    if !hits.is_empty() {
                        (sink.lock().expect("sink"))(FindEvent::Hits(hits));
                    }
                    WalkState::Continue
                })
            });
        }
    }
    let matched = budget.load(Ordering::SeqCst);
    sink(FindEvent::Done(Summary {
        files_searched: files.load(Ordering::SeqCst),
        matched_lines: matched.min(max),
        truncated: matched >= max,
        cancelled: cancel.load(Ordering::SeqCst),
        elapsed: start.elapsed(),
    }));
}

/// Runs a search to completion and returns all hits sorted by path and line.
pub fn search_all(root: &Path, opts: &FindOptions) -> Result<(Vec<Hit>, Summary), String> {
    let hits = Arc::new(Mutex::new(Vec::new()));
    let result = Arc::new(Mutex::new(None));
    let (h, r) = (hits.clone(), result.clone());
    let cancel = AtomicBool::new(false);
    search(root, opts, &cancel, &move |ev| match ev {
        FindEvent::Hits(v) => h.lock().expect("hits").extend(v),
        FindEvent::Done(s) => *r.lock().expect("result") = Some(Ok(s)),
        FindEvent::Error(e) => *r.lock().expect("result") = Some(Err(e)),
    });
    let summary = result
        .lock()
        .expect("result")
        .take()
        .unwrap_or(Err("no result".into()))?;
    let mut hits = std::mem::take(&mut *hits.lock().expect("hits"));
    hits.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    Ok((hits, summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for (p, c) in [
            (
                "src/status.rs",
                "pub struct StatusMap {}\nimpl StatusMap {}\nlet statusmap = 1;\n",
            ),
            ("src/lib.rs", "pub use status::StatusMap;\n"),
            ("docs/notes.md", "StatusMap caches status.\nunrelated\n"),
            ("archive/old.md", "StatusMap legacy\n"),
            ("target/out.txt", "StatusMap ignored\n"),
            (".gitignore", "target/\n"),
            ("bin.dat", "StatusMap\0binary"),
        ] {
            std::fs::create_dir_all(r.join(p).parent().unwrap()).unwrap();
            std::fs::write(r.join(p), c).unwrap();
        }
        std::fs::create_dir_all(r.join(".git")).unwrap();
        d
    }

    fn q(query: &str) -> FindOptions {
        FindOptions {
            query: query.into(),
            ..FindOptions::default()
        }
    }

    fn paths(hits: &[Hit], root: &Path) -> Vec<String> {
        hits.iter()
            .map(|h| format!("{}:{}", h.path.strip_prefix(root).unwrap().display(), h.line))
            .collect()
    }

    #[test]
    fn literal_case_insensitive_by_default() {
        let d = tree();
        let (hits, sum) = search_all(d.path(), &q("statusmap")).unwrap();
        let p = paths(&hits, d.path());
        assert!(p.contains(&"src/status.rs:3".to_string()));
        assert!(p.contains(&"src/status.rs:1".to_string()));
        assert!(!p.iter().any(|x| x.starts_with("target/")), "gitignored");
        assert!(!p.iter().any(|x| x.starts_with("bin.dat")), "binary");
        assert_eq!(sum.matched_lines, hits.len());
        let h = hits
            .iter()
            .find(|h| h.line == 1 && h.path.ends_with("status.rs"))
            .unwrap();
        assert_eq!(h.ranges, vec![(11, 20)]);
    }

    #[test]
    fn case_word_and_regex_toggles() {
        let d = tree();
        let opts = FindOptions {
            case_sensitive: true,
            ..q("statusmap")
        };
        assert_eq!(
            paths(&search_all(d.path(), &opts).unwrap().0, d.path()),
            vec!["src/status.rs:3"]
        );
        let opts = FindOptions {
            whole_word: true,
            ..q("status")
        };
        let p = paths(&search_all(d.path(), &opts).unwrap().0, d.path());
        assert!(p.contains(&"docs/notes.md:1".to_string()), "'status.' is a word");
        assert!(p.contains(&"src/lib.rs:1".to_string()), "'status::' is a word");
        assert!(!p.contains(&"src/status.rs:1".to_string()), "'StatusMap' is not");
        let opts = FindOptions {
            regex: true,
            ..q(r"impl \w+Map")
        };
        assert_eq!(
            paths(&search_all(d.path(), &opts).unwrap().0, d.path()),
            vec!["src/status.rs:2"]
        );
        assert!(search_all(d.path(), &FindOptions { regex: true, ..q("(") }).is_err());
    }

    #[test]
    fn masks_and_scopes() {
        let d = tree();
        let r = d.path();
        let md = FindOptions {
            mask: "*.md, !archive/**".into(),
            ..q("StatusMap")
        };
        assert_eq!(paths(&search_all(r, &md).unwrap().0, r), vec!["docs/notes.md:1"]);
        let dir = FindOptions {
            scope: Scope::Directory(r.join("src")),
            ..q("StatusMap")
        };
        assert!(
            paths(&search_all(r, &dir).unwrap().0, r)
                .iter()
                .all(|p| p.starts_with("src/"))
        );
        let files = FindOptions {
            scope: Scope::Files(vec![r.join("src/lib.rs"), r.join("missing.rs")]),
            ..q("StatusMap")
        };
        assert_eq!(paths(&search_all(r, &files).unwrap().0, r), vec!["src/lib.rs:1"]);
        let ignored = FindOptions {
            include_ignored: true,
            ..q("ignored")
        };
        assert_eq!(paths(&search_all(r, &ignored).unwrap().0, r), vec!["target/out.txt:1"]);
    }

    #[test]
    fn caps_results_and_cancels() {
        let d = tempfile::tempdir().unwrap();
        let text: String = (0..500).map(|i| format!("needle {i}\n")).collect();
        std::fs::write(d.path().join("a.txt"), &text).unwrap();
        let (hits, sum) = search_all(
            d.path(),
            &FindOptions {
                max_results: 50,
                ..q("needle")
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 50);
        assert!(sum.truncated);
        let cancel = AtomicBool::new(true);
        let got = Mutex::new(Vec::new());
        search(d.path(), &q("needle"), &cancel, &|e| got.lock().unwrap().push(e));
        let got = got.into_inner().unwrap();
        assert!(matches!(got.last(), Some(FindEvent::Done(s)) if s.cancelled));
        assert!(!got.iter().any(|e| matches!(e, FindEvent::Hits(_))));
    }

    #[test]
    fn empty_query_finishes_immediately() {
        let d = tree();
        let (hits, sum) = search_all(d.path(), &q("")).unwrap();
        assert!(hits.is_empty());
        assert_eq!(sum.files_searched, 0);
    }
}
