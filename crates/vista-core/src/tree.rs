//! A lazily loaded file tree: directories are read only when expanded.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// A visible row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
}

/// Natural, case-insensitive ordering: `file2` < `file10`, `a` < `B`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Reads one directory: folders first, then files, each in natural order.
/// `.git` is always hidden.
pub fn read_dir_sorted(dir: &Path) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == ".git" {
                return None;
            }
            let path = e.path();
            // Follow symlinks for the dir/file decision; broken links count as files.
            let is_dir = std::fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false);
            Some(Entry { name, path, is_dir })
        })
        .collect();
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| natural_cmp(&a.name, &b.name)));
    entries
}

#[derive(Debug, Clone)]
pub struct FileTree {
    root: PathBuf,
    expanded: HashSet<PathBuf>,
    children: HashMap<PathBuf, Vec<Entry>>,
}

impl FileTree {
    /// A tree whose root is expanded.
    pub fn new(root: &Path) -> FileTree {
        let mut t = FileTree {
            root: root.to_path_buf(),
            expanded: HashSet::new(),
            children: HashMap::new(),
        };
        t.expand(root);
        t
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_expanded(&self, dir: &Path) -> bool {
        self.expanded.contains(dir)
    }

    fn load(&mut self, dir: &Path) {
        if !self.children.contains_key(dir) {
            self.children.insert(dir.to_path_buf(), read_dir_sorted(dir));
        }
    }

    pub fn expand(&mut self, dir: &Path) {
        self.load(dir);
        self.expanded.insert(dir.to_path_buf());
    }

    pub fn collapse(&mut self, dir: &Path) {
        self.expanded.remove(dir);
    }

    /// Expands or collapses; returns the new state.
    pub fn toggle(&mut self, dir: &Path) -> bool {
        if self.is_expanded(dir) {
            self.collapse(dir);
            false
        } else {
            self.expand(dir);
            true
        }
    }

    /// Collapses everything except the root.
    pub fn collapse_all(&mut self) {
        let root = self.root.clone();
        self.expanded.retain(|p| *p == root);
    }

    /// Expands every ancestor of `path` so it becomes visible.
    pub fn reveal(&mut self, path: &Path) {
        let ancestors: Vec<PathBuf> = path
            .ancestors()
            .skip(1)
            .take_while(|p| p.starts_with(&self.root))
            .map(Path::to_path_buf)
            .collect();
        for a in ancestors.into_iter().rev() {
            self.expand(&a);
        }
    }

    /// Re-reads the directories containing `paths` (after file-system changes).
    /// Directories that disappeared are dropped.
    pub fn refresh(&mut self, paths: &[PathBuf]) {
        let mut dirs: HashSet<PathBuf> = HashSet::new();
        for p in paths {
            if let Some(parent) = p.parent() {
                dirs.insert(parent.to_path_buf());
            }
            dirs.insert(p.clone());
        }
        for d in dirs {
            if self.children.contains_key(&d) {
                if d.is_dir() {
                    self.children.insert(d.clone(), read_dir_sorted(&d));
                } else {
                    self.children.remove(&d);
                    self.expanded.remove(&d);
                }
            }
        }
    }

    /// Re-reads every loaded directory.
    pub fn refresh_all(&mut self) {
        let dirs: Vec<PathBuf> = self.children.keys().cloned().collect();
        self.refresh(&dirs);
    }

    /// Visible rows, depth-first. `keep(path, is_dir)` can hide entries
    /// (e.g. ignored files); the root row is always present.
    pub fn rows(&self, keep: &dyn Fn(&Path, bool) -> bool) -> Vec<Row> {
        let mut out = vec![Row {
            path: self.root.clone(),
            name: self
                .root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.root.display().to_string()),
            depth: 0,
            is_dir: true,
            expanded: self.is_expanded(&self.root),
        }];
        if self.is_expanded(&self.root) {
            self.push_children(&self.root, 1, keep, &mut out);
        }
        out
    }

    fn push_children(&self, dir: &Path, depth: usize, keep: &dyn Fn(&Path, bool) -> bool, out: &mut Vec<Row>) {
        let Some(children) = self.children.get(dir) else { return };
        for e in children {
            if !keep(&e.path, e.is_dir) {
                continue;
            }
            let expanded = e.is_dir && self.is_expanded(&e.path);
            out.push(Row {
                path: e.path.clone(),
                name: e.name.clone(),
                depth,
                is_dir: e.is_dir,
                expanded,
            });
            if expanded {
                self.push_children(&e.path, depth + 1, keep, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sample() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        for d in ["src/ui", "docs", ".git/objects"] {
            fs::create_dir_all(r.join(d)).unwrap();
        }
        for f in [
            "src/main.rs",
            "src/ui/tree.rs",
            "docs/a.md",
            "README.md",
            "file10.txt",
            "file2.txt",
            "b.txt",
            "A.txt",
        ] {
            fs::write(r.join(f), "x").unwrap();
        }
        t
    }

    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| format!("{}{}", "  ".repeat(r.depth), r.name))
            .collect()
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["file10", "file2", "File1", "b", "a10b", "a9b", "a"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a", "a9b", "a10b", "b", "File1", "file2", "file10"]);
    }

    #[test]
    fn root_is_expanded_with_folders_first_and_git_hidden() {
        let t = sample();
        let tree = FileTree::new(t.path());
        let rows = tree.rows(&|_, _| true);
        let n = names(&rows);
        assert_eq!(
            &n[1..],
            &[
                "  docs",
                "  src",
                "  A.txt",
                "  b.txt",
                "  file2.txt",
                "  file10.txt",
                "  README.md"
            ]
        );
        assert!(rows[0].expanded && rows[0].depth == 0);
    }

    #[test]
    fn expand_collapse_toggle_and_collapse_all() {
        let t = sample();
        let mut tree = FileTree::new(t.path());
        let src = t.path().join("src");
        assert!(tree.toggle(&src));
        assert!(names(&tree.rows(&|_, _| true)).contains(&"    main.rs".to_string()));
        tree.expand(&src.join("ui"));
        assert!(names(&tree.rows(&|_, _| true)).contains(&"      tree.rs".to_string()));
        tree.collapse_all();
        let rows = tree.rows(&|_, _| true);
        assert!(!names(&rows).contains(&"    main.rs".to_string()));
        assert!(rows[0].expanded, "root stays open");
    }

    #[test]
    fn reveal_expands_ancestors() {
        let t = sample();
        let mut tree = FileTree::new(t.path());
        tree.reveal(&t.path().join("src/ui/tree.rs"));
        let rows = tree.rows(&|_, _| true);
        assert!(rows.iter().any(|r| r.name == "tree.rs" && r.depth == 3));
    }

    #[test]
    fn filter_hides_entries() {
        let t = sample();
        let tree = FileTree::new(t.path());
        let rows = tree.rows(&|p, _| !p.ends_with("docs"));
        assert!(!rows.iter().any(|r| r.name == "docs"));
    }

    #[test]
    fn refresh_picks_up_new_and_removed_entries() {
        let t = sample();
        let mut tree = FileTree::new(t.path());
        let src = t.path().join("src");
        tree.expand(&src);
        fs::write(src.join("new.rs"), "x").unwrap();
        fs::remove_dir_all(src.join("ui")).unwrap();
        tree.refresh(&[src.join("new.rs"), src.join("ui")]);
        let n = names(&tree.rows(&|_, _| true));
        assert!(n.contains(&"    new.rs".to_string()));
        assert!(!n.contains(&"    ui".to_string()));
    }

    #[test]
    fn unreadable_or_missing_dirs_are_empty() {
        assert!(read_dir_sorted(Path::new("/definitely/not/here")).is_empty());
    }
}
