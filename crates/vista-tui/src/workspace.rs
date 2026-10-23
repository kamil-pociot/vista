//! Several open projects in one vista. Each project is an [`App`] with its
//! own tree, tabs, git, file watcher and editor; they share one event
//! channel (events are tagged with their project), the view settings and the
//! look. The active project is on screen and gets the keyboard and mouse.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ratatui::crossterm::event::{MouseButton, MouseEventKind};
use ratatui_image::picker::Picker;
use vista_core::{Config, Paths, UiState};

use crate::app::{App, AppOptions, MsgKind, SharedUi};
use crate::event::{self, AppEvent, EventRx, EventTx, INPUT, ProjectId};
use crate::projects::{ProjectsBar, TabTarget, WsRequest, project_name};
use crate::style::ThemeStyle;
use crate::term_tab::TermTab;

/// The sidebar is at least this wide when dragged…
const MIN_SIDEBAR: u16 = 16;
/// …and leaves this much for the project or terminal beside it.
const MIN_BESIDE_SIDEBAR: u16 = 40;

/// At most this many closed projects are remembered as recent.
const MAX_RECENT: usize = 20;

/// A second click on a project's × (or quit) within this time discards
/// unsaved editor changes.
const CONFIRM: Duration = Duration::from_secs(4);

pub struct Project {
    pub id: ProjectId,
    pub app: App,
    /// Its Terminal tool window's program asked for a notification while the
    /// project wasn't on screen; the tab is marked until shown.
    pub waiting: bool,
}

pub struct Workspace {
    pub projects: Vec<Project>,
    pub active: usize,
    tx: EventTx,
    pub rx: EventRx,
    next_id: ProjectId,
    config: Config,
    paths: Paths,
    theme: Option<String>,
    pub ui: SharedUi,
    ui_path: PathBuf,
    pub picker: Option<Picker>,
    /// The look the active project shows; others catch up when shown.
    look: Option<String>,
    confirm: Option<(Confirm, Instant)>,
    pub should_quit: bool,
    /// Terminal tabs (programs such as `claude agents`), after the projects.
    pub terminals: Vec<TermTab>,
    /// The terminal tab on screen (full screen), if any.
    pub terminal: Option<usize>,
    /// The project or terminal tab being dragged to a new place.
    dragging: Option<TabTarget>,
    /// The mouse column during that drag (its direction decides moves).
    drag_col: u16,
    /// Where the top bar's tabs were in the last frame drawn (the same for
    /// every project; a project switched to may not have drawn them yet).
    top_hits: Vec<crate::projects::ProjectHit>,
    /// The sidebar (Alt-B): projects, terminal tabs, Claude sessions.
    sidebar: bool,
    sessions: Vec<crate::sessions::Session>,
    /// Why the session list couldn't be read, if it couldn't.
    sessions_note: Option<String>,
    /// The session poller's events carry this id.
    sessions_id: ProjectId,
    /// The poller runs while this is set (the sidebar is shown).
    sessions_wanted: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The sidebar's rows in the last frame, for clicks.
    sidebar_hits: Vec<(ratatui::layout::Rect, crate::ui::sidebar::SidebarHit)>,
    sidebar_area: Option<ratatui::layout::Rect>,
    /// The sidebar's border is being dragged.
    sidebar_drag: bool,
    /// The window's width in the last frame (the sidebar leaves room for the rest).
    window_width: u16,
    /// Where the terminal tab on screen was drawn.
    term_body: ratatui::layout::Rect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Confirm {
    Close(PathBuf),
    Quit,
}

impl Workspace {
    pub fn new(config: Config, paths: Paths, theme: Option<String>) -> Workspace {
        let ui_path = UiState::path(&paths);
        let (ui, warning) = UiState::load(&ui_path);
        if let Some(w) = warning {
            tracing::warn!("{w}");
        }
        let (tx, rx) = event::channel(INPUT);
        let sidebar = ui.layout.sidebar.unwrap_or(config.sidebar.show);
        let sessions_id = INPUT + 1;
        let sessions_wanted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(sidebar));
        if !config.sidebar.sessions.trim().is_empty() {
            crate::sessions::spawn_poller(
                config.sidebar.sessions.clone(),
                sessions_wanted.clone(),
                tx.for_project(sessions_id),
            );
        }
        Workspace {
            projects: Vec::new(),
            active: 0,
            tx,
            rx,
            next_id: sessions_id + 1,
            config,
            paths,
            theme,
            ui: std::rc::Rc::new(std::cell::RefCell::new(ui)),
            ui_path,
            picker: None,
            look: None,
            confirm: None,
            should_quit: false,
            terminals: Vec::new(),
            terminal: None,
            dragging: None,
            drag_col: 0,
            top_hits: Vec::new(),
            sidebar,
            sessions: Vec::new(),
            sessions_note: None,
            sessions_id,
            sessions_wanted,
            sidebar_hits: Vec::new(),
            sidebar_area: None,
            sidebar_drag: false,
            window_width: 0,
            term_body: ratatui::layout::Rect::default(),
        }
    }

    /// For the terminal-input thread: events go to the active project.
    pub fn input_tx(&self) -> EventTx {
        self.tx.for_project(INPUT)
    }

    pub fn active_app(&mut self) -> &mut App {
        &mut self.projects[self.active].app
    }

    /// Opens a project (or finds it if already open); returns its index.
    pub fn open(&mut self, root: &Path) -> Result<usize> {
        let root = root
            .canonicalize()
            .with_context(|| format!("cannot open {}", root.display()))?;
        if let Some(i) = self.projects.iter().position(|p| p.app.root == root) {
            return Ok(i);
        }
        let id = self.next_id;
        self.next_id += 1;
        let mut app = App::new(AppOptions {
            root,
            config: self.config.clone(),
            paths: self.paths.clone(),
            theme: self.theme.clone(),
            events: Some(self.tx.for_project(id)),
            ui: Some(self.ui.clone()),
        })?;
        // Image support found for the terminal (once it was probed); until
        // then the project keeps its configured default.
        if self.picker.is_some() {
            app.picker = self.picker.clone();
        }
        self.projects.push(Project {
            id,
            app,
            waiting: false,
        });
        let i = self.projects.len() - 1;
        self.refresh_bars();
        Ok(i)
    }

    /// Reopens the projects that were open last time (skipping any that
    /// are gone).
    pub fn restore(&mut self) {
        let saved = self.ui.borrow().projects.clone();
        for root in &saved.open {
            if root.is_dir()
                && let Err(e) = self.open(root)
            {
                tracing::warn!("not reopening {}: {e:#}", root.display());
            }
        }
        if let Some(i) = saved
            .active
            .and_then(|a| self.projects.iter().position(|p| p.app.root == a))
        {
            self.active = i;
        }
    }

    /// Shows project `i`: starts it if this is its first time on screen and
    /// brings it up to date with the shared settings and the look.
    pub fn activate(&mut self, i: usize) {
        if i >= self.projects.len() {
            return;
        }
        self.active = i;
        self.terminal = None;
        let look = self.look.clone();
        let ui = self.ui.borrow().clone();
        let app = &mut self.projects[i].app;
        if let Some(look) = look
            && app.theme.id != look
        {
            let _ = app.set_theme(&look);
        }
        let show = ui.tree.show_ignored.unwrap_or(app.config.tree.show_ignored);
        if app.project.show_ignored != show {
            app.project.show_ignored = show;
            app.project.rebuild(&app.status);
        }
        let by_dir = ui.commit.group_by_folder.unwrap_or(false);
        if app.commit.by_dir != by_dir {
            app.commit.by_dir = by_dir;
            let root = app.root.clone();
            app.commit.rebuild(&app.status, &root);
        }
        if !app.started {
            app.started = true;
            app.on_start();
        }
        self.refresh_bars();
        self.persist();
    }

    /// Every project's view of the tab row.
    fn refresh_bars(&mut self) {
        let open: Vec<(String, PathBuf)> = self
            .projects
            .iter()
            .map(|p| (project_name(&p.app.root), p.app.root.clone()))
            .collect();
        let recent: Vec<PathBuf> = self
            .ui
            .borrow()
            .projects
            .recent
            .iter()
            .filter(|r| !open.iter().any(|(_, o)| o == *r))
            .cloned()
            .collect();
        let bar = ProjectsBar {
            open,
            active: self.active,
            recent,
            terminals: self.terminals.iter().map(|t| t.title.clone()).collect(),
            terminal: self.terminal,
            sidebar: self.sidebar,
            waiting: (self.projects.iter().enumerate())
                .filter(|(_, p)| p.waiting)
                .map(|(i, _)| TabTarget::Project(i))
                .chain(
                    (self.terminals.iter().enumerate())
                        .filter(|(_, t)| t.waiting)
                        .map(|(i, _)| TabTarget::Terminal(i)),
                )
                .collect(),
            commands: self.config.terminal.commands.clone(),
        };
        for p in &mut self.projects {
            p.app.projects = bar.clone();
        }
    }

    /// Saves which projects are open (and the recent ones).
    fn persist(&mut self) {
        let open: Vec<PathBuf> = self.projects.iter().map(|p| p.app.root.clone()).collect();
        let active = open.get(self.active).cloned();
        let result = {
            let mut ui = self.ui.borrow_mut();
            let before = ui.clone();
            let mut recent = open.clone();
            recent.extend(ui.projects.recent.iter().cloned());
            let mut seen = std::collections::HashSet::new();
            recent.retain(|r| seen.insert(r.clone()));
            recent.truncate(MAX_RECENT);
            ui.projects.open = open;
            ui.projects.active = active;
            ui.projects.recent = recent;
            (*ui != before).then(|| ui.save(&self.ui_path))
        };
        if let Some(Err(e)) = result {
            self.active_app()
                .notify(format!("cannot save settings: {e}"), MsgKind::Error);
        }
    }

    /// Delivers one event: terminal input to the active project, the rest
    /// to the project that asked for it. Then acts on what it requested.
    pub fn handle(&mut self, id: ProjectId, ev: AppEvent) {
        if id == self.sessions_id {
            if let AppEvent::Sessions(result) = ev {
                match result {
                    Ok(list) => {
                        self.sessions = list;
                        self.sessions_note = None;
                    }
                    Err(e) => self.sessions_note = Some(e),
                }
            }
            return;
        }
        if id == INPUT
            && let AppEvent::Term(ratatui::crossterm::event::Event::Mouse(m)) = &ev
            && let Some(area) = self.sidebar_area
        {
            // Its right border resizes it; a drag goes on outside it.
            let on_border = m.column + 1 == area.x + area.width && m.row >= area.y;
            if self.sidebar_drag || (on_border && matches!(m.kind, MouseEventKind::Down(MouseButton::Left))) {
                self.sidebar_resize(m);
                return;
            }
            if area.contains(ratatui::layout::Position::new(m.column, m.row)) {
                self.sidebar_mouse(m);
                return;
            }
        }
        if let Some(ti) = self.terminals.iter().position(|t| t.id == id) {
            match ev {
                // Drawn after this; the reader may wake us again.
                AppEvent::TermOutput(pending) => pending.store(false, std::sync::atomic::Ordering::SeqCst),
                AppEvent::TermExited => self.terminals[ti].mark_exited(),
                AppEvent::TermNotify(n) => {
                    crate::terminal::notify(&n, &self.terminals[ti].title, &self.config.terminal);
                    if self.terminal != Some(ti) {
                        self.terminals[ti].waiting = true;
                        self.refresh_bars();
                    }
                }
                _ => {}
            }
            return;
        }
        if id == INPUT
            && let AppEvent::Term(ratatui::crossterm::event::Event::Mouse(m)) = &ev
            && self.drag_tab(m)
        {
            return;
        }
        if id == INPUT
            && let Some(ti) = self.terminal
        {
            self.terminal_input(ti, ev);
            return;
        }
        let i = if id == INPUT {
            Some(self.active)
        } else {
            self.projects.iter().position(|p| p.id == id)
        };
        // Events of a project closed meanwhile are dropped.
        let Some(i) = i else { return };
        if matches!(ev, AppEvent::TermNotify(_)) && (i != self.active || self.terminal.is_some()) {
            self.projects[i].waiting = true;
            self.refresh_bars();
        }
        self.projects[i].app.handle_event(ev);
        if i == self.active {
            self.look = Some(self.projects[i].app.theme.id.clone());
        }
        if let Some(req) = self.projects[i].app.ws_request.take() {
            self.request(req);
        }
        if self.projects.get(self.active).is_some_and(|p| p.app.should_quit) {
            self.quit_requested();
        }
    }

    /// Dragging a project tab (or a terminal tab) in the top bar reorders
    /// them. Returns true when the event was part of a drag (and is used up).
    fn drag_tab(&mut self, m: &ratatui::crossterm::event::MouseEvent) -> bool {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        let hit = |ws: &Self| {
            if m.row != 0 {
                return None;
            }
            ws.top_hits.iter().find(|h| (h.x.0..h.x.1).contains(&m.column)).copied()
        };
        match (m.kind, self.dragging) {
            (MouseEventKind::Down(MouseButton::Left), _) => {
                // Starts on a tab (not its ×); the click still switches.
                self.dragging = hit(self).and_then(|h| match (h.target, h.request(m.column)) {
                    (t @ TabTarget::Project(_), Some(WsRequest::Switch(_))) => Some(t),
                    (t @ TabTarget::Terminal(_), Some(WsRequest::ShowTerminal(_))) => Some(t),
                    _ => None,
                });
                self.drag_col = m.column;
                false
            }
            (MouseEventKind::Drag(MouseButton::Left), Some(from)) => {
                // Projects move among projects, terminals among terminals,
                // and only the way the mouse is going.
                let moves = |a, b| crate::projects::drag_moves(a, b, m.column, self.drag_col);
                match (from, hit(self).map(|h| h.target)) {
                    (TabTarget::Project(a), Some(TabTarget::Project(b))) if moves(a, b) => {
                        self.move_project(a, b);
                        self.dragging = Some(TabTarget::Project(b));
                    }
                    (TabTarget::Terminal(a), Some(TabTarget::Terminal(b))) if moves(a, b) => {
                        self.move_terminal(a, b);
                        self.dragging = Some(TabTarget::Terminal(b));
                    }
                    _ => {}
                }
                self.drag_col = m.column;
                true
            }
            (MouseEventKind::Up(_), Some(_)) => {
                self.dragging = None;
                true
            }
            _ => false,
        }
    }

    /// Moves terminal tab `from` to position `to`.
    pub fn move_terminal(&mut self, from: usize, to: usize) {
        let n = self.terminals.len();
        if from >= n || to >= n || from == to {
            return;
        }
        let t = self.terminals.remove(from);
        self.terminals.insert(to, t);
        self.terminal = self.terminal.map(|a| {
            if a == from {
                to
            } else if from < a && a <= to {
                a - 1
            } else if to <= a && a < from {
                a + 1
            } else {
                a
            }
        });
        self.refresh_bars();
    }

    /// Moves open project `from` to tab position `to` (remembered).
    pub fn move_project(&mut self, from: usize, to: usize) {
        let n = self.projects.len();
        if from >= n || to >= n || from == to {
            return;
        }
        let p = self.projects.remove(from);
        self.projects.insert(to, p);
        let a = self.active;
        self.active = if a == from {
            to
        } else if from < a && a <= to {
            a - 1
        } else if to <= a && a < from {
            a + 1
        } else {
            a
        };
        self.refresh_bars();
        self.persist();
    }

    /// Handles everything queued.
    pub fn drain(&mut self) {
        while let Ok((id, ev)) = self.rx.try_recv() {
            self.handle(id, ev);
        }
        if let Some(p) = self.projects.get_mut(self.active) {
            p.app.flush_pending();
        }
    }

    fn request(&mut self, req: WsRequest) {
        match req {
            WsRequest::Switch(i) => self.activate(i),
            WsRequest::Cycle(forward) => {
                let n = self.projects.len();
                if n > 1 {
                    let i = if forward {
                        (self.active + 1) % n
                    } else {
                        (self.active + n - 1) % n
                    };
                    self.activate(i);
                } else {
                    self.active_app()
                        .notify("Only one project open (Ctrl-O to add one)", MsgKind::Info);
                }
            }
            WsRequest::Open(root) => match self.open(&root) {
                Ok(i) => self.activate(i),
                Err(e) => self.active_app().notify(format!("{e:#}"), MsgKind::Error),
            },
            WsRequest::Close(i) => self.close(i),
            WsRequest::ShowTerminal(i) => {
                if i < self.terminals.len() {
                    self.terminal = Some(i);
                    self.refresh_bars();
                }
            }
            WsRequest::CloseTerminal(i) => self.close_terminal(i),
            WsRequest::ToggleSidebar => {
                self.sidebar = !self.sidebar;
                self.sessions_wanted
                    .store(self.sidebar, std::sync::atomic::Ordering::SeqCst);
                let on = self.sidebar;
                let result = {
                    let mut ui = self.ui.borrow_mut();
                    ui.layout.sidebar = Some(on);
                    ui.save(&self.ui_path)
                };
                if let Err(e) = result {
                    self.active_app()
                        .notify(format!("cannot save settings: {e}"), MsgKind::Error);
                }
                self.refresh_bars();
            }
            WsRequest::Run(cmd) => self.run_terminal(&cmd),
        }
    }

    /// Runs `command` in a new terminal tab, in the active project's folder.
    pub fn run_terminal(&mut self, command: &str) {
        let id = self.next_id;
        self.next_id += 1;
        let app = &self.projects[self.active].app;
        let cwd = app.root.clone();
        // The full window below the top bar (resized to fit when drawn).
        let size = (app.areas.top.width.max(20), app.areas.bottom.y.max(10));
        match TermTab::spawn(id, command, &cwd, size, self.tx.for_project(id)) {
            Ok(t) => {
                self.terminals.push(t);
                self.terminal = Some(self.terminals.len() - 1);
                self.refresh_bars();
            }
            Err(e) => self.active_app().notify(format!("{e:#}"), MsgKind::Error),
        }
    }

    /// Closes terminal tab `i` (stopping its program).
    pub fn close_terminal(&mut self, i: usize) {
        if i >= self.terminals.len() {
            return;
        }
        self.terminals.remove(i);
        self.terminal = match self.terminal {
            Some(j) if j == i => None,
            Some(j) if j > i => Some(j - 1),
            other => other,
        };
        self.refresh_bars();
    }

    /// Input while a terminal tab is on screen: everything goes to the
    /// program except Ctrl-\ (back to the project) and the top bar's tabs.
    fn terminal_input(&mut self, ti: usize, ev: AppEvent) {
        use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
        match ev {
            AppEvent::Term(Event::Key(key)) if key.kind != KeyEventKind::Release => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                // Legacy terminals report Ctrl-\ as Ctrl-4.
                if ctrl && matches!(key.code, KeyCode::Char('\\') | KeyCode::Char('4')) {
                    self.terminal = None;
                    self.refresh_bars();
                } else if self.terminals[ti].exited.is_some() && key.code == KeyCode::Enter {
                    self.close_terminal(ti);
                } else {
                    self.terminals[ti].key(&key);
                }
            }
            AppEvent::Term(Event::Paste(text)) => self.terminals[ti].paste(&text),
            AppEvent::Term(Event::Mouse(m)) => {
                if m.row == 0 {
                    if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                        let hit = self.projects[self.active]
                            .app
                            .project_hits
                            .iter()
                            .find(|h| (h.x.0..h.x.1).contains(&m.column))
                            .copied();
                        match hit.map(|h| (h.target, h.request(m.column))) {
                            Some((_, Some(req))) => self.request(req),
                            Some((TabTarget::Plus, None)) => {
                                self.terminal = None;
                                self.refresh_bars();
                                self.active_app().handle_action(crate::keymap::Action::Projects);
                            }
                            _ => {}
                        }
                    }
                } else {
                    let body = self.term_body;
                    if body.contains(ratatui::layout::Position::new(m.column, m.row)) {
                        self.terminals[ti].mouse(&m, m.column - body.x, m.row - body.y);
                    }
                }
            }
            _ => {}
        }
    }

    /// Draws the screen: the sidebar when shown, then a terminal tab or the
    /// active project in the rest.
    pub fn draw(&mut self, f: &mut ratatui::Frame) {
        self.unmark_shown();
        let mut area = f.area();
        self.window_width = area.width;
        self.sidebar_area = None;
        self.sidebar_hits.clear();
        let width = self.sidebar_width();
        if self.sidebar && area.width >= width + MIN_BESIDE_SIDEBAR {
            let side = ratatui::layout::Rect { width, ..area };
            self.draw_sidebar(f, side);
            self.sidebar_area = Some(side);
            area.x += width;
            area.width -= width;
        }
        let Some(ti) = self.terminal.filter(|t| *t < self.terminals.len()) else {
            crate::ui::draw(f, area, self.active_app());
            self.top_hits = self.active_app().project_hits.clone();
            return;
        };
        let app = &mut self.projects[self.active].app;
        f.render_widget(ratatui::widgets::Block::new().style(app.theme.base()), area);
        let top = ratatui::layout::Rect { height: 1, ..area };
        crate::ui::top_bar(f, top, app);
        let body = ratatui::layout::Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };
        self.top_hits = app.project_hits.clone();
        self.term_body = body;
        let tab = &mut self.terminals[ti];
        tab.resize(body.width, body.height);
        crate::ui::term::draw(
            f,
            body,
            tab,
            &app.theme,
            true,
            "Enter closes the tab, Ctrl-\\ goes back",
        );
    }

    fn draw_sidebar(&mut self, f: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        use crate::ui::sidebar::{Item, View};
        let on_project = self.terminal.is_none();
        let names: Vec<String> = self.projects.iter().map(|p| project_name(&p.app.root)).collect();
        let projects = (self.projects.iter().enumerate())
            .map(|(i, p)| Item {
                name: &names[i],
                active: on_project && i == self.active,
                waiting: p.waiting,
                // Known once the project has loaded its status.
                branch: Some(&p.app.status.branch)
                    .filter(|b| p.app.git.is_some() && (b.head.is_some() || b.oid.is_some())),
            })
            .collect();
        let terminals = (self.terminals.iter().enumerate())
            .filter(|(_, t)| t.session.is_none())
            .map(|(i, t)| {
                (
                    i,
                    Item {
                        name: &t.title,
                        active: self.terminal == Some(i),
                        waiting: t.waiting,
                        branch: None,
                    },
                )
            })
            .collect();
        let shown_session = self
            .terminal
            .and_then(|i| self.terminals.get(i))
            .and_then(|t| t.session.as_deref());
        let view = View {
            projects,
            terminals,
            sessions: &self.sessions,
            shown_session,
            note: self.sessions_note.as_deref(),
        };
        let theme = self.projects[self.active].app.theme.clone();
        self.sidebar_hits = crate::ui::sidebar::draw(f, area, &view, &theme);
    }

    /// The sidebar's width: dragged (remembered), else configured.
    fn sidebar_width(&self) -> u16 {
        let w = self.ui.borrow().layout.sidebar_width;
        w.unwrap_or(self.config.sidebar.width).max(MIN_SIDEBAR)
    }

    /// Dragging the sidebar's right border; the width is saved on release.
    fn sidebar_resize(&mut self, m: &ratatui::crossterm::event::MouseEvent) {
        let Some(area) = self.sidebar_area else { return };
        let max = self.window_width.saturating_sub(MIN_BESIDE_SIDEBAR).max(MIN_SIDEBAR);
        let width = (m.column.saturating_sub(area.x) + 1).clamp(MIN_SIDEBAR, max);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => self.sidebar_drag = true,
            MouseEventKind::Drag(MouseButton::Left) => self.ui.borrow_mut().layout.sidebar_width = Some(width),
            MouseEventKind::Up(_) => {
                self.sidebar_drag = false;
                self.ui.borrow_mut().layout.sidebar_width = Some(width);
                let result = self.ui.borrow().save(&self.ui_path);
                if let Err(e) = result {
                    self.active_app()
                        .notify(format!("cannot save settings: {e}"), MsgKind::Error);
                }
            }
            _ => {}
        }
    }

    /// A click in the sidebar opens what's on that row.
    fn sidebar_mouse(&mut self, m: &ratatui::crossterm::event::MouseEvent) {
        use crate::ui::sidebar::SidebarHit;
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        if !matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let pos = ratatui::layout::Position::new(m.column, m.row);
        let Some(hit) = self.sidebar_hits.iter().find(|(r, _)| r.contains(pos)).map(|(_, h)| *h) else {
            return;
        };
        match hit {
            SidebarHit::Project(i) => self.activate(i),
            SidebarHit::Terminal(i) => self.request(WsRequest::ShowTerminal(i)),
            SidebarHit::NewProject => {
                self.terminal = None;
                self.refresh_bars();
                self.active_app().handle_action(crate::keymap::Action::Projects);
            }
            SidebarHit::Session(k) => {
                if let Some(s) = self.sessions.get(k).cloned() {
                    self.open_session(&s);
                }
            }
        }
    }

    /// Shows session `s`: its tab if it's open, else a new tab attached to it.
    fn open_session(&mut self, s: &crate::sessions::Session) {
        if let Some(i) = self.terminals.iter().position(|t| t.session.as_deref() == Some(&s.id)) {
            self.request(WsRequest::ShowTerminal(i));
            return;
        }
        let command = self.config.sidebar.attach.replace("{id}", &s.id);
        let before = self.terminals.len();
        self.run_terminal(&command);
        if self.terminals.len() > before
            && let Some(t) = self.terminals.last_mut()
        {
            t.session = Some(s.id.clone());
            t.title = s.name.clone();
            self.refresh_bars();
        }
    }

    /// The tab on screen no longer needs attention.
    fn unmark_shown(&mut self) {
        let flag = match self.terminal {
            Some(ti) => self.terminals.get_mut(ti).map(|t| &mut t.waiting),
            None => self.projects.get_mut(self.active).map(|p| &mut p.waiting),
        };
        if let Some(flag) = flag
            && std::mem::take(flag)
        {
            self.refresh_bars();
        }
    }

    /// The window title: the active project or terminal.
    pub fn title(&mut self) -> String {
        match self.terminal.and_then(|t| self.terminals.get(t)) {
            Some(t) => format!("vista — {}", t.title),
            None => format!("vista — {}", self.active_app().project_name),
        }
    }

    /// Closes project `i` (not the last one). Unsaved editor changes need
    /// a second request to discard.
    pub fn close(&mut self, i: usize) {
        if i >= self.projects.len() {
            return;
        }
        if self.projects.len() == 1 {
            self.active_app()
                .notify("This is the only open project (q quits)", MsgKind::Info);
            return;
        }
        let root = self.projects[i].app.root.clone();
        let unsaved = unsaved(&self.projects[i].app);
        let confirmed = matches!(&self.confirm, Some((Confirm::Close(r), at)) if *r == root && at.elapsed() < CONFIRM);
        if unsaved > 0 && !confirmed {
            self.confirm = Some((Confirm::Close(root.clone()), Instant::now()));
            let name = project_name(&root);
            self.active_app().notify(
                format!("{name} has {unsaved} file(s) with unsaved changes; close again to discard"),
                MsgKind::Error,
            );
            return;
        }
        self.confirm = None;
        let mut p = self.projects.remove(i);
        p.app.on_exit();
        if self.active > i || self.active >= self.projects.len() {
            self.active = self.active.saturating_sub(1);
        }
        let active = self.active;
        self.activate(active);
    }

    /// The active project wants to quit (it already checked its own
    /// editor); other projects' unsaved changes need a second q.
    fn quit_requested(&mut self) {
        let unsaved: usize = self
            .projects
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != self.active)
            .map(|(_, p)| unsaved(&p.app))
            .sum();
        let confirmed = matches!(&self.confirm, Some((Confirm::Quit, at)) if at.elapsed() < CONFIRM);
        let running = self.terminals.iter().filter(|t| t.exited.is_none()).count();
        if running > 0 && !confirmed {
            self.confirm = Some((Confirm::Quit, Instant::now()));
            let app = self.active_app();
            app.should_quit = false;
            app.notify(
                format!("{running} terminal tab(s) still running; press q again to stop them and quit"),
                MsgKind::Error,
            );
            return;
        }
        if unsaved > 0 && !confirmed {
            self.confirm = Some((Confirm::Quit, Instant::now()));
            let app = self.active_app();
            app.should_quit = false;
            app.notify(
                format!("{unsaved} file(s) with unsaved changes in other projects; press q again to discard"),
                MsgKind::Error,
            );
            return;
        }
        self.should_quit = true;
    }

    /// Gives every project the terminal's image support.
    pub fn set_picker(&mut self, picker: Option<Picker>) {
        for p in &mut self.projects {
            p.app.picker = picker.clone();
        }
        self.picker = picker;
    }

    pub fn on_exit(&mut self) {
        self.persist();
        self.terminals.clear();
        for p in &mut self.projects {
            p.app.on_exit();
        }
    }
}

/// Files with unsaved changes in a project's editor.
fn unsaved(app: &App) -> usize {
    app.nvim
        .as_ref()
        .and_then(|n| n.modified_buffers().ok())
        .map(|b| b.len())
        .unwrap_or(0)
}
