//! The Go to File popup (IntelliJ "Search Everywhere" style).

use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vista_search::{FileIndex, FileMatch, Query};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GotoTab {
    All,
    Files,
    Text,
    Recent,
}

impl GotoTab {
    pub const ALL: [GotoTab; 4] = [GotoTab::All, GotoTab::Files, GotoTab::Text, GotoTab::Recent];

    pub fn label(self) -> &'static str {
        match self {
            GotoTab::All => "All",
            GotoTab::Files => "Files",
            GotoTab::Text => "Text",
            GotoTab::Recent => "Recent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GotoOutcome {
    Handled,
    Close,
    Open {
        path: PathBuf,
        line: Option<usize>,
        edit: bool,
    },
    /// The "include ignored" toggle changed: rebuild the index.
    Reindex,
    /// Switch to Find in Files with this query.
    FindText(String),
}

pub const MAX_RESULTS: usize = 500;

#[derive(Debug)]
pub struct GotoPopup {
    pub tab: GotoTab,
    pub query: String,
    pub selected: usize,
    pub scroll: usize,
    pub results: Vec<FileMatch>,
    pub include_ignored: bool,
    /// Rows of results visible in the last draw.
    pub view_height: usize,
    /// Total files in the index the results came from.
    pub indexed: usize,
}

impl GotoPopup {
    pub fn new(tab: GotoTab, include_ignored: bool) -> GotoPopup {
        GotoPopup {
            tab,
            query: String::new(),
            selected: 0,
            scroll: 0,
            results: Vec::new(),
            include_ignored,
            view_height: 10,
            indexed: 0,
        }
    }

    pub fn parsed(&self) -> Query {
        Query::parse(&self.query)
    }

    /// Recomputes results for the current query and tab.
    pub fn refresh(&mut self, index: Option<&FileIndex>, recent: &[String]) {
        // Keep the selection on the same file when results are re-ranked
        // (e.g. the background index finished).
        let keep = (self.selected > 0)
            .then(|| self.results.get(self.selected).map(|m| m.path.clone()))
            .flatten();
        let q = self.parsed();
        self.results = match (self.tab, index) {
            (GotoTab::Recent, Some(idx)) => {
                FileIndex::from_paths(&idx.root, recent.to_vec()).search(&q.text, recent, MAX_RESULTS)
            }
            (GotoTab::All | GotoTab::Files, Some(idx)) => {
                self.indexed = idx.len();
                idx.search(&q.text, recent, MAX_RESULTS)
            }
            _ => Vec::new(),
        };
        if let Some(p) = keep
            && let Some(i) = self.results.iter().position(|m| m.path == p)
        {
            self.selected = i;
        }
        self.selected = self.selected.min(self.results.len().saturating_sub(1));
        self.scroll_into_view();
    }

    fn scroll_into_view(&mut self) {
        let h = self.view_height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + h {
            self.scroll = self.selected + 1 - h;
        }
    }

    fn move_by(&mut self, d: isize) {
        let last = self.results.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + d).clamp(0, last.max(0)) as usize;
        self.scroll_into_view();
    }

    pub fn selected_path(&self, root: &std::path::Path) -> Option<PathBuf> {
        self.results.get(self.selected).map(|m| root.join(&m.path))
    }

    fn open(&self, root: &std::path::Path, edit: bool) -> GotoOutcome {
        match self.selected_path(root) {
            Some(path) => GotoOutcome::Open {
                path,
                line: self.parsed().line,
                edit,
            },
            None => GotoOutcome::Handled,
        }
    }

    fn switch_tab(&mut self, delta: isize) -> GotoOutcome {
        let tabs = GotoTab::ALL;
        let i = tabs.iter().position(|t| *t == self.tab).unwrap_or(0) as isize;
        let n = tabs.len() as isize;
        self.tab = tabs[((i + delta) % n + n) as usize % tabs.len()];
        self.selected = 0;
        self.scroll = 0;
        if self.tab == GotoTab::Text {
            return GotoOutcome::FindText(self.query.clone());
        }
        GotoOutcome::Handled
    }

    pub fn handle_key(&mut self, key: &KeyEvent, root: &std::path::Path) -> GotoOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Esc => return GotoOutcome::Close,
            KeyCode::Enter => {
                let edit = key
                    .modifiers
                    .intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT);
                return self.open(root, edit);
            }
            // Ctrl-Enter arrives as Ctrl-J in legacy terminals.
            KeyCode::Char('j') if ctrl => return self.open(root, true),
            KeyCode::Up => self.move_by(-1),
            KeyCode::Down => self.move_by(1),
            KeyCode::PageUp => self.move_by(-(self.view_height.max(1) as isize)),
            KeyCode::PageDown => self.move_by(self.view_height.max(1) as isize),
            KeyCode::Tab => return self.switch_tab(1),
            KeyCode::BackTab => return self.switch_tab(-1),
            KeyCode::Char('i') if alt => {
                self.include_ignored = !self.include_ignored;
                return GotoOutcome::Reindex;
            }
            // Repeating the shortcut moves through the scopes, like IntelliJ.
            KeyCode::Char('p') | KeyCode::Char('n') if ctrl => return self.switch_tab(1),
            KeyCode::Char('u') if ctrl => {
                self.query.clear();
                self.selected = 0;
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Char(c) if !ctrl && !alt => {
                self.query.push(c);
                self.selected = 0;
                self.scroll = 0;
            }
            _ => {}
        }
        GotoOutcome::Handled
    }

    /// A click on result row `row` (relative to the result list).
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn idx() -> FileIndex {
        FileIndex::from_paths(
            Path::new("/p"),
            vec!["src/main.rs".into(), "src/status.rs".into(), "README.md".into()],
        )
    }

    #[test]
    fn typing_filters_and_enter_opens_with_line() {
        let mut p = GotoPopup::new(GotoTab::Files, false);
        let index = idx();
        for c in "stat:12".chars() {
            p.handle_key(&KeyEvent::from(KeyCode::Char(c)), Path::new("/p"));
            p.refresh(Some(&index), &[]);
        }
        assert_eq!(p.results[0].path, "src/status.rs");
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Enter), Path::new("/p")),
            GotoOutcome::Open {
                path: PathBuf::from("/p/src/status.rs"),
                line: Some(12),
                edit: false
            }
        );
        assert!(matches!(
            p.handle_key(
                &KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
                Path::new("/p")
            ),
            GotoOutcome::Open { edit: true, .. }
        ));
    }

    #[test]
    fn tabs_cycle_and_text_hands_over() {
        let mut p = GotoPopup::new(GotoTab::Files, false);
        p.query = "needle".into();
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Tab), Path::new("/p")),
            GotoOutcome::FindText("needle".into())
        );
        p.handle_key(&KeyEvent::from(KeyCode::Tab), Path::new("/p"));
        assert_eq!(p.tab, GotoTab::Recent);
        p.refresh(Some(&idx()), &["README.md".to_string()]);
        assert_eq!(p.results.len(), 0, "query 'needle' matches no recent file");
        p.query.clear();
        p.refresh(Some(&idx()), &["README.md".to_string()]);
        assert_eq!(p.results[0].path, "README.md");
        assert_eq!(
            p.handle_key(&KeyEvent::new(KeyCode::Char('i'), KeyModifiers::ALT), Path::new("/p")),
            GotoOutcome::Reindex
        );
        assert!(p.include_ignored);
    }
}
