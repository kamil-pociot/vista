//! The Project tool window: tree state, navigation and mouse handling.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vista_core::{FileTree, Row};
use vista_git::StatusSnapshot;

/// What a key or click in the tree asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Consumed; nothing else to do.
    Handled,
    /// Open this file in the main area.
    Open(PathBuf),
    /// Not a tree key; let the global keymap have it.
    Unhandled,
}

const DOUBLE_CLICK: Duration = Duration::from_millis(500);

pub struct ProjectView {
    pub tree: FileTree,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub scroll: usize,
    pub show_ignored: bool,
    /// Rows visible in the last draw.
    pub view_height: usize,
    last_click: Option<(usize, Instant)>,
}

impl ProjectView {
    pub fn new(root: &Path, show_ignored: bool) -> ProjectView {
        let mut v = ProjectView {
            tree: FileTree::new(root),
            rows: Vec::new(),
            selected: 0,
            scroll: 0,
            show_ignored,
            view_height: 20,
            last_click: None,
        };
        v.rebuild(&StatusSnapshot::default());
        v
    }

    pub fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.selected_row().map(|r| r.path.as_path())
    }

    /// Recomputes visible rows, keeping the selection on the same path.
    pub fn rebuild(&mut self, status: &StatusSnapshot) {
        let keep_path = self.selected_path().map(Path::to_path_buf);
        let show_ignored = self.show_ignored;
        self.rows = self.tree.rows(&|p, _| show_ignored || !status.is_ignored(p));
        if let Some(p) = keep_path {
            if let Some(i) = self.rows.iter().position(|r| r.path == p) {
                self.selected = i;
            } else if let Some(i) = p
                .ancestors()
                .skip(1)
                .find_map(|a| self.rows.iter().position(|r| r.path == a))
            {
                // The selected entry vanished (deleted or hidden): select its nearest visible ancestor.
                self.selected = i;
            }
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        self.scroll_into_view();
    }

    pub fn select_path(&mut self, path: &Path) -> bool {
        if let Some(i) = self.rows.iter().position(|r| r.path == path) {
            self.selected = i;
            self.scroll_into_view();
            true
        } else {
            false
        }
    }

    pub fn scroll_into_view(&mut self) {
        let h = self.view_height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + h {
            self.scroll = self.selected + 1 - h;
        }
        let max_scroll = self.rows.len().saturating_sub(h);
        self.scroll = self.scroll.min(max_scroll);
    }

    fn move_to(&mut self, i: isize) {
        let last = self.rows.len().saturating_sub(1) as isize;
        self.selected = i.clamp(0, last.max(0)) as usize;
        self.scroll_into_view();
    }

    /// Scrolls the view without moving the selection (mouse wheel).
    pub fn scroll_by(&mut self, delta: isize) {
        let max_scroll = self.rows.len().saturating_sub(self.view_height.max(1));
        self.scroll = (self.scroll as isize + delta).clamp(0, max_scroll as isize) as usize;
    }

    fn activate(&mut self, status: &StatusSnapshot) -> Outcome {
        let Some(row) = self.selected_row().cloned() else {
            return Outcome::Handled;
        };
        if row.is_dir {
            self.tree.toggle(&row.path);
            self.rebuild(status);
            Outcome::Handled
        } else {
            Outcome::Open(row.path)
        }
    }

    pub fn handle_key(&mut self, key: &KeyEvent, status: &StatusSnapshot) -> Outcome {
        if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return Outcome::Unhandled;
        }
        let sel = self.selected as isize;
        let page = self.view_height.max(1) as isize;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_to(sel - 1),
            KeyCode::Down | KeyCode::Char('j') => self.move_to(sel + 1),
            KeyCode::PageUp => self.move_to(sel - page),
            KeyCode::PageDown => self.move_to(sel + page),
            KeyCode::Home | KeyCode::Char('g') => self.move_to(0),
            KeyCode::End | KeyCode::Char('G') => self.move_to(isize::MAX / 2),
            KeyCode::Enter => return self.activate(status),
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(row) = self.selected_row().cloned()
                    && row.is_dir
                {
                    if row.expanded {
                        self.move_to(sel + 1);
                    } else {
                        self.tree.expand(&row.path);
                        self.rebuild(status);
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(row) = self.selected_row().cloned() {
                    if row.is_dir && row.expanded && row.depth > 0 {
                        self.tree.collapse(&row.path);
                        self.rebuild(status);
                    } else if let Some(parent) = row.path.parent()
                        && let Some(i) = self.rows.iter().position(|r| r.path == parent)
                    {
                        self.move_to(i as isize);
                    }
                }
            }
            _ => return Outcome::Unhandled,
        }
        Outcome::Handled
    }

    /// A left click on visible row `view_row` at column `col` (both relative
    /// to the tree's content area).
    pub fn click(&mut self, view_row: usize, col: usize, status: &StatusSnapshot) -> Outcome {
        let i = self.scroll + view_row;
        if i >= self.rows.len() {
            return Outcome::Handled;
        }
        let now = Instant::now();
        let double = matches!(self.last_click, Some((r, t)) if r == i && now.duration_since(t) < DOUBLE_CLICK);
        self.last_click = Some((i, now));
        self.selected = i;
        let row = self.rows[i].clone();
        // The arrow sits at `depth*2 + 1 .. +3` (one leading space).
        let arrow_start = row.depth * 2 + 1;
        let on_arrow = row.is_dir && (arrow_start..arrow_start + 2).contains(&col);
        if on_arrow || (double && row.is_dir) {
            self.last_click = None;
            self.tree.toggle(&row.path);
            self.rebuild(status);
            return Outcome::Handled;
        }
        if double {
            self.last_click = None;
            return Outcome::Open(row.path);
        }
        Outcome::Handled
    }

    pub fn collapse_all(&mut self, status: &StatusSnapshot) {
        self.tree.collapse_all();
        self.selected = 0;
        self.rebuild(status);
    }

    pub fn reveal(&mut self, path: &Path, status: &StatusSnapshot) {
        self.tree.reveal(path);
        self.rebuild(status);
        self.select_path(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    fn sample() -> (tempfile::TempDir, ProjectView) {
        let t = tempfile::tempdir().unwrap();
        let r = t.path().canonicalize().unwrap();
        std::fs::create_dir_all(r.join("src/ui")).unwrap();
        std::fs::write(r.join("src/main.rs"), "").unwrap();
        std::fs::write(r.join("src/ui/tree.rs"), "").unwrap();
        std::fs::write(r.join("README.md"), "").unwrap();
        let v = ProjectView::new(&r, true);
        (t, v)
    }

    fn names(v: &ProjectView) -> Vec<&str> {
        v.rows.iter().map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn keyboard_navigation_expands_and_collapses() {
        let (_t, mut v) = sample();
        let st = StatusSnapshot::default();
        assert_eq!(&names(&v)[1..], &["src", "README.md"]);
        v.handle_key(&key(KeyCode::Down), &st);
        assert_eq!(v.selected_row().unwrap().name, "src");
        v.handle_key(&key(KeyCode::Right), &st);
        assert_eq!(&names(&v)[1..], &["src", "ui", "main.rs", "README.md"]);
        v.handle_key(&key(KeyCode::Right), &st);
        assert_eq!(
            v.selected_row().unwrap().name,
            "ui",
            "right on an open folder moves into it"
        );
        v.handle_key(&key(KeyCode::Left), &st);
        assert_eq!(
            v.selected_row().unwrap().name,
            "src",
            "left on a closed folder goes to the parent"
        );
        v.handle_key(&key(KeyCode::Left), &st);
        assert_eq!(&names(&v)[1..], &["src", "README.md"]);
        assert_eq!(v.handle_key(&key(KeyCode::End), &st), Outcome::Handled);
        assert_eq!(v.selected_row().unwrap().name, "README.md");
        assert!(matches!(v.handle_key(&key(KeyCode::Enter), &st), Outcome::Open(p) if p.ends_with("README.md")));
        assert_eq!(v.handle_key(&key(KeyCode::Char('x')), &st), Outcome::Unhandled);
    }

    #[test]
    fn clicks_select_toggle_and_open() {
        let (_t, mut v) = sample();
        let st = StatusSnapshot::default();
        // Click on the arrow of `src` (row 1, depth 1 => arrow at col 3).
        v.click(1, 3, &st);
        assert!(names(&v).contains(&"main.rs"));
        // Double-click a file opens it.
        let i = v.rows.iter().position(|r| r.name == "main.rs").unwrap();
        assert_eq!(v.click(i, 10, &st), Outcome::Handled);
        assert!(matches!(v.click(i, 10, &st), Outcome::Open(p) if p.ends_with("main.rs")));
    }

    #[test]
    fn selection_survives_rebuilds_and_falls_back_to_parent() {
        let (t, mut v) = sample();
        let st = StatusSnapshot::default();
        let root = t.path().canonicalize().unwrap();
        v.reveal(&root.join("src/ui/tree.rs"), &st);
        assert_eq!(v.selected_row().unwrap().name, "tree.rs");
        std::fs::remove_file(root.join("src/ui/tree.rs")).unwrap();
        v.tree.refresh(&[root.join("src/ui/tree.rs")]);
        v.rebuild(&st);
        assert_eq!(v.selected_row().unwrap().name, "ui");
    }

    #[test]
    fn scrolling_keeps_selection_visible() {
        let t = tempfile::tempdir().unwrap();
        for i in 0..50 {
            std::fs::write(t.path().join(format!("f{i:02}.txt")), "").unwrap();
        }
        let mut v = ProjectView::new(t.path(), true);
        v.view_height = 10;
        let st = StatusSnapshot::default();
        v.handle_key(&key(KeyCode::PageDown), &st);
        v.handle_key(&key(KeyCode::PageDown), &st);
        assert_eq!(v.selected, 20);
        assert!(v.scroll <= 20 && 20 < v.scroll + 10);
        v.scroll_by(1000);
        assert_eq!(v.scroll, 51 - 10);
        v.scroll_by(-1000);
        assert_eq!(v.scroll, 0);
    }
}
