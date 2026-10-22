//! Several projects in one vista: the tab row in the top bar, and the
//! Projects popup (switch, reopen, add by path, close). The projects
//! themselves live in [`crate::workspace::Workspace`]; a project's `App` only
//! sees this summary and asks the workspace for changes.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The open projects, as the active one shows them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectsBar {
    /// (name, root) of every open project, in tab order.
    pub open: Vec<(String, PathBuf)>,
    pub active: usize,
    /// Projects opened before (most recent first), not open now.
    pub recent: Vec<PathBuf>,
    /// Open terminal tabs (their commands), after the projects.
    pub terminals: Vec<String>,
    /// The terminal tab on screen, if any (then no project is).
    pub terminal: Option<usize>,
    /// Tabs whose program asked for a notification (Claude Code waiting for
    /// input) since they were last on screen.
    pub waiting: Vec<TabTarget>,
    /// Commands offered for new terminal tabs (`[terminal] commands`).
    pub commands: Vec<String>,
}

/// What a tab in the top bar is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabTarget {
    Project(usize),
    Terminal(usize),
    /// The + button.
    Plus,
}

/// Where a tab was drawn in the top bar: screen columns of the whole tab
/// and of its ×.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectHit {
    pub target: TabTarget,
    pub x: (u16, u16),
    pub close: Option<(u16, u16)>,
}

impl ProjectHit {
    /// What clicking column `col` of this tab asks for.
    pub fn request(&self, col: u16) -> Option<WsRequest> {
        let on_close = self.close.is_some_and(|c| (c.0..c.1).contains(&col));
        match (self.target, on_close) {
            (TabTarget::Project(i), true) => Some(WsRequest::Close(i)),
            (TabTarget::Project(i), false) => Some(WsRequest::Switch(i)),
            (TabTarget::Terminal(i), true) => Some(WsRequest::CloseTerminal(i)),
            (TabTarget::Terminal(i), false) => Some(WsRequest::ShowTerminal(i)),
            (TabTarget::Plus, _) => None,
        }
    }
}

/// What a project asks the workspace to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsRequest {
    /// Show open project `i`.
    Switch(usize),
    /// The next (true) or previous open project.
    Cycle(bool),
    /// Open (or switch to) the project at this folder.
    Open(PathBuf),
    /// Close open project `i`.
    Close(usize),
    /// Show terminal tab `i`.
    ShowTerminal(usize),
    CloseTerminal(usize),
    /// Run a command in a new terminal tab.
    Run(String),
}

/// Whether a tab dragged from slot `from` should move to slot `to`, with
/// the mouse now at `col` and before at `last`: only the way the mouse is
/// going. (After a move the tabs are laid out again; a wider neighbour
/// would otherwise sit under the mouse and swap straight back.)
pub fn drag_moves(from: usize, to: usize, col: u16, last: u16) -> bool {
    (to > from && col > last) || (to < from && col < last)
}

/// The name a project goes by: its folder's name.
pub fn project_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

/// `$HOME/x` → `~/x`, for showing and completing paths.
pub fn tilde(path: &Path) -> String {
    match dirs::home_dir().and_then(|h| path.strip_prefix(h).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// `~/x` → `$HOME/x`; relative paths are left alone.
pub fn expand_home(input: &str) -> PathBuf {
    match input.strip_prefix("~") {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            dirs::home_dir().unwrap_or_default().join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(input),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// An open project (index into `ProjectsBar::open`).
    Open(usize),
    /// A project opened before.
    Recent(PathBuf),
    /// A folder typed as a path, to add.
    Add(PathBuf),
    /// An open terminal tab.
    Terminal(usize),
    /// A command to run in a new terminal tab.
    Run(String),
}

/// At most this many folders are offered while typing.
const MAX_FOLDERS: usize = 50;

/// How well a folder name matches what was typed (lower is better): 0 it
/// starts with it, 1 a word in it does (`project-two` for "two"), 2 it's
/// somewhere inside; `None` no match. Case-insensitive.
fn folder_score(name: &str, want: &str) -> Option<u8> {
    let lower = name.to_lowercase();
    if lower.starts_with(want) {
        return Some(0);
    }
    let at_word = lower
        .match_indices(want)
        .any(|(i, _)| lower[..i].ends_with(['-', '_', '.', ' ']));
    if at_word {
        Some(1)
    } else {
        lower.contains(want).then_some(2)
    }
}

/// Sub-folders of the `dirs` whose names match `partial`, best first (see
/// [`folder_score`]); hidden folders only when `partial` starts with a dot.
pub fn matching_folders_in(dirs: &[&Path], partial: &str) -> Vec<PathBuf> {
    let want = partial.to_lowercase();
    let mut hits: Vec<(u8, String, PathBuf)> = Vec::new();
    for dir in dirs {
        let Ok(read) = std::fs::read_dir(dir) else { continue };
        for e in read.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            if (name.starts_with('.') && !want.starts_with('.')) || !e.path().is_dir() {
                continue;
            }
            if let Some(score) = folder_score(&name, &want) {
                hits.push((score, name.to_lowercase(), e.path()));
            }
        }
    }
    hits.sort();
    hits.dedup_by(|a, b| a.2 == b.2);
    hits.into_iter().take(MAX_FOLDERS).map(|(_, _, p)| p).collect()
}

/// [`matching_folders_in`] for one folder.
pub fn matching_folders(dir: &Path, partial: &str) -> Vec<PathBuf> {
    matching_folders_in(&[dir], partial)
}

/// Ctrl-O: open projects first, then recent ones, then folders next to them;
/// type to filter. Typing a path (`/…`, `~/…`, `./…`) lists the matching
/// folders there instead; Tab completes the selected one.
pub struct ProjectsPopup {
    pub query: String,
    pub selected: usize,
    pub entries: Vec<Entry>,
    /// First entry shown; the list scrolls to keep `selected` in view.
    pub scroll: usize,
    bar: ProjectsBar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupOutcome {
    Handled,
    Close,
    Request(WsRequest),
}

impl ProjectsPopup {
    pub fn new(bar: ProjectsBar) -> ProjectsPopup {
        let mut p = ProjectsPopup {
            query: String::new(),
            selected: 0,
            entries: Vec::new(),
            scroll: 0,
            bar,
        };
        p.refilter();
        // Start on the next project, so Ctrl-O Enter flips between two.
        if p.bar.open.len() > 1 {
            p.selected = (p.bar.active + 1) % p.bar.open.len();
        }
        p
    }

    /// (label, detail, is active) for an entry.
    pub fn describe(&self, e: &Entry) -> (String, String, bool) {
        match e {
            Entry::Open(i) => {
                let (name, root) = &self.bar.open[*i];
                (name.clone(), root.display().to_string(), *i == self.bar.active)
            }
            Entry::Recent(p) => (project_name(p), p.display().to_string(), false),
            Entry::Add(p) => (project_name(p), p.display().to_string(), false),
            Entry::Terminal(i) => (
                self.bar.terminals[*i].clone(),
                "terminal tab".into(),
                self.bar.terminal == Some(*i),
            ),
            Entry::Run(cmd) if cmd.is_empty() => ("shell".into(), "a new terminal tab with your shell".into(), false),
            Entry::Run(cmd) => (cmd.clone(), "run in a new terminal tab".into(), false),
        }
    }

    fn refilter(&mut self) {
        let q = self.query.trim().to_lowercase();
        let matches = |name: &str, path: &Path| {
            q.is_empty() || name.to_lowercase().contains(&q) || path.to_string_lossy().to_lowercase().contains(&q)
        };
        let mut entries: Vec<Entry> = Vec::new();
        let looks_like_path = ["/", "~", "./", "../"].iter().any(|p| self.query.trim().starts_with(p));
        let as_entry = |path: PathBuf, bar: &ProjectsBar| match bar.open.iter().position(|(_, r)| *r == path) {
            Some(i) => Entry::Open(i),
            None => Entry::Add(path),
        };
        if let Some(cmd) = self.query.trim().strip_prefix('>') {
            // `>command`: run it in a terminal tab.
            // `>` alone: your shell.
            entries.push(Entry::Run(cmd.trim().to_string()));
        } else if looks_like_path {
            // `~/Repos/lm` → folders in ~/Repos matching "lm"; `~/Repos/` → all.
            let typed = self.query.trim();
            let full = expand_home(typed);
            let (dir, partial) = if typed.ends_with('/') {
                (full, String::new())
            } else {
                let partial = full
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (full.parent().map(Path::to_path_buf).unwrap_or_default(), partial)
            };
            let dir = dir.canonicalize().unwrap_or(dir);
            // `~/Repos/app/` (what Tab completes to) offers app itself
            // first, so Enter opens it; its sub-folders follow.
            if typed.ends_with('/') && dir.is_dir() {
                entries.push(as_entry(dir.clone(), &self.bar));
            }
            for path in matching_folders(&dir, &partial) {
                entries.push(as_entry(path, &self.bar));
            }
        } else {
            for (i, (name, root)) in self.bar.open.iter().enumerate() {
                if matches(name, root) {
                    entries.push(Entry::Open(i));
                }
            }
            for (i, t) in self.bar.terminals.iter().enumerate() {
                if q.is_empty() || t.to_lowercase().contains(&q) {
                    entries.push(Entry::Terminal(i));
                }
            }
            for r in &self.bar.recent {
                if matches(&project_name(r), r) {
                    entries.push(Entry::Recent(r.clone()));
                }
            }
            for c in &self.bar.commands {
                if q.is_empty() || c.to_lowercase().contains(&q) {
                    entries.push(Entry::Run(c.clone()));
                }
            }
            // Folders next to known projects (often all in ~/Repositories),
            // by name, so a few letters are enough.
            if !q.is_empty() {
                let known: Vec<&PathBuf> = self.bar.open.iter().map(|(_, r)| r).chain(&self.bar.recent).collect();
                let mut parents: Vec<&Path> = known.iter().filter_map(|r| r.parent()).collect();
                parents.sort();
                parents.dedup();
                for path in matching_folders_in(&parents, &q) {
                    if !known.contains(&&path) && entries.len() < MAX_FOLDERS {
                        entries.push(Entry::Add(path));
                    }
                }
            }
        }
        self.entries = entries;
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> PopupOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return PopupOutcome::Close,
            KeyCode::Enter => {
                return match self.entries.get(self.selected) {
                    Some(Entry::Open(i)) => PopupOutcome::Request(WsRequest::Switch(*i)),
                    Some(Entry::Recent(p) | Entry::Add(p)) => PopupOutcome::Request(WsRequest::Open(p.clone())),
                    Some(Entry::Terminal(i)) => PopupOutcome::Request(WsRequest::ShowTerminal(*i)),
                    Some(Entry::Run(cmd)) => PopupOutcome::Request(WsRequest::Run(cmd.clone())),
                    None => PopupOutcome::Handled,
                };
            }
            // Ctrl-W closes the selected open project.
            KeyCode::Char('w') if ctrl => match self.entries.get(self.selected) {
                Some(Entry::Open(i)) => return PopupOutcome::Request(WsRequest::Close(*i)),
                Some(Entry::Terminal(i)) => return PopupOutcome::Request(WsRequest::CloseTerminal(*i)),
                _ => {}
            },
            // Tab: complete the selected folder into the query, to go deeper.
            KeyCode::Tab => {
                let path = match self.entries.get(self.selected) {
                    Some(Entry::Open(i)) => Some(self.bar.open[*i].1.clone()),
                    Some(Entry::Recent(p) | Entry::Add(p)) => Some(p.clone()),
                    _ => None,
                };
                if let Some(p) = path {
                    self.query = format!("{}/", tilde(&p));
                    self.selected = 0;
                    self.refilter();
                }
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.entries.len().saturating_sub(1));
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
                self.refilter();
            }
            KeyCode::Char(c) if !ctrl => {
                self.query.push(c);
                self.selected = 0;
                self.refilter();
            }
            _ => {}
        }
        PopupOutcome::Handled
    }

    /// Scrolls so the selected entry is among the `rows` shown.
    pub fn keep_visible(&mut self, rows: usize) {
        let rows = rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + rows {
            self.scroll = self.selected + 1 - rows;
        }
        self.scroll = self.scroll.min(self.entries.len().saturating_sub(rows));
    }

    /// Clicking row `row` of the list opens the entry shown there.
    pub fn click(&mut self, row: usize) -> PopupOutcome {
        let i = self.scroll + row;
        if i < self.entries.len() {
            self.selected = i;
            return self.handle_key(&KeyEvent::from(KeyCode::Enter));
        }
        PopupOutcome::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(dir: &Path) -> ProjectsBar {
        ProjectsBar {
            open: vec![("alpha".into(), dir.join("alpha")), ("beta".into(), dir.join("beta"))],
            active: 0,
            recent: vec![dir.join("gamma")],
            ..ProjectsBar::default()
        }
    }

    fn typed(p: &mut ProjectsPopup, s: &str) {
        for c in s.chars() {
            p.handle_key(&KeyEvent::from(KeyCode::Char(c)));
        }
    }

    #[test]
    fn lists_open_then_recent_and_starts_on_the_next_project() {
        let dir = tempfile::tempdir().unwrap();
        let p = ProjectsPopup::new(bar(dir.path()));
        assert_eq!(
            p.entries,
            vec![Entry::Open(0), Entry::Open(1), Entry::Recent(dir.path().join("gamma"))]
        );
        assert_eq!(p.selected, 1);
        assert!(p.describe(&Entry::Open(0)).2, "the active project is marked");
    }

    #[test]
    fn filters_switches_reopens_and_closes() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = ProjectsPopup::new(bar(dir.path()));
        typed(&mut p, "gam");
        assert_eq!(p.entries, vec![Entry::Recent(dir.path().join("gamma"))]);
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Enter)),
            PopupOutcome::Request(WsRequest::Open(dir.path().join("gamma")))
        );
        let mut p = ProjectsPopup::new(bar(dir.path()));
        typed(&mut p, "bet");
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Enter)),
            PopupOutcome::Request(WsRequest::Switch(1))
        );
        assert_eq!(
            p.handle_key(&KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            PopupOutcome::Request(WsRequest::Close(1))
        );
        assert_eq!(p.handle_key(&KeyEvent::from(KeyCode::Esc)), PopupOutcome::Close);
    }

    #[test]
    fn a_typed_folder_path_is_offered_for_adding() {
        let dir = tempfile::tempdir().unwrap();
        let new = dir.path().join("newproj");
        std::fs::create_dir(&new).unwrap();
        let new = new.canonicalize().unwrap();
        let mut p = ProjectsPopup::new(bar(dir.path()));
        typed(&mut p, &new.display().to_string());
        assert_eq!(p.entries[0], Entry::Add(new.clone()));
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Enter)),
            PopupOutcome::Request(WsRequest::Open(new))
        );
        // Not a folder: nothing to add.
        let mut p = ProjectsPopup::new(bar(dir.path()));
        typed(&mut p, "/definitely/not/here");
        assert!(p.entries.is_empty());
        assert_eq!(p.handle_key(&KeyEvent::from(KeyCode::Enter)), PopupOutcome::Handled);
    }

    #[test]
    fn typing_part_of_a_path_lists_matching_folders() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        for d in ["project-one", "Project-two", "other-project", "zeta", ".hidden"] {
            std::fs::create_dir(base.join(d)).unwrap();
        }
        std::fs::write(base.join("project-file.txt"), "").unwrap();
        let mut p = ProjectsPopup::new(ProjectsBar::default());
        typed(&mut p, &format!("{}/proj", base.display()));
        // Folders only; names starting with "proj" first (any case), then containing it.
        assert_eq!(
            p.entries,
            vec![
                Entry::Add(base.join("project-one")),
                Entry::Add(base.join("Project-two")),
                Entry::Add(base.join("other-project")),
            ]
        );
        // A trailing slash offers that folder, then everything in it (not hidden folders).
        let mut p = ProjectsPopup::new(ProjectsBar::default());
        typed(&mut p, &format!("{}/", base.display()));
        assert_eq!(p.entries.len(), 5);
        assert_eq!(p.entries[0], Entry::Add(base.clone()));
        // Tab completes the selected folder; Enter then opens it, not its first sub-folder.
        std::fs::create_dir(base.join("zeta/inner")).unwrap();
        let mut p = ProjectsPopup::new(ProjectsBar::default());
        typed(&mut p, &format!("{}/ze", base.display()));
        p.handle_key(&KeyEvent::from(KeyCode::Tab));
        assert!(p.query.ends_with("zeta/"), "{}", p.query);
        assert_eq!(
            p.entries,
            vec![Entry::Add(base.join("zeta")), Entry::Add(base.join("zeta/inner"))]
        );
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Enter)),
            PopupOutcome::Request(WsRequest::Open(base.join("zeta")))
        );
        p.handle_key(&KeyEvent::from(KeyCode::Down));
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Enter)),
            PopupOutcome::Request(WsRequest::Open(base.join("zeta/inner")))
        );
        // A completed folder without sub-folders can still be opened.
        let mut p = ProjectsPopup::new(ProjectsBar::default());
        typed(&mut p, &format!("{}/project-one/", base.display()));
        assert_eq!(p.entries, vec![Entry::Add(base.join("project-one"))]);
    }

    #[test]
    fn the_list_scrolls_to_the_selection_and_clicks_follow() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        for i in 0..10 {
            std::fs::create_dir(base.join(format!("d{i}"))).unwrap();
        }
        let mut p = ProjectsPopup::new(ProjectsBar::default());
        typed(&mut p, &format!("{}/d", base.display()));
        assert_eq!(p.entries.len(), 10);
        for _ in 0..7 {
            p.handle_key(&KeyEvent::from(KeyCode::Down));
        }
        p.keep_visible(3);
        assert_eq!(p.scroll, 5, "d7 is the last of three rows");
        // Row 0 on screen is d5 now.
        assert_eq!(p.click(0), PopupOutcome::Request(WsRequest::Open(base.join("d5"))));
        for _ in 0..7 {
            p.handle_key(&KeyEvent::from(KeyCode::Up));
        }
        p.keep_visible(3);
        assert_eq!(p.scroll, 0);
    }

    #[test]
    fn a_name_finds_folders_next_to_known_projects() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        for d in ["alpha", "lms-context", "lms-other"] {
            std::fs::create_dir(base.join(d)).unwrap();
        }
        let bar = ProjectsBar {
            open: vec![("alpha".into(), base.join("alpha"))],
            active: 0,
            recent: vec![],
            ..ProjectsBar::default()
        };
        let mut p = ProjectsPopup::new(bar);
        typed(&mut p, "lms");
        assert_eq!(
            p.entries,
            vec![Entry::Add(base.join("lms-context")), Entry::Add(base.join("lms-other"))]
        );
        // The open project itself isn't offered again as a folder to add.
        let mut p = ProjectsPopup::new(ProjectsBar {
            open: vec![("alpha".into(), base.join("alpha"))],
            active: 0,
            recent: vec![],
            ..ProjectsBar::default()
        });
        typed(&mut p, "alp");
        assert_eq!(p.entries, vec![Entry::Open(0)]);
    }

    #[test]
    fn whole_words_rank_above_letters_inside_a_word() {
        assert_eq!(folder_score("lms-context", "lms"), Some(0));
        assert_eq!(folder_score("project-two", "two"), Some(1));
        assert_eq!(folder_score("my_TWO", "two"), Some(1));
        assert_eq!(folder_score("NetworkExtension", "two"), Some(2));
        assert_eq!(folder_score("alpha", "two"), None);
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        std::fs::create_dir_all(a.join("NetworkStuff")).unwrap();
        std::fs::create_dir_all(b.join("project-two")).unwrap();
        assert_eq!(
            matching_folders_in(&[&a, &b], "two"),
            vec![b.join("project-two"), a.join("NetworkStuff")]
        );
    }

    #[test]
    fn terminals_and_commands_are_offered() {
        let dir = tempfile::tempdir().unwrap();
        let bar = ProjectsBar {
            terminals: vec!["claude agents".into()],
            commands: vec!["claude agents".into(), "htop".into()],
            ..bar(dir.path())
        };
        let mut p = ProjectsPopup::new(bar.clone());
        assert!(p.entries.contains(&Entry::Terminal(0)));
        assert!(p.entries.contains(&Entry::Run("htop".into())));
        typed(&mut p, "htop");
        assert_eq!(p.entries, vec![Entry::Run("htop".into())]);
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Enter)),
            PopupOutcome::Request(WsRequest::Run("htop".into()))
        );
        // `>` alone opens the shell.
        let mut p = ProjectsPopup::new(bar.clone());
        typed(&mut p, ">");
        assert_eq!(p.entries, vec![Entry::Run(String::new())]);
        assert_eq!(
            p.handle_key(&KeyEvent::from(KeyCode::Enter)),
            PopupOutcome::Request(WsRequest::Run(String::new()))
        );
        // `>anything` runs that command.
        let mut p = ProjectsPopup::new(bar.clone());
        typed(&mut p, ">git log --oneline");
        assert_eq!(p.entries, vec![Entry::Run("git log --oneline".into())]);
        // Ctrl-W on a terminal closes it.
        let mut p = ProjectsPopup::new(bar);
        typed(&mut p, "claude");
        assert_eq!(p.entries[0], Entry::Terminal(0));
        assert_eq!(
            p.handle_key(&KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            PopupOutcome::Request(WsRequest::CloseTerminal(0))
        );
    }

    #[test]
    fn drags_only_move_the_way_the_mouse_goes() {
        assert!(drag_moves(0, 1, 31, 30), "moving right onto the next tab");
        assert!(!drag_moves(1, 0, 32, 31), "still moving right: no swap back");
        assert!(drag_moves(1, 0, 20, 21), "moving left onto the previous tab");
        assert!(!drag_moves(0, 0, 25, 24));
    }

    #[test]
    fn home_is_expanded() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_home("~/x/y"), home.join("x/y"));
        assert_eq!(expand_home("~"), home);
        assert_eq!(expand_home("/abs"), PathBuf::from("/abs"));
    }
}
