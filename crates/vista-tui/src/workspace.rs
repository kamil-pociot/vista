//! Several open projects in one vista. Each project is an [`App`] with its
//! own tree, tabs, git, file watcher and editor; they share one event
//! channel (events are tagged with their project), the view settings and the
//! look. The active project is on screen and gets the keyboard and mouse.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ratatui_image::picker::Picker;
use vista_core::{Config, Paths, UiState};

use crate::app::{App, AppOptions, MsgKind, SharedUi};
use crate::event::{self, AppEvent, EventRx, EventTx, INPUT, ProjectId};
use crate::projects::{ProjectsBar, WsRequest, project_name};

/// At most this many closed projects are remembered as recent.
const MAX_RECENT: usize = 20;

/// A second click on a project's × (or quit) within this time discards
/// unsaved editor changes.
const CONFIRM: Duration = Duration::from_secs(4);

pub struct Project {
    pub id: ProjectId,
    pub app: App,
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
        Workspace {
            projects: Vec::new(),
            active: 0,
            tx,
            rx,
            next_id: INPUT + 1,
            config,
            paths,
            theme,
            ui: std::rc::Rc::new(std::cell::RefCell::new(ui)),
            ui_path,
            picker: None,
            look: None,
            confirm: None,
            should_quit: false,
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
        self.projects.push(Project { id, app });
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
        let i = if id == INPUT {
            Some(self.active)
        } else {
            self.projects.iter().position(|p| p.id == id)
        };
        // Events of a project closed meanwhile are dropped.
        let Some(i) = i else { return };
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
