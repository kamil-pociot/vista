//! The Review panel: every file that differs between a base revision and
//! the working tree (`vista --review <rev>`, or the control socket), listed
//! like the Commit window but without checkboxes. Enter opens a read-only
//! diff base ⇄ working tree.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vista_git::review::Review;
use vista_git::{CommitFile, FileStatus};

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A folder heading (grouped by folder), relative to the root.
    Dir(PathBuf),
    /// `files[index]`, indented under its folder when grouped.
    File { index: usize, depth: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewOutcome {
    Handled,
    /// Open this file's diff against the base.
    Open(CommitFile),
    /// Back to the Project tree.
    Close,
    Unhandled,
}

pub struct ReviewView {
    pub review: Review,
    /// What's being reviewed (the agent's session name, say), if given.
    pub label: Option<String>,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub scroll: usize,
    pub view_height: usize,
    pub by_dir: bool,
    /// Bumped per refresh; stale results are dropped.
    pub generation: u64,
}

/// A review file's status, for its colour.
pub fn status_of(f: &CommitFile) -> FileStatus {
    match f.status {
        'A' | 'C' => FileStatus::Added,
        'D' => FileStatus::Deleted,
        'R' => FileStatus::Renamed,
        _ => FileStatus::Modified,
    }
}

impl ReviewView {
    pub fn new(review: Review, label: Option<String>, by_dir: bool, root: &Path) -> ReviewView {
        let mut v = ReviewView {
            review,
            label,
            rows: Vec::new(),
            selected: 0,
            scroll: 0,
            view_height: 20,
            by_dir,
            generation: 0,
        };
        v.rebuild(root);
        // Start on the first file, not a folder heading.
        v.selected = v.rows.iter().position(|r| matches!(r, Row::File { .. })).unwrap_or(0);
        v
    }

    /// Takes a fresh list for the same base, keeping the selected file.
    pub fn refresh(&mut self, review: Review, root: &Path) {
        let keep = self.selected_file().map(|f| f.path.clone());
        self.review = review;
        self.rebuild(root);
        if let Some(p) = keep {
            self.select_path(&p);
        }
    }

    pub fn rebuild(&mut self, root: &Path) {
        let mut rows = Vec::new();
        let mut dir: Option<PathBuf> = None;
        for (index, f) in self.review.files.iter().enumerate() {
            if self.by_dir {
                let rel = f.path.strip_prefix(root).unwrap_or(&f.path);
                let parent = rel.parent().map(Path::to_path_buf).unwrap_or_default();
                if !parent.as_os_str().is_empty() && dir.as_ref() != Some(&parent) {
                    rows.push(Row::Dir(parent.clone()));
                }
                let depth = usize::from(!parent.as_os_str().is_empty());
                dir = Some(parent);
                rows.push(Row::File { index, depth });
            } else {
                rows.push(Row::File { index, depth: 0 });
            }
        }
        self.rows = rows;
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    pub fn file_at(&self, row: usize) -> Option<&CommitFile> {
        match self.rows.get(row)? {
            Row::File { index, .. } => self.review.files.get(*index),
            Row::Dir(_) => None,
        }
    }

    pub fn selected_file(&self) -> Option<&CommitFile> {
        self.file_at(self.selected)
    }

    pub fn select_path(&mut self, path: &Path) -> bool {
        match (0..self.rows.len()).find(|&i| self.file_at(i).is_some_and(|f| f.path == path)) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    fn move_by(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last.max(0)) as usize;
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> ReviewOutcome {
        if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
            return ReviewOutcome::Unhandled;
        }
        let page = self.view_height.max(1) as isize;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::PageUp => self.move_by(-page),
            KeyCode::PageDown => self.move_by(page),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.move_by(isize::MAX / 2),
            KeyCode::Enter => {
                return match self.selected_file() {
                    Some(f) => ReviewOutcome::Open(f.clone()),
                    None => ReviewOutcome::Handled,
                };
            }
            KeyCode::Esc => return ReviewOutcome::Close,
            _ => return ReviewOutcome::Unhandled,
        }
        ReviewOutcome::Handled
    }

    /// A click on visible row `view_row`.
    pub fn click(&mut self, view_row: usize) {
        let i = self.scroll + view_row;
        if i < self.rows.len() {
            self.selected = i;
        }
    }

    /// The files in list order (for `]` / `[` / `J` / `K` in a review diff).
    pub fn files_in_order(&self) -> Vec<CommitFile> {
        (0..self.rows.len()).filter_map(|i| self.file_at(i).cloned()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(paths: &[(char, &str)]) -> Review {
        Review {
            base: "a".repeat(40),
            short: "aaaaaaa".into(),
            files: paths
                .iter()
                .map(|(s, p)| CommitFile {
                    status: *s,
                    path: PathBuf::from("/r").join(p),
                    old_path: None,
                })
                .collect(),
            commits: 1,
        }
    }

    #[test]
    fn rows_flat_or_by_folder_and_keys() {
        let root = Path::new("/r");
        let r = review(&[('M', "a.txt"), ('A', "src/b.rs"), ('D', "src/c.rs")]);
        let flat = ReviewView::new(r.clone(), None, false, root);
        assert_eq!(flat.rows.len(), 3);
        let mut grouped = ReviewView::new(r, Some("fix auth".into()), true, root);
        assert_eq!(
            grouped.rows,
            [
                Row::File { index: 0, depth: 0 },
                Row::Dir(PathBuf::from("src")),
                Row::File { index: 1, depth: 1 },
                Row::File { index: 2, depth: 1 },
            ]
        );
        assert_eq!(grouped.selected_file().unwrap().path, Path::new("/r/a.txt"));
        grouped.handle_key(&KeyEvent::from(KeyCode::Down));
        // A folder heading opens nothing.
        assert_eq!(
            grouped.handle_key(&KeyEvent::from(KeyCode::Enter)),
            ReviewOutcome::Handled
        );
        grouped.handle_key(&KeyEvent::from(KeyCode::Down));
        assert!(matches!(
            grouped.handle_key(&KeyEvent::from(KeyCode::Enter)),
            ReviewOutcome::Open(f) if f.path == Path::new("/r/src/b.rs")
        ));
        assert_eq!(grouped.handle_key(&KeyEvent::from(KeyCode::Esc)), ReviewOutcome::Close);
        // In list order, without the headings.
        let order: Vec<_> = grouped.files_in_order().iter().map(|f| f.path.clone()).collect();
        assert_eq!(order, ["/r/a.txt", "/r/src/b.rs", "/r/src/c.rs"].map(PathBuf::from));
        // A refresh keeps the selected file.
        grouped.refresh(review(&[('A', "0.txt"), ('M', "a.txt"), ('A', "src/b.rs")]), root);
        assert_eq!(grouped.selected_file().unwrap().path, Path::new("/r/src/b.rs"));
        assert_eq!(status_of(&grouped.review.files[0]), FileStatus::Added);
    }
}
