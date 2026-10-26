//! The Commit tool window: changes list with checkboxes, message, amend.

use std::collections::{BTreeMap, HashMap, HashSet};
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
    /// A repository's part of a group, when changes come from more than
    /// one (repositories cloned inside the project): its top level.
    Repo {
        group: Group,
        top: PathBuf,
    },
    /// A folder (group-by-directory mode), relative to the repository's
    /// row (or the project).
    Dir {
        group: Group,
        rel: String,
        depth: usize,
    },
    File {
        change: Change,
        depth: usize,
    },
}

impl Row {
    /// Screen column of the row's checkbox; rows that fold have their
    /// arrow just before it.
    pub fn checkbox_col(&self) -> usize {
        match self {
            Row::Group(_) => 3,
            Row::Repo { .. } => 5,
            Row::Dir { depth, .. } | Row::File { depth, .. } => 1 + depth * 2,
        }
    }
}

/// A row that folds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Fold {
    Group(Group),
    Repo(Group, PathBuf),
}

/// What a repository's row is called: its path in the project, or (for
/// the project's own repository) its folder's name.
pub fn repo_label(top: &Path, root: &Path) -> String {
    match top.strip_prefix(root) {
        Ok(rel) if !rel.as_os_str().is_empty() => rel.to_string_lossy().into_owned(),
        _ => top
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| top.display().to_string()),
    }
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
    /// Changes come from more than one repository: each gets a row.
    pub by_repo: bool,
    pub collapsed: HashSet<Fold>,
    pub message: TextArea,
    pub editing_message: bool,
    pub amend: bool,
    /// Last commit/push result: (success, text).
    pub output: Option<(bool, String)>,
    pub busy: bool,
    /// Partial commits: hunk ids left out of the commit, per ticked file.
    pub excluded: HashMap<PathBuf, HashSet<u64>>,
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
        self.excluded.retain(|p, _| present.contains(p));
        self.known.retain(|p| present.contains(p));
        let top = |c: &Change| status.repo(&c.path).toplevel.clone();
        self.by_repo = changes.iter().any(|c| status.repo(&c.path).toplevel != status.toplevel);
        let mut rows = Vec::new();
        for group in [Group::Changes, Group::Unversioned] {
            let items: Vec<&Change> = changes.iter().copied().filter(|c| Group::of(c) == group).collect();
            if items.is_empty() {
                continue;
            }
            rows.push(Row::Group(group));
            if self.collapsed.contains(&Fold::Group(group)) {
                continue;
            }
            if !self.by_repo {
                self.push_files(&mut rows, group, &items, root, 1);
                continue;
            }
            // The project's repository first, then the nested ones by path.
            let mut repos: BTreeMap<(bool, PathBuf), Vec<&Change>> = BTreeMap::new();
            for c in items {
                let t = top(c);
                repos.entry((t != status.toplevel, t)).or_default().push(c);
            }
            for ((_, t), files) in repos {
                let folded = self.collapsed.contains(&Fold::Repo(group, t.clone()));
                rows.push(Row::Repo { group, top: t.clone() });
                if !folded {
                    let base = if t.starts_with(root) { &t } else { root };
                    self.push_files(&mut rows, group, &files, base, 2);
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

    /// File rows (under folders when grouping by folder, paths relative to
    /// `base`) at `depth`.
    fn push_files(&self, rows: &mut Vec<Row>, group: Group, files: &[&Change], base: &Path, depth: usize) {
        if !self.by_dir {
            rows.extend(files.iter().map(|c| Row::File {
                change: (*c).clone(),
                depth,
            }));
            return;
        }
        let mut dirs: BTreeMap<String, Vec<&Change>> = BTreeMap::new();
        for c in files {
            let rel = c.path.strip_prefix(base).unwrap_or(&c.path);
            let dir = rel
                .parent()
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default();
            dirs.entry(dir).or_default().push(c);
        }
        for (dir, files) in dirs {
            let d = if dir.is_empty() {
                depth
            } else {
                rows.push(Row::Dir { group, rel: dir, depth });
                depth + 1
            };
            for c in files {
                rows.push(Row::File {
                    change: c.clone(),
                    depth: d,
                });
            }
        }
    }

    /// What a file's folder is shown relative to: its repository's row,
    /// or the project.
    pub fn base<'a>(&self, path: &Path, all: &'a StatusSnapshot, root: &'a Path) -> &'a Path {
        let top = &all.repo(path).toplevel;
        if self.by_repo && top.starts_with(root) {
            top
        } else {
            root
        }
    }

    /// Whether a group or repository row is folded.
    pub fn is_folded(&self, row: &Row) -> bool {
        Self::fold_of(row).is_some_and(|f| self.collapsed.contains(&f))
    }

    fn fold_of(row: &Row) -> Option<Fold> {
        match row {
            Row::Group(g) => Some(Fold::Group(*g)),
            Row::Repo { group, top } => Some(Fold::Repo(*group, top.clone())),
            _ => None,
        }
    }

    /// Folds (`Some(true)`), unfolds or flips (`None`) the selected group
    /// or repository.
    fn fold(&mut self, fold: Option<bool>, all: &StatusSnapshot, root: &Path) {
        let Some(f) = self.rows.get(self.selected).and_then(Self::fold_of) else {
            return;
        };
        if fold.unwrap_or(!self.collapsed.contains(&f)) {
            self.collapsed.insert(f);
        } else {
            self.collapsed.remove(&f);
        }
        self.rebuild(all, root);
    }

    pub fn selected_change(&self) -> Option<&Change> {
        match self.rows.get(self.selected) {
            Some(Row::File { change, .. }) => Some(change),
            _ => None,
        }
    }

    /// Files under a row (a file, a folder, a repository or a whole
    /// group; folded ones too).
    pub fn files_of(&self, i: usize, all: &StatusSnapshot) -> Vec<Change> {
        let in_group = |g: Group| {
            all.changes()
                .iter()
                .filter(move |c| c.status != FileStatus::Ignored && Group::of(c) == g)
        };
        match self.rows.get(i) {
            Some(Row::File { change, .. }) => vec![change.clone()],
            Some(Row::Group(g)) => in_group(*g).cloned().collect(),
            Some(Row::Repo { group, top }) => in_group(*group)
                .filter(|c| all.repo(&c.path).toplevel == *top)
                .cloned()
                .collect(),
            Some(Row::Dir { depth, .. }) => self.rows[i + 1..]
                .iter()
                .map_while(|r| match r {
                    Row::File { change, depth: d } if d > depth => Some(change.clone()),
                    _ => None,
                })
                .collect(),
            None => Vec::new(),
        }
    }

    /// Whether a file is partly in the commit (some changes left out).
    pub fn is_partial(&self, path: &Path) -> bool {
        self.included.contains(path) && self.excluded.get(path).is_some_and(|e| !e.is_empty())
    }

    /// Checkbox state of a row: Some(true) all ticked, Some(false) none, None mixed.
    pub fn tick_state(&self, i: usize, all: &StatusSnapshot) -> Option<bool> {
        let files = self.files_of(i, all);
        let n = files.iter().filter(|c| self.included.contains(&c.path)).count();
        let partial = files.iter().any(|c| self.is_partial(&c.path));
        if n == 0 {
            Some(false)
        } else if n == files.len() && !partial {
            Some(true)
        } else {
            None
        }
    }

    /// Ticks or unticks one change of a file (IntelliJ's per-change
    /// checkboxes). `all` are the file's hunk ids.
    pub fn toggle_hunk(&mut self, path: &Path, id: u64, all: &[u64]) {
        let p = path.to_path_buf();
        if !self.included.contains(&p) {
            // Ticking one change of an unticked file commits just that change.
            self.included.insert(p.clone());
            self.excluded
                .insert(p, all.iter().copied().filter(|h| *h != id).collect());
            return;
        }
        let ex = self.excluded.entry(p.clone()).or_default();
        if !ex.remove(&id) {
            ex.insert(id);
        }
        if all.iter().all(|h| ex.contains(h)) {
            // Nothing left: the file is out of the commit.
            self.included.remove(&p);
            self.excluded.remove(&p);
        }
    }

    /// Whether a change of `path` goes into the commit.
    pub fn hunk_included(&self, path: &Path, id: u64) -> bool {
        self.included.contains(path) && !self.excluded.get(path).is_some_and(|e| e.contains(&id))
    }

    pub fn toggle(&mut self, i: usize, all: &StatusSnapshot) {
        let files = self.files_of(i, all);
        let tick = self.tick_state(i, all) != Some(true);
        for c in files {
            // File-level ticks replace any per-change choice.
            self.excluded.remove(&c.path);
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
            KeyCode::Left => self.fold(Some(true), all, root),
            KeyCode::Right => self.fold(Some(false), all, root),
            KeyCode::Enter => {
                if let Some(Row::File { change, .. }) = self.rows.get(self.selected) {
                    return CommitOutcome::Show(change.clone());
                }
                self.fold(None, all, root);
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
            .position(|r| matches!(r, Row::Dir { group: Group::Unversioned, rel, .. } if rel == "src"))
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
    fn per_change_ticks_make_files_partial() {
        let st = snap();
        let mut v = CommitView::default();
        v.rebuild(&st, Path::new("/r"));
        let a = Path::new("/r/src/a.rs");
        let all = [1u64, 2, 3];
        v.toggle_hunk(a, 2, &all);
        assert!(v.is_partial(a));
        assert!(!v.hunk_included(a, 2) && v.hunk_included(a, 1));
        let row = v
            .rows
            .iter()
            .position(|r| matches!(r, Row::File { change, .. } if change.path == a))
            .unwrap();
        assert_eq!(v.tick_state(row, &st), None, "partial file shows [~]");
        assert_eq!(v.tick_state(0, &st), None, "and so does its group");
        // Unticking the remaining changes drops the file from the commit.
        v.toggle_hunk(a, 1, &all);
        v.toggle_hunk(a, 3, &all);
        assert!(!v.included.contains(a));
        assert!(!v.is_partial(a));
        // Ticking one change of an unticked file commits only that change.
        v.toggle_hunk(a, 3, &all);
        assert!(v.hunk_included(a, 3) && !v.hunk_included(a, 1));
        // A file-level tick on a partial file includes all of it.
        v.toggle(row, &st);
        assert!(v.included.contains(a) && !v.is_partial(a));
        v.toggle(row, &st);
        assert!(!v.included.contains(a));
    }

    /// `snap()` plus a repository cloned at `/r/repos/foo`.
    fn nested_snap() -> StatusSnapshot {
        let parse = |top: &str, recs: &[&str]| {
            let mut bytes = Vec::new();
            for r in recs {
                bytes.extend_from_slice(r.as_bytes());
                bytes.push(0);
            }
            StatusSnapshot::parse(Path::new(top), &bytes)
        };
        let mut st = snap();
        st.nest(vec![parse(
            "/r/repos/foo",
            &[
                "1 .M N... 100644 100644 100644 a b lib/x.rs",
                "1 .M N... 100644 100644 100644 a b y.rs",
                "? todo.txt",
            ],
        )]);
        st
    }

    #[test]
    fn changes_are_split_by_repository() {
        let st = nested_snap();
        let root = Path::new("/r");
        let mut v = CommitView::default();
        v.rebuild(&st, root);
        assert!(v.by_repo);
        let foo = PathBuf::from("/r/repos/foo");
        let repo = |g: Group, t: &Path| Row::Repo {
            group: g,
            top: t.to_path_buf(),
        };
        let at = |v: &CommitView, r: &Row| v.rows.iter().position(|x| x == r).unwrap();
        // Changes › r (the project's repository) then repos/foo; the same for unversioned files.
        assert_eq!(v.rows[0], Row::Group(Group::Changes));
        assert_eq!(v.rows[1], repo(Group::Changes, root));
        let foo_row = at(&v, &repo(Group::Changes, &foo));
        assert!(matches!(&v.rows[foo_row + 1], Row::File { change, depth: 2 } if change.path == foo.join("lib/x.rs")));
        assert!(at(&v, &repo(Group::Unversioned, &foo)) > at(&v, &repo(Group::Unversioned, root)));
        assert_eq!(v.files_of(foo_row, &st).len(), 2);
        assert_eq!(repo_label(&foo, root), "repos/foo");
        assert_eq!(repo_label(root, root), "r");
        assert_eq!(v.base(&foo.join("y.rs"), &st, root), foo);
        assert_eq!(v.base(Path::new("/r/src/a.rs"), &st, root), root);

        // Ticking a repository's row ticks just its files.
        v.selected = foo_row;
        v.handle_key(&key(KeyCode::Char(' ')), &st, root);
        assert_eq!(v.tick_state(foo_row, &st), Some(false));
        assert!(!v.included.contains(&foo.join("y.rs")));
        assert!(v.included.contains(Path::new("/r/src/a.rs")));
        assert_eq!(v.tick_state(0, &st), None, "the group is mixed");

        // A repository folds on its own (and still ticks as a whole).
        v.handle_key(&key(KeyCode::Left), &st, root);
        assert!(v.is_folded(&v.rows[foo_row]));
        assert!(
            !v.rows
                .iter()
                .any(|r| matches!(r, Row::File { change, .. } if change.path == foo.join("y.rs")))
        );
        v.handle_key(&key(KeyCode::Char(' ')), &st, root);
        assert!(v.included.contains(&foo.join("y.rs")));
        v.handle_key(&key(KeyCode::Enter), &st, root);
        assert!(!v.is_folded(&v.rows[foo_row]));

        // Grouping by folder: folders relative to each repository, a level deeper.
        v.handle_key(&key(KeyCode::Char('g')), &st, root);
        let dir = v
            .rows
            .iter()
            .position(|r| matches!(r, Row::Dir { group: Group::Changes, rel, depth: 2 } if rel == "lib"))
            .unwrap();
        assert!(matches!(&v.rows[dir + 1], Row::File { depth: 3, .. }));
        assert_eq!(v.rows[dir].checkbox_col(), 5);
        assert_eq!(v.files_of(dir, &st).len(), 1);
    }

    #[test]
    fn repository_rows_only_when_a_nested_repository_has_changes() {
        let mut v = CommitView::default();
        v.rebuild(&snap(), Path::new("/r"));
        assert!(!v.by_repo);
        assert!(!v.rows.iter().any(|r| matches!(r, Row::Repo { .. })));
        // Changes only in the nested one: its row says where they are.
        let mut only_foo = StatusSnapshot::parse(Path::new("/r"), b"");
        only_foo.nest(vec![StatusSnapshot::parse(Path::new("/r/repos/foo"), b"? todo.txt\0")]);
        v.rebuild(&only_foo, Path::new("/r"));
        assert!(v.by_repo);
        assert_eq!(
            v.rows[1],
            Row::Repo {
                group: Group::Unversioned,
                top: PathBuf::from("/r/repos/foo")
            }
        );
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
