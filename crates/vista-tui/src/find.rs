//! Find in Files: the popup (IntelliJ `Ctrl+Shift+F`) and the docked
//! Find tool window with result tabs.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vista_search::{FindOptions, Hit, Scope, Summary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Project,
    Directory,
    Changed,
    Open,
}

impl ScopeKind {
    pub const ALL: [ScopeKind; 4] = [
        ScopeKind::Project,
        ScopeKind::Directory,
        ScopeKind::Changed,
        ScopeKind::Open,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ScopeKind::Project => "In Project",
            ScopeKind::Directory => "Directory",
            ScopeKind::Changed => "Changed files",
            ScopeKind::Open => "Open files",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Query,
    Mask,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindOutcome {
    Handled,
    Close,
    /// Options changed: run the search again.
    Restart,
    Open {
        path: PathBuf,
        line: usize,
        edit: bool,
    },
    /// Move the results into the Find tool window.
    ToWindow,
}

pub struct FindPopup {
    pub query: String,
    pub mask: String,
    pub field: Field,
    pub case: bool,
    pub word: bool,
    pub regex: bool,
    pub scope: ScopeKind,
    /// Directory for the Directory scope.
    pub dir: PathBuf,
    pub results: Vec<Hit>,
    pub selected: usize,
    pub scroll: usize,
    pub view_height: usize,
    pub summary: Option<Summary>,
    pub error: Option<String>,
    pub running: bool,
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
    /// The prefilled query is replaced by the first typed character.
    pub fresh: bool,
    history_pos: Option<usize>,
}

impl FindPopup {
    pub fn new(query: String, dir: PathBuf) -> FindPopup {
        FindPopup {
            fresh: !query.is_empty(),
            query,
            mask: String::new(),
            field: Field::Query,
            case: false,
            word: false,
            regex: false,
            scope: ScopeKind::Project,
            dir,
            results: Vec::new(),
            selected: 0,
            scroll: 0,
            view_height: 10,
            summary: None,
            error: None,
            running: false,
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            history_pos: None,
        }
    }

    /// Search options for the current state; `changed` and `open` resolve the
    /// file-list scopes.
    pub fn options(&self, changed: Vec<PathBuf>, open: Vec<PathBuf>) -> FindOptions {
        FindOptions {
            query: self.query.clone(),
            case_sensitive: self.case,
            whole_word: self.word,
            regex: self.regex,
            mask: self.mask.clone(),
            scope: match self.scope {
                ScopeKind::Project => Scope::Project,
                ScopeKind::Directory => Scope::Directory(self.dir.clone()),
                ScopeKind::Changed => Scope::Files(changed),
                ScopeKind::Open => Scope::Files(open),
            },
            include_ignored: false,
            max_results: 1000,
        }
    }

    /// Adds streamed hits, keeping results sorted by path and line.
    pub fn add_hits(&mut self, hits: Vec<Hit>) {
        self.results.extend(hits);
        self.results
            .sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    }

    pub fn reset_results(&mut self) {
        self.results.clear();
        self.selected = 0;
        self.scroll = 0;
        self.summary = None;
        self.error = None;
    }

    pub fn selected_hit(&self) -> Option<&Hit> {
        self.results.get(self.selected)
    }

    /// Number of distinct files in the results.
    pub fn file_count(&self) -> usize {
        let mut n = 0;
        let mut last: Option<&Path> = None;
        for h in &self.results {
            if last != Some(h.path.as_path()) {
                n += 1;
                last = Some(&h.path);
            }
        }
        n
    }

    fn move_by(&mut self, d: isize) {
        let last = self.results.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + d).clamp(0, last.max(0)) as usize;
        let h = self.view_height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + h {
            self.scroll = self.selected + 1 - h;
        }
    }

    fn field_mut(&mut self) -> &mut String {
        match self.field {
            Field::Query => &mut self.query,
            Field::Mask => &mut self.mask,
        }
    }

    fn edited(&mut self) -> FindOutcome {
        self.history_pos = None;
        self.selected = 0;
        self.scroll = 0;
        FindOutcome::Restart
    }

    pub fn handle_key(&mut self, key: &KeyEvent, history: &[String]) -> FindOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => return FindOutcome::Close,
            KeyCode::Enter if ctrl => return FindOutcome::ToWindow,
            KeyCode::Enter => {
                return match self.selected_hit() {
                    Some(h) => FindOutcome::Open {
                        path: h.path.clone(),
                        line: h.line,
                        edit: key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT),
                    },
                    None => FindOutcome::Handled,
                };
            }
            KeyCode::Char('o') if alt => return FindOutcome::ToWindow,
            KeyCode::Char('c') if alt => {
                self.case = !self.case;
                return self.edited();
            }
            KeyCode::Char('w') if alt => {
                self.word = !self.word;
                return self.edited();
            }
            KeyCode::Char('x') if alt => {
                self.regex = !self.regex;
                return self.edited();
            }
            KeyCode::Char('s') if alt => {
                let i = ScopeKind::ALL.iter().position(|s| *s == self.scope).unwrap_or(0);
                self.scope = ScopeKind::ALL[(i + 1) % ScopeKind::ALL.len()];
                return self.edited();
            }
            KeyCode::Up if alt => {
                if history.is_empty() {
                    return FindOutcome::Handled;
                }
                let pos = self.history_pos.map(|p| (p + 1).min(history.len() - 1)).unwrap_or(0);
                self.history_pos = Some(pos);
                self.query = history[pos].clone();
                self.fresh = false;
                self.selected = 0;
                return FindOutcome::Restart;
            }
            KeyCode::Down if alt => {
                match self.history_pos {
                    Some(0) | None => {
                        self.history_pos = None;
                        self.query.clear();
                    }
                    Some(p) => {
                        self.history_pos = Some(p - 1);
                        self.query = history[p - 1].clone();
                    }
                }
                return FindOutcome::Restart;
            }
            KeyCode::Up => self.move_by(-1),
            KeyCode::Down => self.move_by(1),
            KeyCode::PageUp => self.move_by(-(self.view_height as isize)),
            KeyCode::PageDown => self.move_by(self.view_height as isize),
            KeyCode::Tab | KeyCode::BackTab => {
                self.field = match self.field {
                    Field::Query => Field::Mask,
                    Field::Mask => Field::Query,
                };
                self.fresh = false;
            }
            KeyCode::Char('u') if ctrl => {
                self.field_mut().clear();
                return self.edited();
            }
            KeyCode::Backspace => {
                if self.fresh {
                    self.query.clear();
                    self.fresh = false;
                } else {
                    self.field_mut().pop();
                }
                return self.edited();
            }
            KeyCode::Char(c) if !ctrl && !alt => {
                if self.fresh && self.field == Field::Query {
                    self.query.clear();
                }
                self.fresh = false;
                self.field_mut().push(c);
                return self.edited();
            }
            _ => {}
        }
        FindOutcome::Handled
    }

    pub fn click(&mut self, row: usize) {
        let i = self.scroll + row;
        if i < self.results.len() {
            self.selected = i;
        }
    }

    pub fn scroll_by(&mut self, d: isize) {
        self.move_by(d);
    }
}

/// A row in the Find tool window: a file header or one hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowRow {
    File { path: PathBuf, count: usize },
    Hit(usize),
}

pub struct ResultTab {
    pub title: String,
    pub hits: Vec<Hit>,
    pub rows: Vec<WindowRow>,
    pub selected: usize,
    pub scroll: usize,
    /// The hit last opened with next/previous occurrence.
    pub current: Option<usize>,
}

impl ResultTab {
    pub fn new(title: String, hits: Vec<Hit>) -> ResultTab {
        let mut rows = Vec::new();
        let mut i = 0;
        while i < hits.len() {
            let path = hits[i].path.clone();
            let count = hits[i..].iter().take_while(|h| h.path == path).count();
            rows.push(WindowRow::File { path, count });
            for k in i..i + count {
                rows.push(WindowRow::Hit(k));
            }
            i += count;
        }
        let selected = usize::from(!rows.is_empty() && rows.len() > 1);
        ResultTab {
            title,
            hits,
            rows,
            selected,
            scroll: 0,
            current: None,
        }
    }

    pub fn selected_hit(&self) -> Option<&Hit> {
        match self.rows.get(self.selected) {
            Some(WindowRow::Hit(i)) => self.hits.get(*i),
            _ => None,
        }
    }

    pub fn move_by(&mut self, d: isize, height: usize) {
        let last = self.rows.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + d).clamp(0, last.max(0)) as usize;
        let h = height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + h {
            self.scroll = self.selected + 1 - h;
        }
    }

    /// The next (or previous) hit after the current one, wrapping around.
    pub fn step(&mut self, forward: bool) -> Option<Hit> {
        if self.hits.is_empty() {
            return None;
        }
        let n = self.hits.len();
        let next = match (self.current, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(c), true) => (c + 1) % n,
            (Some(c), false) => (c + n - 1) % n,
        };
        self.current = Some(next);
        if let Some(row) = self.rows.iter().position(|r| *r == WindowRow::Hit(next)) {
            self.selected = row;
        }
        self.hits.get(next).cloned()
    }
}

#[derive(Default)]
pub struct FindWindow {
    pub tabs: Vec<ResultTab>,
    pub active: usize,
    pub visible: bool,
    pub view_height: usize,
}

impl FindWindow {
    pub fn add(&mut self, tab: ResultTab) {
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.visible = true;
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut ResultTab> {
        self.tabs.get_mut(self.active)
    }

    pub fn close_active(&mut self) {
        if self.active < self.tabs.len() {
            self.tabs.remove(self.active);
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        if self.tabs.is_empty() {
            self.visible = false;
        }
    }
}

/// Persisted query history (most recent first).
pub fn load_history(cache_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(cache_dir.join("find-history.txt"))
        .map(|s| s.lines().filter(|l| !l.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

pub fn push_history(cache_dir: &Path, history: &mut Vec<String>, query: &str) {
    if query.is_empty() {
        return;
    }
    history.retain(|q| q != query);
    history.insert(0, query.to_string());
    history.truncate(30);
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(cache_dir.join("find-history.txt"), history.join("\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(p: &str, line: usize) -> Hit {
        Hit {
            path: PathBuf::from(p),
            line,
            text: format!("text {line}"),
            ranges: vec![(0, 4)],
        }
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::from(c)
    }

    fn alt(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
    }

    #[test]
    fn typing_replaces_prefilled_query_and_toggles_restart() {
        let mut p = FindPopup::new("old".into(), PathBuf::from("/p"));
        assert_eq!(p.handle_key(&key(KeyCode::Char('n')), &[]), FindOutcome::Restart);
        assert_eq!(p.query, "n");
        assert_eq!(p.handle_key(&alt('c'), &[]), FindOutcome::Restart);
        assert!(p.case);
        p.handle_key(&alt('w'), &[]);
        p.handle_key(&alt('x'), &[]);
        assert!(p.word && p.regex);
        p.handle_key(&alt('s'), &[]);
        assert_eq!(p.scope, ScopeKind::Directory);
        p.handle_key(&key(KeyCode::Tab), &[]);
        p.handle_key(&key(KeyCode::Char('*')), &[]);
        assert_eq!((p.query.as_str(), p.mask.as_str()), ("n", "*"));
        assert_eq!(p.handle_key(&key(KeyCode::Esc), &[]), FindOutcome::Close);
    }

    #[test]
    fn history_navigation() {
        let history = vec!["second".to_string(), "first".to_string()];
        let mut p = FindPopup::new(String::new(), PathBuf::from("/p"));
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT);
        p.handle_key(&up, &history);
        assert_eq!(p.query, "second");
        p.handle_key(&up, &history);
        assert_eq!(p.query, "first");
        p.handle_key(&up, &history);
        assert_eq!(p.query, "first");
        p.handle_key(&down, &history);
        assert_eq!(p.query, "second");
        p.handle_key(&down, &history);
        assert_eq!(p.query, "");
    }

    #[test]
    fn results_sort_and_open() {
        let mut p = FindPopup::new("x".into(), PathBuf::from("/p"));
        p.add_hits(vec![hit("/p/b.rs", 3)]);
        p.add_hits(vec![hit("/p/a.rs", 9), hit("/p/a.rs", 2)]);
        assert_eq!(p.results.iter().map(|h| h.line).collect::<Vec<_>>(), vec![2, 9, 3]);
        assert_eq!(p.file_count(), 2);
        p.handle_key(&key(KeyCode::Down), &[]);
        assert_eq!(
            p.handle_key(&key(KeyCode::Enter), &[]),
            FindOutcome::Open {
                path: PathBuf::from("/p/a.rs"),
                line: 9,
                edit: false
            }
        );
        assert_eq!(p.handle_key(&alt('o'), &[]), FindOutcome::ToWindow);
    }

    #[test]
    fn result_tab_groups_by_file_and_steps_through_hits() {
        let mut t = ResultTab::new(
            "q".into(),
            vec![hit("/p/a.rs", 1), hit("/p/a.rs", 5), hit("/p/b.rs", 2)],
        );
        assert_eq!(t.rows.len(), 5);
        assert_eq!(t.selected_hit().unwrap().line, 1);
        assert_eq!(t.step(true).unwrap().line, 1);
        assert_eq!(t.step(true).unwrap().line, 5);
        assert_eq!(t.step(true).unwrap().path, PathBuf::from("/p/b.rs"));
        assert_eq!(t.step(true).unwrap().line, 1, "wraps around");
        assert_eq!(t.step(false).unwrap().path, PathBuf::from("/p/b.rs"));
    }

    #[test]
    fn history_persists() {
        let d = tempfile::tempdir().unwrap();
        let mut h = Vec::new();
        push_history(d.path(), &mut h, "a");
        push_history(d.path(), &mut h, "b");
        push_history(d.path(), &mut h, "a");
        assert_eq!(load_history(d.path()), vec!["a", "b"]);
    }
}
