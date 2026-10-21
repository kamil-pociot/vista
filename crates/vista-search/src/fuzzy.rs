//! "Go to File": a fuzzy index of project paths with IntelliJ-like ranking.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32String};

/// Every file in the project, relative and `/`-separated.
#[derive(Debug, Clone, Default)]
pub struct FileIndex {
    pub root: PathBuf,
    paths: Vec<String>,
    haystacks: Vec<Utf32String>,
    /// Char offset where the file name starts in each path.
    name_start: Vec<usize>,
}

/// One ranked result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatch {
    pub path: String,
    pub score: u32,
    /// Matched char indices into `path`, sorted.
    pub indices: Vec<u32>,
}

/// A parsed query: `foo/bar.rs:42` → text `foo/bar.rs`, line 42.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub line: Option<usize>,
}

impl Query {
    pub fn parse(q: &str) -> Query {
        let q = q.trim();
        if let Some((text, line)) = q.rsplit_once(':')
            && let Ok(n) = line.parse::<usize>()
        {
            return Query {
                text: text.to_string(),
                line: Some(n),
            };
        }
        Query {
            text: q.to_string(),
            line: None,
        }
    }
}

const NAME_BONUS: u32 = 10_000;
const EXACT_NAME_BONUS: u32 = 20_000;
const RECENT_BONUS: u32 = 4_000;

impl FileIndex {
    /// Walks `root` (respecting `.gitignore` unless `include_ignored`).
    /// Hidden files are included; `.git` never is.
    pub fn build(root: &Path, include_ignored: bool) -> FileIndex {
        let mut paths: Vec<String> = WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(!include_ignored)
            .git_global(!include_ignored)
            .git_exclude(!include_ignored)
            .ignore(!include_ignored)
            .parents(true)
            .filter_entry(|e| e.file_name() != ".git")
            .build()
            .flatten()
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .filter_map(|e| {
                e.path()
                    .strip_prefix(root)
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
            })
            .collect();
        paths.sort();
        FileIndex::from_paths(root, paths)
    }

    pub fn from_paths(root: &Path, paths: Vec<String>) -> FileIndex {
        let haystacks = paths.iter().map(|p| Utf32String::from(p.as_str())).collect();
        let name_start = paths
            .iter()
            .map(|p| p.rfind('/').map(|i| p[..=i].chars().count()).unwrap_or(0))
            .collect();
        FileIndex {
            root: root.to_path_buf(),
            paths,
            haystacks,
            name_start,
        }
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Ranks files for `text` (no `:line` suffix). `recent` (most recent
    /// first, relative paths) get a bonus. Empty queries list recent files
    /// first, then everything else in path order.
    pub fn search(&self, text: &str, recent: &[String], limit: usize) -> Vec<FileMatch> {
        let recent_bonus = |p: &str| {
            recent
                .iter()
                .take(20)
                .position(|r| r == p)
                .map(|i| RECENT_BONUS - i as u32 * 100)
                .unwrap_or(0)
        };
        if text.trim().is_empty() {
            let mut out: Vec<FileMatch> = self
                .paths
                .iter()
                .map(|p| FileMatch {
                    path: p.clone(),
                    score: recent_bonus(p),
                    indices: Vec::new(),
                })
                .collect();
            out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
            out.truncate(limit);
            return out;
        }
        let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
        let pattern = Pattern::parse(text, CaseMatching::Smart, Normalization::Smart);
        let lowered = text.to_lowercase();
        let mut indices = Vec::new();
        let mut out = Vec::new();
        for (i, hay) in self.haystacks.iter().enumerate() {
            let path = &self.paths[i];
            let name_at = self.name_start[i];
            // Prefer matches inside the file name.
            indices.clear();
            let in_name = pattern.indices(hay.slice(name_at..), &mut matcher, &mut indices);
            let (score, idx) = match in_name {
                Some(s) => {
                    let mut v: Vec<u32> = indices.iter().map(|x| x + name_at as u32).collect();
                    v.sort_unstable();
                    v.dedup();
                    let name = &path[path.len() - (path.chars().count() - name_at)..];
                    let exact = if name.to_lowercase() == lowered {
                        EXACT_NAME_BONUS
                    } else {
                        0
                    };
                    (s + NAME_BONUS + exact, v)
                }
                None => {
                    indices.clear();
                    match pattern.indices(hay.slice(..), &mut matcher, &mut indices) {
                        Some(s) => {
                            let mut v = indices.clone();
                            v.sort_unstable();
                            v.dedup();
                            (s, v)
                        }
                        None => continue,
                    }
                }
            };
            out.push(FileMatch {
                path: path.clone(),
                score: score + recent_bonus(path),
                indices: idx,
            });
        }
        out.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.path.len().cmp(&b.path.len()))
                .then_with(|| a.path.cmp(&b.path))
        });
        out.truncate(limit);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(paths: &[&str]) -> FileIndex {
        FileIndex::from_paths(Path::new("/p"), paths.iter().map(|s| s.to_string()).collect())
    }

    fn top(idx: &FileIndex, q: &str) -> Vec<String> {
        idx.search(q, &[], 10).into_iter().map(|m| m.path).collect()
    }

    #[test]
    fn parses_line_suffix() {
        assert_eq!(
            Query::parse("main.rs:42"),
            Query {
                text: "main.rs".into(),
                line: Some(42)
            }
        );
        assert_eq!(Query::parse("a:b").line, None);
        assert_eq!(Query::parse(" x ").text, "x");
    }

    #[test]
    fn ranking_table() {
        let idx = index(&[
            "crates/fb-git/src/status.rs",
            "crates/fb-tui/src/widgets/status_bar.rs",
            "crates/fb-core/src/stat_cache.rs",
            "crates/fb-search/tests/stream_tests.rs",
            "docs/statistics/readme.md",
            "src/controllers/FooBarController.java",
            "src/FooBar.java",
            "src/main.rs",
            "README.md",
        ]);
        // File-name hits beat folder hits; shorter/better names first.
        let r = top(&idx, "stat");
        assert_eq!(r[0], "crates/fb-git/src/status.rs");
        assert!(
            r.iter().position(|p| p == "docs/statistics/readme.md")
                > r.iter().position(|p| p.ends_with("stat_cache.rs"))
        );
        // Camel humps.
        assert_eq!(top(&idx, "FoBaCo")[0], "src/controllers/FooBarController.java");
        assert_eq!(top(&idx, "fbc")[0], "src/controllers/FooBarController.java");
        // Exact name wins.
        assert_eq!(top(&idx, "readme.md")[0], "README.md");
        // Path segments.
        assert_eq!(top(&idx, "tui/stat")[0], "crates/fb-tui/src/widgets/status_bar.rs");
        // No match.
        assert!(top(&idx, "zzzq").is_empty());
    }

    #[test]
    fn recent_files_get_a_boost_and_lead_empty_queries() {
        let idx = index(&["a/status.rs", "b/status.rs", "c/other.rs"]);
        let recent = vec!["b/status.rs".to_string()];
        assert_eq!(idx.search("status", &recent, 10)[0].path, "b/status.rs");
        assert_eq!(idx.search("", &recent, 10)[0].path, "b/status.rs");
        assert_eq!(idx.search("", &recent, 2).len(), 2);
    }

    #[test]
    fn indices_point_at_matched_chars() {
        let idx = index(&["crates/fb-git/src/status.rs"]);
        let m = &idx.search("stat", &[], 1)[0];
        let chars: Vec<char> = m.path.chars().collect();
        let hit: String = m.indices.iter().map(|i| chars[*i as usize]).collect();
        assert_eq!(hit, "stat");
        assert!(m.indices.iter().all(|i| *i >= 18), "matched inside the file name");
    }

    #[test]
    fn builds_from_disk_respecting_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        std::fs::create_dir_all(r.join(".git")).unwrap();
        std::fs::create_dir_all(r.join("src")).unwrap();
        std::fs::create_dir_all(r.join("target")).unwrap();
        std::fs::write(r.join(".gitignore"), "target/\n").unwrap();
        std::fs::write(r.join("src/a.rs"), "").unwrap();
        std::fs::write(r.join("target/x.o"), "").unwrap();
        std::fs::write(r.join(".git/config"), "").unwrap();
        std::fs::write(r.join(".env"), "").unwrap();
        let idx = FileIndex::build(r, false);
        assert_eq!(idx.paths(), &[".env", ".gitignore", "src/a.rs"]);
        let all = FileIndex::build(r, true);
        assert!(all.paths().contains(&"target/x.o".to_string()));
        assert!(!all.paths().iter().any(|p| p.starts_with(".git/")));
    }

    #[test]
    fn fast_enough_for_large_projects() {
        let paths: Vec<String> = (0..50_000)
            .map(|i| format!("pkg{}/module{}/sub{}/file_{i}.rs", i % 97, i % 31, i % 7))
            .collect();
        let idx = FileIndex::from_paths(Path::new("/p"), paths);
        let start = std::time::Instant::now();
        let r = idx.search("mod3file12", &[], 50);
        let took = start.elapsed();
        assert!(!r.is_empty());
        let budget = if cfg!(debug_assertions) { 400 } else { 16 };
        assert!(took.as_millis() < budget, "search took {took:?}");
    }
}
