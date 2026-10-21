//! The Commit tool window: changes list with checkboxes, message, amend.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vista_git::{Change, FileStatus, StatusSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    Changes,
    Unversioned,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Changes => "Changes",
            Group::Unversioned => "Unversioned Files",
        }
    }

    fn of(c: &Change) -> Group {
        if c.status == FileStatus::Unversioned {
            Group::Unversioned
        } else {
            Group::Changes
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Group(Group),
    /// A folder (group-by-directory mode), relative path.
    Dir {
        group: Group,
        rel: String,
    },
    File {
        change: Change,
        depth: usize,
    },
}

/// A tiny multi-line text editor for the commit message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextArea {
    pub text: String,
    /// Byte offset of the cursor.
    pub cursor: usize,
}

impl TextArea {
    pub fn set(&mut self, s: &str) {
        self.text = s.to_string();
        self.cursor = self.text.len();
    }

    fn prev_boundary(&self) -> usize {
        self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn next_boundary(&self) -> usize {
        self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.text.len())
    }

    fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map(|i| i + 1).unwrap_or(0)
    }

    fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map(|i| self.cursor + i)
            .unwrap_or(self.text.len())
    }

    fn column(&self) -> usize {
        self.text[self.line_start()..self.cursor].chars().count()
    }

    fn move_to_column(&mut self, start: usize, col: usize) {
        let end = self.text[start..]
            .find('\n')
            .map(|i| start + i)
            .unwrap_or(self.text.len());
        self.cursor = self.text[start..end]
            .char_indices()
            .nth(col)
            .map(|(i, _)| start + i)
            .unwrap_or(end);
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.text.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
            KeyCode::Enter => {
                self.text.insert(self.cursor, '\n');
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                let p = self.prev_boundary();
                self.text.replace_range(p..self.cursor, "");
                self.cursor = p;
            }
            KeyCode::Delete if self.cursor < self.text.len() => {
                let n = self.next_boundary();
                self.text.replace_range(self.cursor..n, "");
            }
            KeyCode::Left => self.cursor = self.prev_boundary(),
            KeyCode::Right => self.cursor = self.next_boundary(),
            KeyCode::Home => self.cursor = self.line_start(),
            KeyCode::End => self.cursor = self.line_end(),
            KeyCode::Up => {
                let col = self.column();
                let start = self.line_start();
                if start > 0 {
                    let prev_start = self.text[..start - 1].rfind('\n').map(|i| i + 1).unwrap_or(0);
                    self.move_to_column(prev_start, col);
                }
            }
            KeyCode::Down => {
                let col = self.column();
                let end = self.line_end();
                if end < self.text.len() {
                    self.move_to_column(end + 1, col);
                }
            }
            KeyCode::Backspace | KeyCode::Delete => {}
            _ => return false,
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    Handled,
    Unhandled,
    Commit {
        push: bool,
    },
    /// Show this change (its diff from M7 on).
    Show(Change),
    Rollback(Vec<Change>),
    Add(Vec<PathBuf>),
    AmendToggled,
}

#[derive(Debug, Default)]
pub struct CommitView {
    pub rows: Vec<Row>,
    pub selected: usize,
    pub scroll: usize,
    pub view_height: usize,
    /// Files ticked for the commit.
    pub included: HashSet<PathBuf>,
    /// Paths seen before (so new tracked changes default to ticked once).
    known: HashSet<PathBuf>,
    pub by_dir: bool,
    pub collapsed: HashSet<Group>,
    pub message: TextArea,
    pub editing_message: bool,
    pub amend: bool,
    /// Last commit/push result: (success, text).
    pub output: Option<(bool, String)>,
    pub busy: bool,
}

impl CommitView {
    /// Rebuilds rows from a status snapshot, keeping ticks and selection.
    pub fn rebuild(&mut self, status: &StatusSnapshot, root: &Path) {
        let keep = self.selected_change().map(|c| c.path.clone());
        let changes: Vec<&Change> = status
            .changes()
            .iter()
            .filter(|c| c.status != FileStatus::Ignored)
            .collect();
        let present: HashSet<PathBuf> = changes.iter().map(|c| c.path.clone()).collect();
        for c in &changes {
            if self.known.insert(c.path.clone()) && Group::of(c) == Group::Changes {
                self.included.insert(c.path.clone());
            }
        }
        self.included.retain(|p| present.contains(p));
        self.known.retain(|p| present.contains(p));
        let mut rows = Vec::new();
        for group in [Group::Changes, Group::Unversioned] {
            let items: Vec<&&Change> = changes.iter().filter(|c| Group::of(c) == group).collect();
            if items.is_empty() {
                continue;
            }
            rows.push(Row::Group(group));
            if self.collapsed.contains(&group) {
                continue;
            }
            if self.by_dir {
                let mut dirs: BTreeMap<String, Vec<&Change>> = BTreeMap::new();
                for c in &items {
                    let rel = c.path.strip_prefix(root).unwrap_or(&c.path);
                    let dir = rel
                        .parent()
                        .map(|d| d.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    dirs.entry(dir).or_default().push(c);
                }
                for (dir, files) in dirs {
                    let depth = if dir.is_empty() {
                        1
                    } else {
                        rows.push(Row::Dir { group, rel: dir });
                        2
                    };
                    for c in files {
                        rows.push(Row::File {
                            change: c.clone(),
                            depth,
                        });
                    }
                }
            } else {
                for c in items {
                    rows.push(Row::File {
                        change: (*c).clone(),
                        depth: 1,
                    });
                }
            }
        }
        self.rows = rows;
        if let Some(p) = keep
            && let Some(i) = self
                .rows
                .iter()
                .position(|r| matches!(r, Row::File { change, .. } if change.path == p))
        {
            self.selected = i;
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    pub fn selected_change(&self) -> Option<&Change> {
        match self.rows.get(self.selected) {
            Some(Row::File { change, .. }) => Some(change),
            _ => None,
        }
    }

    /// Files under a row (a file, a folder or a whole group).
    fn files_of(&self, i: usize, all: &StatusSnapshot) -> Vec<Change> {
        match self.rows.get(i) {
            Some(Row::File { change, .. }) => vec![change.clone()],
            Some(Row::Group(g)) => all
                .changes()
                .iter()
                .filter(|c| c.status != FileStatus::Ignored && Group::of(c) == *g)
                .cloned()
                .collect(),
            Some(Row::Dir { .. }) => self.rows[i + 1..]
                .iter()
                .map_while(|r| match r {
                    Row::File { change, depth: 2 } => Some(change.clone()),
                    _ => None,
                })
                .collect(),
            None => Vec::new(),
        }
    }

    /// Checkbox state of a row: Some(true) all ticked, Some(false) none, None mixed.
    pub fn tick_state(&self, i: usize, all: &StatusSnapshot) -> Option<bool> {
        let files = self.files_of(i, all);
        let n = files.iter().filter(|c| self.included.contains(&c.path)).count();
        if n == 0 {
            Some(false)
        } else if n == files.len() {
            Some(true)
        } else {
            None
        }
    }

    pub fn toggle(&mut self, i: usize, all: &StatusSnapshot) {
        let files = self.files_of(i, all);
        let tick = self.tick_state(i, all) != Some(true);
        for c in files {
            if tick {
                self.included.insert(c.path);
            } else {
                self.included.remove(&c.path);
            }
        }
    }

    /// The ticked changes, in path order.
    pub fn included_changes(&self, all: &StatusSnapshot) -> Vec<Change> {
        all.changes()
            .iter()
            .filter(|c| self.included.contains(&c.path))
            .cloned()
            .collect()
    }

    pub fn total(&self, all: &StatusSnapshot) -> usize {
        all.changes().iter().filter(|c| c.status != FileStatus::Ignored).count()
    }

    fn move_by(&mut self, d: isize) {
        let last = self.rows.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + d).clamp(0, last.max(0)) as usize;
        let h = self.view_height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + h {
            self.scroll = self.selected + 1 - h;
        }
    }

    pub fn handle_key(&mut self, key: &KeyEvent, all: &StatusSnapshot, root: &Path) -> CommitOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if ctrl && key.code == KeyCode::Char('k') {
            return CommitOutcome::Commit { push: alt };
        }
        if self.editing_message {
            match key.code {
                KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab => self.editing_message = false,
                _ => {
                    if !self.message.handle_key(key) {
                        return CommitOutcome::Unhandled;
                    }
                }
            }
            return CommitOutcome::Handled;
        }
        if ctrl || alt {
            return CommitOutcome::Unhandled;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::PageUp => self.move_by(-(self.view_height.max(1) as isize)),
            KeyCode::PageDown => self.move_by(self.view_height.max(1) as isize),
            KeyCode::Home => self.move_by(-(self.rows.len() as isize)),
            KeyCode::End => self.move_by(self.rows.len() as isize),
            KeyCode::Char(' ') => self.toggle(self.selected, all),
            KeyCode::Tab | KeyCode::Char('c') => self.editing_message = true,
            KeyCode::Left => {
                if let Some(Row::Group(g)) = self.rows.get(self.selected) {
                    self.collapsed.insert(*g);
                    self.rebuild(all, root);
                }
            }
            KeyCode::Right => {
                if let Some(Row::Group(g)) = self.rows.get(self.selected) {
                    self.collapsed.remove(g);
                    self.rebuild(all, root);
                }
            }
            KeyCode::Enter => {
                return match self.rows.get(self.selected) {
                    Some(Row::File { change, .. }) => CommitOutcome::Show(change.clone()),
                    Some(Row::Group(g)) => {
                        let g = *g;
                        if !self.collapsed.remove(&g) {
                            self.collapsed.insert(g);
                        }
                        self.rebuild(all, root);
                        CommitOutcome::Handled
                    }
                    _ => CommitOutcome::Handled,
                };
            }
            KeyCode::Char('g') => {
                self.by_dir = !self.by_dir;
                self.rebuild(all, root);
            }
            KeyCode::Char('m') => {
                self.amend = !self.amend;
                return CommitOutcome::AmendToggled;
            }
            KeyCode::Char('P') => return CommitOutcome::Commit { push: true },
            KeyCode::Char('r') => {
                let files: Vec<Change> = self
                    .files_of(self.selected, all)
                    .into_iter()
                    .filter(|c| c.status != FileStatus::Unversioned)
                    .collect();
                if !files.is_empty() {
                    return CommitOutcome::Rollback(files);
                }
            }
            KeyCode::Char('a') => {
                let files: Vec<PathBuf> = self
                    .files_of(self.selected, all)
                    .into_iter()
                    .filter(|c| c.status == FileStatus::Unversioned)
                    .map(|c| c.path)
                    .collect();
                if !files.is_empty() {
                    return CommitOutcome::Add(files);
                }
            }
            _ => return CommitOutcome::Unhandled,
        }
        CommitOutcome::Handled
    }

    pub fn click(&mut self, row: usize, all: &StatusSnapshot, on_checkbox: bool) {
        let i = self.scroll + row;
        if i < self.rows.len() {
            self.selected = i;
            self.editing_message = false;
            if on_checkbox {
                self.toggle(i, all);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> StatusSnapshot {
        let recs = [
            "1 .M N... 100644 100644 100644 a b src/a.rs",
            "1 .M N... 100644 100644 100644 a b src/b.rs",
            "1 A. N... 000000 100644 100644 0 c README.md",
            "? notes.txt",
            "? src/new.rs",
        ];
        let mut bytes = Vec::new();
        for r in recs {
            bytes.extend_from_slice(r.as_bytes());
            bytes.push(0);
        }
        StatusSnapshot::parse(Path::new("/r"), &bytes)
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::from(c)
    }

    #[test]
    fn groups_and_default_ticks() {
        let st = snap();
        let mut v = CommitView::default();
        v.rebuild(&st, Path::new("/r"));
        assert_eq!(v.rows[0], Row::Group(Group::Changes));
        assert!(v.rows.contains(&Row::Group(Group::Unversioned)));
        assert_eq!(v.included.len(), 3, "tracked changes ticked, unversioned not");
        assert_eq!(v.tick_state(0, &st), Some(true));
        let unv = v
            .rows
            .iter()
            .position(|r| *r == Row::Group(Group::Unversioned))
            .unwrap();
        assert_eq!(v.tick_state(unv, &st), Some(false));
        assert_eq!(v.total(&st), 5);
    }

    #[test]
    fn toggling_files_groups_and_dirs() {
        let st = snap();
        let mut v = CommitView::default();
        v.rebuild(&st, Path::new("/r"));
        v.selected = 1;
        v.handle_key(&key(KeyCode::Char(' ')), &st, Path::new("/r"));
        assert_eq!(v.tick_state(0, &st), None, "mixed after unticking one file");
        v.selected = 0;
        v.handle_key(&key(KeyCode::Char(' ')), &st, Path::new("/r"));
        assert_eq!(v.tick_state(0, &st), Some(true));
        v.handle_key(&key(KeyCode::Char(' ')), &st, Path::new("/r"));
        assert_eq!(v.tick_state(0, &st), Some(false));
        // Group by directory, then tick the `src` folder of unversioned files.
        v.handle_key(&key(KeyCode::Char('g')), &st, Path::new("/r"));
        let dir = v
            .rows
            .iter()
            .position(|r| matches!(r, Row::Dir { group: Group::Unversioned, rel } if rel == "src"))
            .unwrap();
        v.selected = dir;
        v.handle_key(&key(KeyCode::Char(' ')), &st, Path::new("/r"));
        assert!(v.included.contains(Path::new("/r/src/new.rs")));
        assert!(!v.included.contains(Path::new("/r/notes.txt")));
    }

    #[test]
    fn outcomes_for_commit_rollback_add_and_amend() {
        let st = snap();
        let mut v = CommitView::default();
        v.rebuild(&st, Path::new("/r"));
        v.selected = 1;
        assert!(matches!(
            v.handle_key(&key(KeyCode::Enter), &st, Path::new("/r")),
            CommitOutcome::Show(_)
        ));
        assert!(
            matches!(v.handle_key(&key(KeyCode::Char('r')), &st, Path::new("/r")), CommitOutcome::Rollback(f) if f.len() == 1)
        );
        let unv = v
            .rows
            .iter()
            .position(|r| *r == Row::Group(Group::Unversioned))
            .unwrap();
        v.selected = unv;
        assert!(
            matches!(v.handle_key(&key(KeyCode::Char('a')), &st, Path::new("/r")), CommitOutcome::Add(f) if f.len() == 2)
        );
        assert_eq!(
            v.handle_key(&key(KeyCode::Char('m')), &st, Path::new("/r")),
            CommitOutcome::AmendToggled
        );
        assert!(v.amend);
        assert_eq!(
            v.handle_key(
                &KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
                &st,
                Path::new("/r")
            ),
            CommitOutcome::Commit { push: false }
        );
    }

    #[test]
    fn message_editing() {
        let st = snap();
        let mut v = CommitView::default();
        v.rebuild(&st, Path::new("/r"));
        v.handle_key(&key(KeyCode::Tab), &st, Path::new("/r"));
        assert!(v.editing_message);
        for c in "Fix it".chars() {
            v.handle_key(&key(KeyCode::Char(c)), &st, Path::new("/r"));
        }
        v.handle_key(&key(KeyCode::Enter), &st, Path::new("/r"));
        for c in "body".chars() {
            v.handle_key(&key(KeyCode::Char(c)), &st, Path::new("/r"));
        }
        v.handle_key(&key(KeyCode::Up), &st, Path::new("/r"));
        v.handle_key(&key(KeyCode::End), &st, Path::new("/r"));
        v.handle_key(&key(KeyCode::Char('!')), &st, Path::new("/r"));
        assert_eq!(v.message.text, "Fix it!\nbody");
        v.handle_key(&key(KeyCode::Backspace), &st, Path::new("/r"));
        assert_eq!(v.message.text, "Fix it\nbody");
        v.handle_key(&key(KeyCode::Esc), &st, Path::new("/r"));
        assert!(!v.editing_message);
    }

    #[test]
    fn ticks_survive_rebuilds_and_new_changes_default_on() {
        let st = snap();
        let mut v = CommitView::default();
        v.rebuild(&st, Path::new("/r"));
        v.included.remove(Path::new("/r/src/a.rs"));
        v.rebuild(&st, Path::new("/r"));
        assert!(!v.included.contains(Path::new("/r/src/a.rs")), "user choice kept");
        assert!(v.included.contains(Path::new("/r/src/b.rs")));
    }
}
