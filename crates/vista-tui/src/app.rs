//! Application state and event handling. Drawing lives in [`crate::ui`].

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use vista_core::{Config, FsChange, Paths, Watcher};
use vista_git::{Git, StatusSnapshot};
use vista_theme::{Theme, ThemeLoader};

use crate::commit::{CommitOutcome, CommitView};
use crate::editor;
use crate::event::{AppEvent, EventRx, EventTx};
use crate::find::{FindOutcome, FindPopup, FindWindow, ResultTab};
use crate::goto::{GotoOutcome, GotoPopup, GotoTab};
use crate::keymap::{Action, Keymap};
use crate::project::{Outcome, ProjectView};
use crate::recent::{PopupOutcome, Prompt, Recent, RecentPopup};
use crate::viewer::{Content, ImageState, LoadCtx, MdImages, Viewer, ViewerOutcome};
use ratatui_image::picker::Picker;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use vista_core::ImageProtocol;
use vista_git::Change;
use vista_git::write::{CommitFile, CommitResult};
use vista_nvim::{Grid, Nvim, NvimEvent};
use vista_search::FileIndex;

/// `theme = "herdr"`: use herdr's theme.
pub const HERDR: &str = "herdr";

/// Which panel has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The left tool window (Project or Commit).
    Left,
    /// The main area (viewer / editor / diff).
    Main,
    /// The bottom tool window (Find results).
    Bottom,
}

/// Which tool window occupies the left side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftPanel {
    Project,
    Commit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgKind {
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct StatusMsg {
    pub text: String,
    pub kind: MsgKind,
    pub at: Instant,
}

/// What a confirmed dialog does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    Rollback(Vec<Change>),
}

/// A yes/no question shown over everything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub text: String,
    pub action: ConfirmAction,
}

pub struct AppOptions {
    pub root: PathBuf,
    pub config: Config,
    pub paths: Paths,
    /// `--theme` on the command line wins over the config file.
    pub theme: Option<String>,
    /// The workspace's shared event channel; `None` makes a private one.
    pub events: Option<EventTx>,
    /// Settings shared by all open projects; `None` loads `state.toml`.
    pub ui: Option<SharedUi>,
}

/// View settings shared by every open project (one `state.toml`).
pub type SharedUi = Rc<RefCell<vista_core::UiState>>;

/// Background `git status` runs: at most one at a time; requests that arrive
/// meanwhile are coalesced into one follow-up run.
#[derive(Debug, Default)]
struct StatusJob {
    running: bool,
    pending: bool,
}

/// The popup offered at the last (first) change of a file: pressing the
/// same key again moves to the next (previous) changed file.
#[derive(Debug, Clone)]
pub struct ChangeHop {
    pub forward: bool,
    pub at: Instant,
    pub text: String,
    /// A second press goes on to another file; `false` when this is the
    /// end (no further file in that direction).
    pub armed: bool,
}

/// How long the "press again" offer stays.
pub const HOP_TTL: Duration = Duration::from_secs(4);

/// Two clicks on the same row within this time open it.
const DOUBLE_CLICK: Duration = Duration::from_millis(500);

/// Lists that open their row on a double click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClickList {
    Commit,
    FindPopup,
    FindWindow,
    Goto,
    LogFiles,
    Viewer,
}

/// The active diff tab, as the side-by-side editor opens it.
struct DiffEdit {
    path: PathBuf,
    head: String,
    label: String,
    view: vista_nvim::DiffView,
}

impl DiffEdit {
    fn open_in(&self, n: &vista_nvim::Nvim) -> Result<(), vista_nvim::RpcError> {
        n.open_diff(&self.path, &self.head, &self.label, self.view)
    }
}

pub struct App {
    pub root: PathBuf,
    pub project_name: String,
    pub config: Config,
    pub paths: Paths,
    pub theme: Theme,
    /// `--theme` / `VISTA_THEME` chose the look: it doesn't follow herdr.
    theme_forced: bool,
    /// The look's "terminal" colours are filled in (the Fill background preference).
    pub theme_filled: bool,
    theme_loader: ThemeLoader,
    theme_stamp: Option<SystemTime>,
    pub keymap: Keymap,
    pub focus: Focus,
    pub left: Option<LeftPanel>,
    pub show_help: bool,
    pub should_quit: bool,
    /// Transient message in the status bar.
    pub message: Option<StatusMsg>,
    pub tx: EventTx,
    /// Only for a project running on its own (tests); in a workspace the
    /// workspace receives.
    pub rx: Option<EventRx>,
    /// Screen regions from the last draw (mouse hit-testing).
    pub areas: crate::ui::Areas,
    /// Clickable labels in the last frame (on top of `areas`).
    pub buttons: Vec<crate::ui::click::Button>,
    /// The Preferences page, when open (Ctrl-,).
    pub prefs: Option<crate::prefs::PrefsPage>,
    pub project: ProjectView,
    /// The repository, if the project is inside one.
    pub git: Option<Git>,
    pub status: StatusSnapshot,
    status_job: StatusJob,
    watcher: Option<Watcher>,
    pub viewer: Viewer,
    /// Image encoder for the terminal; `None` when images are off.
    pub picker: Option<Picker>,
    pub md_images: MdImages,
    pub recent: Recent,
    pub recent_popup: Option<RecentPopup>,
    pub prompt: Option<Prompt>,
    /// The embedded editor, started on first use.
    pub nvim: Option<Nvim>,
    pub grid: Grid,
    /// Files with unsaved changes in the editor.
    pub dirty: HashSet<PathBuf>,
    quit_confirm: Option<Instant>,
    /// Armed "press again for the next file" step (next/previous change).
    pub change_hop: Option<ChangeHop>,
    /// View settings remembered between runs (`state.toml`), shared by all
    /// open projects.
    pub ui: SharedUi,
    /// The open projects (tab row in the top bar), kept current by the workspace.
    pub projects: crate::projects::ProjectsBar,
    /// What this project asks of the workspace (switch, open, close, …).
    pub ws_request: Option<crate::projects::WsRequest>,
    /// `on_start` ran: a project starts its watcher and git work when first shown.
    pub started: bool,
    pub projects_popup: Option<crate::projects::ProjectsPopup>,
    /// Where the project tabs (and +) were drawn in the top bar.
    pub project_hits: Vec<crate::projects::ProjectHit>,
    /// The divider between the tree and the editor is being dragged.
    dragging_divider: bool,
    /// The editor tab being dragged to a new place.
    dragging_tab: Option<usize>,
    /// The mouse column during a tab drag (its direction decides moves).
    drag_col: u16,
    /// Text is being selected with the mouse in the viewer.
    selecting: bool,
    /// The Git tool window (log, file history).
    pub log: crate::log::LogWindow,
    /// The Terminal tool window's shell (started on first use).
    pub shell: Option<crate::term_tab::TermTab>,
    pub shell_visible: bool,
    ui_path: PathBuf,
    /// Where each tab and its × were drawn (last frame), for clicks.
    pub tab_hits: Vec<crate::ui::chrome::TabHit>,
    /// Last left click: (list, row, when), to spot double clicks.
    last_click: Option<(ClickList, usize, Instant)>,
    /// A command to run with the terminal handed over (`$EDITOR` fallback).
    pending_external: Option<(String, PathBuf)>,
    pub goto: Option<GotoPopup>,
    /// All project files for Go to File (built in the background).
    pub file_index: Option<Arc<FileIndex>>,
    index_job: bool,
    index_dirty: bool,
    pub find: Option<FindPopup>,
    pub find_window: FindWindow,
    pub find_history: Vec<String>,
    find_generation: u64,
    find_pending: bool,
    pub commit: CommitView,
    pub confirm: Option<Confirm>,
    last_tick: Instant,
}

/// How long transient status messages stay visible.
const STATUS_TTL: Duration = Duration::from_secs(6);

impl App {
    pub fn new(opts: AppOptions) -> Result<App> {
        let root = opts
            .root
            .canonicalize()
            .with_context(|| format!("cannot open {}", opts.root.display()))?;
        anyhow::ensure!(root.is_dir(), "{} is not a directory", root.display());
        let project_name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let theme_loader = ThemeLoader::new(Some(opts.paths.themes_dir()));
        let ui_path = vista_core::UiState::path(&opts.paths);
        let (ui, ui_warning) = match opts.ui {
            Some(ui) => (ui, None),
            None => {
                let (ui, warning) = vista_core::UiState::load(&ui_path);
                (Rc::new(RefCell::new(ui)), warning)
            }
        };
        // --theme / VISTA_THEME › the look picked last (Ctrl-T, Preferences)
        // › config. `herdr` follows herdr's theme.
        let forced = opts
            .theme
            .clone()
            .or_else(|| std::env::var("VISTA_THEME").ok().filter(|t| !t.is_empty()));
        let theme_forced = forced.is_some();
        let theme_id = forced
            .or_else(|| ui.borrow().look.clone())
            .unwrap_or_else(|| opts.config.theme.clone());
        let follows_herdr = theme_id == HERDR;
        let theme_id = if follows_herdr {
            vista_core::herdr::current_theme()
        } else {
            theme_id
        };
        let mut startup_error = ui_warning;
        let filled = ui.borrow().looks.fill_background.unwrap_or(false);
        let theme = match load_look(&theme_loader, &theme_id, filled) {
            Ok(t) => t,
            Err(e) if follows_herdr => {
                tracing::warn!("herdr's theme: {e}");
                startup_error = Some(format!("herdr's theme \"{theme_id}\" has no vista look; using darcula"));
                Theme::default_theme()
            }
            Err(e) => {
                startup_error = Some(format!("{e}; using darcula"));
                Theme::default_theme()
            }
        };
        let theme_stamp = stamp(theme_loader.user_path(&theme.id).as_deref());
        let (keymap, warnings) = Keymap::with_overrides(&opts.config.keys);
        let (tx, rx) = match opts.events {
            Some(tx) => (tx, None),
            None => {
                let (tx, rx) = crate::event::channel(1);
                (tx, Some(rx))
            }
        };
        let t = Instant::now();
        let show_ignored = ui.borrow().tree.show_ignored.unwrap_or(opts.config.tree.show_ignored);
        let project = ProjectView::new(&root, show_ignored);
        tracing::info!("tree read in {:?}", t.elapsed());
        let t = Instant::now();
        let git = Git::discover(&root).ok();
        tracing::info!("repository discovered in {:?}", t.elapsed());
        let recent = Recent::load(&opts.paths.cache_dir, &root);
        let picker = default_picker(opts.config.images.protocol);
        let find_history = crate::find::load_history(&opts.paths.cache_dir);
        let mut app = App {
            root,
            project_name,
            config: opts.config,
            paths: opts.paths,
            theme,
            theme_loader,
            theme_stamp,
            theme_forced,
            theme_filled: filled,
            keymap,
            focus: Focus::Left,
            left: Some(LeftPanel::Project),
            show_help: false,
            should_quit: false,
            message: None,
            tx,
            rx,
            areas: Default::default(),
            buttons: Vec::new(),
            prefs: None,
            project,
            git,
            status: StatusSnapshot::default(),
            status_job: StatusJob::default(),
            watcher: None,
            viewer: Viewer::new(),
            picker,
            md_images: MdImages::default(),
            recent,
            recent_popup: None,
            prompt: None,
            nvim: None,
            grid: Grid::default(),
            dirty: HashSet::new(),
            quit_confirm: None,
            change_hop: None,
            ui,
            ui_path,
            projects: crate::projects::ProjectsBar::default(),
            ws_request: None,
            started: false,
            projects_popup: None,
            project_hits: Vec::new(),
            dragging_divider: false,
            dragging_tab: None,
            drag_col: 0,
            selecting: false,
            log: crate::log::LogWindow::default(),
            shell: None,
            shell_visible: false,
            tab_hits: Vec::new(),
            last_click: None,
            pending_external: None,
            goto: None,
            file_index: None,
            index_job: false,
            index_dirty: true,
            find: None,
            find_window: FindWindow::default(),
            find_history,
            find_generation: 0,
            find_pending: false,
            commit: CommitView::default(),
            confirm: None,
            last_tick: Instant::now(),
        };
        if let Some(w) = warnings.first() {
            app.notify(format!("config [keys]: {}", w.0), MsgKind::Error);
        }
        if let Some(e) = startup_error {
            app.notify(e, MsgKind::Error);
        }
        app.commit.by_dir = app.ui.borrow().commit.group_by_folder.unwrap_or(false);
        Ok(app)
    }

    /// Called once the terminal is ready, before the first event: starts the
    /// file watcher and the first `git status`.
    pub fn on_start(&mut self) {
        vista_render::Highlighter::preload();
        // The first search in a process pays one-time costs (thread pool, matcher
        // and ignore-rule setup, cold file cache); pay them in the background.
        let root = self.root.clone();
        std::thread::spawn(move || {
            let opts = vista_search::FindOptions {
                query: "\u{1}vista-warm-up".into(),
                max_results: 1,
                ..Default::default()
            };
            vista_search::find::search(&root, &opts, &std::sync::atomic::AtomicBool::new(false), &|_| {});
        });
        if std::env::var_os("VISTA_NO_WATCH").is_none() {
            let started = Instant::now();
            let tx = self.tx.clone();
            match Watcher::new(&self.root, Duration::from_millis(120), move |c| {
                let _ = tx.send(AppEvent::Fs(c));
            }) {
                Ok(w) => {
                    tracing::info!("file watcher ready in {:?}", started.elapsed());
                    self.watcher = Some(w);
                }
                Err(e) => self.notify(format!("file watching unavailable: {e}"), MsgKind::Error),
            }
        }
        // Files created between reading the tree and the watcher starting
        // sent no event: read again now that changes are being watched, and
        // build the Go to File index after that point too.
        self.project.tree.refresh_all();
        self.project.rebuild(&self.status);
        self.request_index();
        self.request_status();
    }

    /// Called after the main loop ends, while the terminal is still set up.
    pub fn on_exit(&mut self) {
        self.watcher = None;
        self.shell = None;
        if let Some(mut n) = self.nvim.take() {
            n.quit();
        }
    }

    /// A command the run loop should execute with the terminal released.
    pub fn take_external(&mut self) -> Option<(String, PathBuf)> {
        self.pending_external.take()
    }

    /// Called after an external editor returned.
    pub fn after_external(&mut self, path: &Path, result: std::io::Result<std::process::ExitStatus>) {
        if let Err(e) = result {
            self.notify(format!("could not run the editor: {e}"), MsgKind::Error);
        }
        let ctx = self.load_ctx();
        self.viewer.reload_paths(&[path.to_path_buf()], &ctx);
        self.project.tree.refresh(&[path.to_path_buf()]);
        self.project.rebuild(&self.status);
        self.request_status();
    }

    /// Keys go to Neovim: the main area is focused and shows an editor.
    pub fn editing_focused(&self) -> bool {
        self.focus == Focus::Main && self.nvim.is_some() && self.viewer.active_tab().is_some_and(|t| t.editing)
    }

    fn edit_action(&mut self) {
        // In a text diff, edit side by side (HEAD | working tree), like IntelliJ.
        if self.focus == Focus::Main
            && self.viewer.active_tab().is_some_and(|t| {
                !t.editing
                    && matches!(&t.content, Content::Diff(d) if matches!(d.body, crate::diffview::DiffBody::Text))
            })
        {
            self.start_diff_editing();
            return;
        }
        let target = match self.focus {
            Focus::Left if self.left == Some(LeftPanel::Project) => self
                .project
                .selected_row()
                .filter(|r| !r.is_dir)
                .map(|r| (r.path.clone(), None)),
            _ => self.viewer.active_tab().filter(|t| !t.editing).map(|t| {
                let line = match &t.content {
                    Content::Diff(d) => d.new_line_at(t.cursor).map(|l| l + 1),
                    _ => t.has_cursor().then_some(t.cursor + 1),
                };
                (t.path.clone(), line)
            }),
        };
        if let Some((path, line)) = target {
            self.start_editing(&path, line);
        }
    }

    /// Opens `path` in the embedded Neovim (starting it if needed), or hands
    /// the terminal to `$EDITOR` when Neovim isn't available.
    pub fn start_editing(&mut self, path: &Path, line: Option<usize>) {
        if !self.ensure_nvim(path) {
            return;
        }
        match self.viewer.position(path) {
            Some(i) => self.viewer.activate(i),
            None => self.open_file(path),
        }
        // Neovim wraps long lines like the viewer did.
        let wrap = self.viewer.active_tab().map(|t| t.wrap);
        let number = Some(self.line_numbers());
        let res = self.nvim.as_ref().map(|n| n.open(path, line, wrap, number));
        if let Some(Err(e)) = res {
            self.notify(format!("editor: {e}"), MsgKind::Error);
            return;
        }
        // Opening a file ends any side-by-side diff editing in Neovim.
        self.stop_diff_editing_tabs();
        if let Some(t) = self.viewer.active_tab_mut() {
            t.editing = true;
        }
        self.focus = Focus::Main;
    }

    /// Edits the active diff tab's file side by side with its HEAD version
    /// (Neovim's diff mode: aligned, scroll-bound, re-diffed as you type).
    fn start_diff_editing(&mut self) {
        let Some(target) = self.diff_edit_target() else {
            return;
        };
        if !self.ensure_nvim(&target.path) {
            return;
        }
        let res = self.nvim.as_ref().map(|n| target.open_in(n));
        if let Some(Err(e)) = res {
            self.notify(format!("editor: {e}"), MsgKind::Error);
            return;
        }
        self.stop_diff_editing_tabs();
        if let Some(t) = self.viewer.active_tab_mut() {
            t.editing = true;
        }
        self.focus = Focus::Main;
    }

    /// What the side-by-side editor needs from the active diff tab.
    fn diff_edit_target(&self) -> Option<DiffEdit> {
        let t = self.viewer.active_tab()?;
        let Content::Diff(d) = &t.content else { return None };
        let rel = t.path.strip_prefix(&self.root).unwrap_or(&t.path);
        let label = format!("HEAD {} · {}", d.head, rel.display());
        // Open where the diff is: the cursor's line and the top visible line
        // of the working-tree side, and the same sideways scroll.
        let view = vista_nvim::DiffView {
            line: d.new_line_at(t.cursor).map(|l| l + 1),
            top: d.new_line_at(t.scroll).map(|l| l + 1),
            top_head: d.head_only_line_at(t.scroll).map(|l| l + 1),
            leftcol: t.hscroll,
            wrap: t.wrap,
            collapse: d.collapse,
        };
        Some(DiffEdit {
            path: t.path.clone(),
            head: d.old_text.clone(),
            label,
            view,
        })
    }

    /// Neovim shows one thing at a time: diff tabs stop being edited.
    fn stop_diff_editing_tabs(&mut self) {
        for t in &mut self.viewer.tabs {
            if t.diff.is_some() {
                t.editing = false;
            }
        }
    }

    /// Starts the embedded Neovim if needed; on failure falls back to
    /// `$EDITOR` for `path` and returns false.
    fn ensure_nvim(&mut self, path: &Path) -> bool {
        if self.nvim.is_none() {
            let size = self.areas.viewer.map(|r| (r.width, r.height)).unwrap_or((80, 24));
            let minimal = self.config.editor.config == vista_core::EditorConfigMode::Minimal;
            match editor::spawn(&self.config.editor.nvim, minimal, &self.root, size, self.tx.clone()) {
                Ok(n) => {
                    if self.config.editor.theme_sync {
                        let _ = n.set_highlights(&editor::highlight_groups(&self.theme));
                    }
                    self.nvim = Some(n);
                }
                Err(e) => {
                    let ed = editor::external_editor();
                    self.notify(format!("{e}; using {ed}"), MsgKind::Error);
                    self.pending_external = Some((ed, path.to_path_buf()));
                    return false;
                }
            }
        }
        true
    }

    fn leave_editor(&mut self) {
        if self.left.is_none() {
            self.left = Some(LeftPanel::Project);
        }
        self.focus = Focus::Left;
    }

    /// Keeps Neovim on the buffer of the active tab.
    fn sync_editor_buffer(&mut self) {
        let Some(t) = self.viewer.active_tab().filter(|t| t.editing) else {
            return;
        };
        if t.diff.is_some() {
            if let (Some(n), Some(target)) = (&self.nvim, self.diff_edit_target()) {
                let _ = target.open_in(n);
            }
        } else if let Some(n) = &self.nvim {
            let _ = n.open(&t.path, None, Some(t.wrap), Some(self.line_numbers()));
            self.stop_diff_editing_tabs();
        }
    }

    fn on_nvim(&mut self, e: NvimEvent) {
        match e {
            NvimEvent::Redraw(p) => self.grid.apply_redraw(&p),
            NvimEvent::Modified(p, m) => {
                if m {
                    self.dirty.insert(p);
                } else {
                    self.dirty.remove(&p);
                }
            }
            NvimEvent::Written(p) => {
                self.dirty.remove(&p);
                let ctx = self.load_ctx();
                self.viewer.reload_paths(std::slice::from_ref(&p), &ctx);
                self.project.tree.refresh(&[p]);
                self.project.rebuild(&self.status);
                self.request_status();
            }
            NvimEvent::Wrap { path, wrap, diff } => {
                // `:set wrap` / `:set nowrap` in Neovim carries over to the
                // viewer (and back, when the editor opens a file again).
                let shown = self.viewer.tabs.iter().find(|t| {
                    t.editing
                        && if diff {
                            t.diff.is_some()
                        } else {
                            t.diff.is_none() && t.path == path
                        }
                });
                if let Some(markdown) = shown.map(|t| vista_render::filetype::is_markdown_path(&t.path)) {
                    self.set_wrap(markdown, wrap);
                }
            }
            NvimEvent::Collapse(collapse) => {
                for t in self.viewer.tabs.iter_mut().filter(|t| t.editing) {
                    if let Content::Diff(d) = &mut t.content
                        && d.collapse != collapse
                    {
                        d.collapse = collapse;
                        d.rebuild();
                        t.scroll = 0;
                        t.cursor = 0;
                    }
                }
                self.remember(|s| s.diff.collapse = Some(collapse));
            }
            NvimEvent::DiffClosed => {
                // `:q` in the side-by-side editor: back to vista's diff.
                self.stop_diff_editing_tabs();
                self.notify("Diff editor closed", MsgKind::Info);
            }
            NvimEvent::Exited => {
                self.nvim = None;
                self.grid = Grid::default();
                self.dirty.clear();
                let paths: Vec<PathBuf> = self
                    .viewer
                    .tabs
                    .iter_mut()
                    .filter(|t| t.editing)
                    .map(|t| {
                        t.editing = false;
                        t.path.clone()
                    })
                    .collect();
                let ctx = self.load_ctx();
                self.viewer.reload_paths(&paths, &ctx);
                self.notify("Editor closed", MsgKind::Info);
            }
        }
    }

    pub fn notify(&mut self, text: impl Into<String>, kind: MsgKind) {
        let text = text.into();
        match kind {
            MsgKind::Error => tracing::warn!("{text}"),
            MsgKind::Info => tracing::info!("{text}"),
        }
        self.message = Some(StatusMsg {
            text,
            kind,
            at: Instant::now(),
        });
    }

    // ----- git status ----------------------------------------------------

    /// Asks for a fresh `git status` in the background.
    pub fn request_status(&mut self) {
        let Some(git) = self.git.clone() else { return };
        if self.status_job.running {
            self.status_job.pending = true;
            return;
        }
        self.status_job.running = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let res = StatusSnapshot::load(&git).map(Box::new).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::GitStatus(res));
        });
    }

    /// Loads status on the calling thread (tests and snapshots).
    pub fn load_status_now(&mut self) {
        if let Some(git) = &self.git
            && let Ok(s) = StatusSnapshot::load(git)
        {
            self.set_status(s);
        }
    }

    fn set_status(&mut self, s: StatusSnapshot) {
        let head_moved = s.branch.oid != self.status.branch.oid;
        self.status = s;
        self.project.rebuild(&self.status);
        self.commit.rebuild(&self.status, &self.root);
        if head_moved {
            // Gutter markers compare with HEAD; recompute them.
            let ctx = self.load_ctx();
            self.viewer.reload_all(&ctx);
        }
    }

    /// Rebuilds the file index in the background.
    pub fn request_index(&mut self) {
        if self.index_job {
            return;
        }
        self.index_job = true;
        self.index_dirty = false;
        let include_ignored = self.goto.as_ref().is_some_and(|g| g.include_ignored);
        let (root, tx) = (self.root.clone(), self.tx.clone());
        std::thread::spawn(move || {
            let index = Arc::new(FileIndex::build(&root, include_ignored));
            let _ = tx.send(AppEvent::Index { index, include_ignored });
        });
    }

    /// Builds the index on the calling thread (tests).
    pub fn build_index_now(&mut self) {
        self.file_index = Some(Arc::new(FileIndex::build(&self.root, false)));
    }

    /// Recent files relative to the project root, most recent first.
    pub fn recent_relative(&self) -> Vec<String> {
        self.recent
            .list
            .iter()
            .filter_map(|p| p.strip_prefix(&self.root).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect()
    }

    fn refresh_goto(&mut self) {
        let recent = self.recent_relative();
        if let Some(g) = self.goto.as_mut() {
            g.refresh(self.file_index.as_deref(), &recent);
        }
    }

    pub fn open_goto(&mut self, tab: GotoTab) {
        self.goto = Some(GotoPopup::new(tab, false));
        if self.index_dirty || self.file_index.is_none() {
            self.request_index();
        }
        self.refresh_goto();
    }

    /// Opens a file at an optional 1-based line, in the viewer or the editor.
    pub fn open_at(&mut self, path: &Path, line: Option<usize>, edit: bool) {
        if edit {
            self.start_editing(path, line);
            return;
        }
        self.open_file(path);
        if let (Some(n), Some(t)) = (line, self.viewer.active_tab_mut())
            && n > 0
        {
            t.set_cursor(n - 1);
            let h = t.view_height.max(1);
            t.scroll = (n - 1).saturating_sub(h / 2);
        }
    }

    /// Opens Find in Files, prefilled with `query`.
    pub fn open_find(&mut self, query: String) {
        // Directory scope: the folder selected in the tree (or the file's folder).
        let dir = self
            .project
            .selected_row()
            .map(|r| {
                if r.is_dir {
                    r.path.clone()
                } else {
                    r.path.parent().map(Path::to_path_buf).unwrap_or_default()
                }
            })
            .unwrap_or_else(|| self.root.clone());
        let mut p = FindPopup::new(query, dir);
        {
            let ui = self.ui.borrow();
            p.case = ui.find.case.unwrap_or(false);
            p.word = ui.find.words.unwrap_or(false);
            p.regex = ui.find.regex.unwrap_or(false);
        }
        self.find = Some(p);
        self.start_find();
    }

    /// (Re)starts the search for the popup's current options.
    pub fn start_find(&mut self) {
        let changed: Vec<PathBuf> = self
            .status
            .changes()
            .iter()
            // IntelliJ's "Changed files": local changes, not unversioned files.
            .filter(|c| {
                !matches!(
                    c.status,
                    vista_git::FileStatus::Deleted | vista_git::FileStatus::Unversioned
                )
            })
            .map(|c| c.path.clone())
            .collect();
        let open: Vec<PathBuf> = self.viewer.tabs.iter().map(|t| t.path.clone()).collect();
        let Some(p) = self.find.as_mut() else { return };
        p.cancel.store(true, Ordering::SeqCst);
        p.cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.find_generation += 1;
        p.generation = self.find_generation;
        p.reset_results();
        p.running = !p.query.is_empty();
        let opts = p.options(changed, open);
        let (root, tx, generation, cancel) = (self.root.clone(), self.tx.clone(), p.generation, p.cancel.clone());
        std::thread::spawn(move || {
            vista_search::find::search(&root, &opts, &cancel, &|event| {
                let _ = tx.send(AppEvent::Find { generation, event });
            });
        });
    }

    fn close_find(&mut self) {
        if let Some(p) = self.find.take() {
            p.cancel.store(true, Ordering::SeqCst);
            crate::find::push_history(&self.paths.cache_dir, &mut self.find_history, &p.query);
        }
    }

    fn find_key(&mut self, key: &KeyEvent) {
        let opts = |p: &FindPopup| (p.case, p.word, p.regex);
        let before = self.find.as_ref().map(opts);
        self.find_key_inner(key);
        // Case / words / regex are remembered (their keys keep the popup open).
        if let (Some(b), Some(after)) = (before, self.find.as_ref().map(opts))
            && b != after
        {
            self.remember(|s| {
                s.find.case = Some(after.0);
                s.find.words = Some(after.1);
                s.find.regex = Some(after.2);
            });
        }
    }

    fn find_key_inner(&mut self, key: &KeyEvent) {
        let history = self.find_history.clone();
        let Some(p) = self.find.as_mut() else { return };
        match p.handle_key(key, &history) {
            FindOutcome::Handled => {}
            // Coalesced: a burst of typed keys starts one search (see `drain_events`).
            FindOutcome::Restart => self.find_pending = true,
            FindOutcome::Close => self.close_find(),
            FindOutcome::Open { path, line, edit } => {
                self.close_find();
                self.open_at(&path, Some(line), edit);
            }
            FindOutcome::ToWindow => {
                let tab = ResultTab::new(p.query.clone(), p.results.clone());
                self.close_find();
                self.find_window.add(tab);
                self.focus = Focus::Bottom;
            }
        }
    }

    /// Keys while the Find tool window has focus.
    fn find_window_key(&mut self, key: &KeyEvent) -> bool {
        use ratatui::crossterm::event::KeyCode;
        let h = self.find_window.view_height;
        let n_tabs = self.find_window.tabs.len();
        let Some(tab) = self.find_window.active_tab_mut() else {
            return false;
        };
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => tab.move_by(-1, h),
            KeyCode::Down | KeyCode::Char('j') => tab.move_by(1, h),
            KeyCode::PageUp => tab.move_by(-(h as isize), h),
            KeyCode::PageDown => tab.move_by(h as isize, h),
            KeyCode::Left if n_tabs > 1 => self.find_window.active = (self.find_window.active + n_tabs - 1) % n_tabs,
            KeyCode::Right if n_tabs > 1 => self.find_window.active = (self.find_window.active + 1) % n_tabs,
            KeyCode::Enter => {
                if let Some(hit) = tab.selected_hit().cloned() {
                    let idx = tab.hits.iter().position(|x| *x == hit);
                    tab.current = idx;
                    self.open_at(&hit.path, Some(hit.line), false);
                }
            }
            KeyCode::Esc => self.focus = Focus::Main,
            _ => return false,
        }
        true
    }

    fn step_occurrence(&mut self, forward: bool) {
        let Some(hit) = self.find_window.active_tab_mut().and_then(|t| t.step(forward)) else {
            self.notify("No Find results", MsgKind::Info);
            return;
        };
        let focus = self.focus;
        self.open_at(&hit.path, Some(hit.line), false);
        if focus == Focus::Bottom {
            self.focus = Focus::Bottom;
        }
    }

    fn goto_key(&mut self, key: &KeyEvent) {
        let root = self.root.clone();
        let Some(g) = self.goto.as_mut() else { return };
        let outcome = g.handle_key(key, &root);
        self.goto_outcome(outcome);
    }

    fn goto_outcome(&mut self, outcome: GotoOutcome) {
        match outcome {
            GotoOutcome::Handled => self.refresh_goto(),
            GotoOutcome::Close => self.goto = None,
            GotoOutcome::Open { path, line, edit } => {
                self.goto = None;
                self.open_at(&path, line, edit);
            }
            GotoOutcome::Reindex => {
                self.file_index = None;
                self.index_job = false;
                self.request_index();
                self.refresh_goto();
            }
            GotoOutcome::FindText(q) => {
                self.goto = None;
                self.open_find(q);
            }
        }
    }

    // ----- commit ----------------------------------------------------------

    fn commit_key(&mut self, key: &KeyEvent) -> bool {
        let by_dir = self.commit.by_dir;
        let handled = self.commit_key_inner(key);
        if self.commit.by_dir != by_dir {
            let on = self.commit.by_dir;
            self.remember(|s| s.commit.group_by_folder = Some(on));
        }
        handled
    }

    fn commit_key_inner(&mut self, key: &KeyEvent) -> bool {
        let root = self.root.clone();
        match self.commit.handle_key(key, &self.status, &root) {
            CommitOutcome::Unhandled => return false,
            CommitOutcome::Handled => {}
            CommitOutcome::Commit { push } => self.do_commit(push),
            CommitOutcome::Show(change) => self.open_diff(&change),
            CommitOutcome::Rollback(files) => {
                let n = files.len();
                self.confirm = Some(Confirm {
                    text: format!(
                        "Rollback {n} file{}? Local changes will be lost.",
                        if n == 1 { "" } else { "s" }
                    ),
                    action: ConfirmAction::Rollback(files),
                });
            }
            CommitOutcome::Add(paths) => {
                if let Some(git) = &self.git {
                    match git.add(&paths) {
                        Ok(()) => self.notify(format!("Added {} file(s) to VCS", paths.len()), MsgKind::Info),
                        Err(e) => self.notify(e.to_string(), MsgKind::Error),
                    }
                }
                self.request_status();
            }
            CommitOutcome::AmendToggled => {
                if self.commit.amend
                    && self.commit.message.text.trim().is_empty()
                    && let Some(m) = self.git.as_ref().and_then(|g| g.last_message())
                {
                    self.commit.message.set(&m);
                }
            }
        }
        true
    }

    /// Commits the ticked files with the message (in the background).
    pub fn do_commit(&mut self, push: bool) {
        let Some(git) = self.git.clone() else {
            self.notify("Not a git repository", MsgKind::Error);
            return;
        };
        if self.commit.busy {
            return;
        }
        let message = self.commit.message.text.trim().to_string();
        // (change, hunks left out) — partial commits are built on the worker.
        let plan: Vec<(Change, std::collections::HashSet<u64>)> = self
            .commit
            .included_changes(&self.status)
            .into_iter()
            .map(|c| {
                let ex = self.commit.excluded.get(&c.path).cloned().unwrap_or_default();
                (c, ex)
            })
            .collect();
        let files = plan.clone();
        let amend = self.commit.amend;
        if message.is_empty() {
            self.commit.output = Some((false, "Commit message is empty".into()));
            self.commit.editing_message = true;
            return;
        }
        if plan.is_empty() && !amend {
            self.commit.output = Some((false, "No files are ticked for the commit".into()));
            return;
        }
        self.commit.busy = true;
        self.commit.output = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let n = files.len();
            let files: Vec<CommitFile> = files
                .iter()
                .map(|(c, excluded)| {
                    if excluded.is_empty() {
                        return CommitFile::whole(c);
                    }
                    let head = c.orig_path.as_deref().unwrap_or(&c.path);
                    let old = git
                        .head_content(head)
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                        .unwrap_or_default();
                    let new = std::fs::read(&c.path)
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                        .unwrap_or_default();
                    CommitFile {
                        path: c.path.clone(),
                        orig_path: c.orig_path.clone(),
                        content: vista_git::write::CommitContent::Blob(
                            vista_diff::partial_content(&old, &new, excluded).into_bytes(),
                        ),
                    }
                })
                .collect();
            let result = git.commit(&files, &message, amend).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitDone {
                result: Box::new(result),
                files: n,
                push,
            });
        });
    }

    fn on_commit_done(&mut self, result: Result<CommitResult, String>, files: usize, push: bool) {
        self.commit.busy = false;
        match result {
            Ok(r) => {
                let text = format!(
                    "Committed {files} file{}: {} {}",
                    if files == 1 { "" } else { "s" },
                    r.id,
                    r.subject
                );
                self.notify(text.clone(), MsgKind::Info);
                self.commit.output = Some((true, text));
                self.commit.message.set("");
                self.commit.amend = false;
                // Hunk ids change once committed; start fresh.
                self.commit.excluded.clear();
                self.commit.editing_message = false;
                if push && let Some(git) = self.git.clone() {
                    self.commit.busy = true;
                    let tx = self.tx.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(AppEvent::PushDone(git.push().map_err(|e| e.to_string())));
                    });
                }
            }
            Err(e) => {
                self.notify("Commit failed", MsgKind::Error);
                // Show what the hooks said, without the command prefix.
                let text = e.split_once(" failed: ").map(|(_, rest)| rest.to_string()).unwrap_or(e);
                self.commit.output = Some((false, text));
            }
        }
        self.request_status();
    }

    /// Shows `change` as a diff against HEAD.
    pub fn open_diff(&mut self, change: &Change) {
        let ctx = self.load_ctx();
        let spec = crate::diffview::DiffSpec {
            orig_path: change.orig_path.clone(),
            revs: None,
        };
        self.viewer.open_diff(&change.path, spec, &ctx);
        self.focus = Focus::Main;
    }

    // ----- Git window: log and file history -----------------------------------

    /// Shows the Terminal tool window, starting the shell (in the project
    /// folder) if it isn't running.
    fn show_shell(&mut self) {
        self.find_window.visible = false;
        self.log.visible = false;
        self.shell_visible = true;
        self.focus = Focus::Bottom;
        if self.shell.as_ref().is_none_or(|s| s.exited.is_some()) {
            self.start_shell();
        }
    }

    fn start_shell(&mut self) {
        let size = self
            .areas
            .shell
            .or(self.areas.panel_bottom)
            .map(|r| (r.width, r.height.saturating_sub(1)))
            .unwrap_or((self.areas.top.width.max(40), 10));
        match crate::term_tab::TermTab::spawn(0, "", &self.root, size, self.tx.clone()) {
            Ok(s) => self.shell = Some(s),
            Err(e) => self.notify(format!("{e:#}"), MsgKind::Error),
        }
    }

    /// Keys while the Terminal tool window has focus: all to the shell, but
    /// Ctrl-\ (back to the editor) and the tool-window keys.
    fn shell_key(&mut self, key: &KeyEvent) {
        match self.keymap.action_for(key) {
            Some(Action::LeaveEditor) => self.focus = Focus::Main,
            Some(
                a @ (Action::ToggleProject
                | Action::ToggleCommit
                | Action::ToggleFind
                | Action::ToggleLog
                | Action::ToggleTerminal
                | Action::NextProject
                | Action::PrevProject),
            ) => self.handle_action(a),
            _ => match self.shell.as_mut() {
                // The shell ended: Enter starts a new one.
                Some(s) if s.exited.is_some() => {
                    if key.code == ratatui::crossterm::event::KeyCode::Enter {
                        self.start_shell();
                    }
                }
                Some(s) => s.key(key),
                None => self.start_shell(),
            },
        }
    }

    /// Shows the Git window (instead of Find), loading the log.
    fn show_log(&mut self, path: Option<PathBuf>) {
        if self.git.is_none() {
            self.notify("Not a git repository", MsgKind::Info);
            return;
        }
        self.find_window.visible = false;
        self.shell_visible = false;
        self.log.visible = true;
        if self.log.path != path {
            self.log.path = path;
            self.log.commits.clear();
            self.log.selected = 0;
            self.log.scroll = 0;
        }
        self.log.on_files = false;
        self.focus = Focus::Bottom;
        self.load_log();
    }

    fn load_log(&mut self) {
        let Some(git) = self.git.clone() else { return };
        self.log.generation += 1;
        self.log.loading = true;
        let (generation, path, tx) = (self.log.generation, self.log.path.clone(), self.tx.clone());
        std::thread::spawn(move || {
            let result = vista_git::log::log(&git, path.as_deref(), crate::log::MAX_COMMITS).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::Log { generation, result });
        });
    }

    fn load_commit_files(&mut self, commit: vista_git::Commit) {
        let Some(git) = self.git.clone() else { return };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = vista_git::log::commit_files(&git, &commit).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitFiles {
                hash: commit.hash,
                result,
            });
        });
    }

    fn log_outcome(&mut self, outcome: crate::log::LogOutcome) -> bool {
        use crate::log::LogOutcome;
        match outcome {
            LogOutcome::Unhandled => return false,
            LogOutcome::Handled => {}
            LogOutcome::Close => {
                self.log.visible = false;
                self.focus = Focus::Main;
            }
            LogOutcome::NeedFiles(c) => self.load_commit_files(c),
            LogOutcome::Open(c, f) => self.open_history_diff(&c, &f),
            LogOutcome::Reload => self.load_log(),
        }
        true
    }

    /// A file's change in a commit, as a read-only diff (parent ⇄ commit).
    fn open_history_diff(&mut self, commit: &vista_git::Commit, file: &vista_git::CommitFile) {
        let ctx = self.load_ctx();
        let spec = crate::diffview::DiffSpec {
            orig_path: file.old_path.clone(),
            revs: Some(crate::diffview::Revs {
                parent: commit.parents.first().cloned(),
                commit: commit.hash.clone(),
                short: commit.short.clone(),
                subject: commit.subject.clone(),
            }),
        };
        self.viewer.open_diff(&file.path, spec, &ctx);
        self.focus = Focus::Main;
    }

    /// Shows the diff of the next/previous file in the Commit list.
    /// Changed files in Commit-window order (in a history diff: that
    /// commit's files).
    fn changed_files(&self) -> Vec<Change> {
        let history = self
            .viewer
            .active_tab()
            .and_then(|t| t.diff.as_ref())
            .and_then(|d| d.revs.as_ref());
        if let Some(r) = history {
            return self
                .log
                .files_of(&r.commit)
                .unwrap_or_default()
                .iter()
                .map(|f| Change {
                    path: f.path.clone(),
                    status: vista_git::FileStatus::Modified,
                    orig_path: f.old_path.clone(),
                    staged: false,
                    unstaged: false,
                })
                .collect();
        }
        self.commit
            .rows
            .iter()
            .filter_map(|r| match r {
                crate::commit::Row::File { change, .. } => Some(change.clone()),
                _ => None,
            })
            .collect()
    }

    /// The changed file after (before) `cur`; `None` at either end (no
    /// wrapping around, so the end of the changes is visible).
    fn neighbour(files: &[Change], cur: &Path, forward: bool) -> Option<Change> {
        let i = files.iter().position(|c| c.path == cur);
        let next = match (i, forward) {
            (Some(i), true) => i + 1,
            (Some(i), false) => i.checked_sub(1)?,
            (None, true) => 0,
            (None, false) => files.len().checked_sub(1)?,
        };
        files.get(next).cloned()
    }

    /// Past the last (first) change of a file, like IntelliJ: the first press
    /// offers the next (previous) changed file, a second press goes there.
    fn hop_to_changed_file(&mut self, forward: bool) {
        let Some(tab) = self.viewer.active_tab() else { return };
        let (cur, is_diff) = (tab.path.clone(), tab.diff.is_some());
        // The plain viewer can't show deleted files; diffs can.
        let files: Vec<Change> = self
            .changed_files()
            .into_iter()
            .filter(|c| is_diff || c.path.is_file())
            .collect();
        let Some(target) = Self::neighbour(&files, &cur, forward) else {
            // The end: say so instead of wrapping around.
            let text = if forward {
                "Last change: no more changed files after this one"
            } else {
                "First change: no changed files before this one"
            };
            self.change_hop = Some(ChangeHop {
                forward,
                at: Instant::now(),
                text: text.into(),
                armed: false,
            });
            return;
        };
        let armed = self
            .change_hop
            .as_ref()
            .is_some_and(|h| h.armed && h.forward == forward && h.at.elapsed() < HOP_TTL);
        if !armed {
            let key = self.keymap.hint(if forward {
                Action::NextChange
            } else {
                Action::PrevChange
            });
            let name = target
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let text = if forward {
                format!("Last change in this file. Press {key} again to go to {name}")
            } else {
                format!("First change in this file. Press {key} again to go to {name}")
            };
            self.change_hop = Some(ChangeHop {
                forward,
                at: Instant::now(),
                text,
                armed: true,
            });
            return;
        }
        self.change_hop = None;
        // Forward lands on the file's first change, backward on its last.
        if is_diff {
            self.show_diff_of(&target, forward);
        } else {
            self.open_file(&target.path);
            if let Some(t) = self.viewer.active_tab_mut()
                && !t.land_on_change(forward)
            {
                t.land = Some(forward);
            }
        }
    }

    /// `J`/`K` in a diff: the next/previous changed file, on its first change.
    fn step_diff_file(&mut self, forward: bool) {
        let Some(cur) = self.viewer.active_tab().map(|t| t.path.clone()) else {
            return;
        };
        let files = self.changed_files();
        match Self::neighbour(&files, &cur, forward) {
            Some(change) => self.show_diff_of(&change, true),
            None if forward => self.notify("This is the last changed file", MsgKind::Info),
            None => self.notify("This is the first changed file", MsgKind::Info),
        }
    }

    /// Shows `change` in the active diff tab (selecting it in the Commit
    /// list) and puts the cursor on its first or last change once loaded.
    fn show_diff_of(&mut self, change: &Change, first: bool) {
        if let Some(row) = self
            .commit
            .rows
            .iter()
            .position(|r| matches!(r, crate::commit::Row::File { change: c, .. } if c.path == change.path))
        {
            self.commit.selected = row;
        }
        let ctx = self.load_ctx();
        // Stepping within a history diff stays in that commit.
        let revs = self
            .viewer
            .active_tab()
            .and_then(|t| t.diff.as_ref())
            .and_then(|d| d.revs.clone());
        let spec = crate::diffview::DiffSpec {
            orig_path: change.orig_path.clone(),
            revs,
        };
        self.viewer.retarget_diff(&change.path, spec, &ctx);
        if let Some(t) = self.viewer.active_tab_mut() {
            t.land = Some(first);
        }
    }

    fn run_confirmed(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::Rollback(files) => {
                if let Some(git) = &self.git {
                    match git.rollback(&files) {
                        Ok(()) => self.notify(format!("Rolled back {} file(s)", files.len()), MsgKind::Info),
                        Err(e) => self.notify(e.to_string(), MsgKind::Error),
                    }
                }
                let paths: Vec<PathBuf> = files.iter().map(|c| c.path.clone()).collect();
                self.project.tree.refresh(&paths);
                self.project.rebuild(&self.status);
                let ctx = self.load_ctx();
                self.viewer.reload_paths(&paths, &ctx);
                self.request_status();
            }
        }
    }

    fn on_fs_change(&mut self, change: FsChange) {
        if !change.paths.is_empty() {
            self.index_dirty = true;
            if self.goto.is_some() {
                self.request_index();
            }
            self.project.tree.refresh(&change.paths);
            self.project.rebuild(&self.status);
            let ctx = self.load_ctx();
            self.viewer.reload_paths(&change.paths, &ctx);
        }
        self.request_status();
    }

    pub fn load_ctx(&self) -> LoadCtx {
        LoadCtx {
            tx: self.tx.clone(),
            git: self.git.clone(),
            images_enabled: self.picker.is_some(),
            wrap_code: self.wrap_default(false),
            wrap_markdown: self.wrap_default(true),
        }
    }

    // ----- remembered settings ----------------------------------------------

    /// Line numbers in the viewer (and the editor).
    pub fn line_numbers(&self) -> bool {
        self.ui.borrow().viewer.line_numbers.unwrap_or(true)
    }

    /// Whether long lines wrap for Markdown or for code.
    fn wrap_default(&self, markdown: bool) -> bool {
        if markdown {
            self.ui.borrow().viewer.wrap_markdown.unwrap_or(true)
        } else {
            self.ui.borrow().viewer.wrap_code.unwrap_or(false)
        }
    }

    /// Changes the remembered settings and saves them if anything changed:
    /// the change is applied to `state.toml` as it is now (other vistas may
    /// have saved meanwhile), and this instance takes the merged result.
    fn remember(&mut self, change: impl Fn(&mut vista_core::UiState)) {
        let changed = {
            let mut ui = self.ui.borrow_mut();
            let before = ui.clone();
            change(&mut ui);
            *ui != before
        };
        if !changed {
            return;
        }
        match vista_core::UiState::update(&self.ui_path, &change) {
            Ok(merged) => *self.ui.borrow_mut() = merged,
            Err(e) => self.notify(format!("cannot save settings: {e}"), MsgKind::Error),
        }
    }

    /// Wrapping for Markdown or code: remembered, and applied to every open
    /// tab of that kind.
    fn set_wrap(&mut self, markdown: bool, on: bool) {
        self.remember(|s| {
            if markdown {
                s.viewer.wrap_markdown = Some(on);
            } else {
                s.viewer.wrap_code = Some(on);
            }
        });
        for t in &mut self.viewer.tabs {
            if vista_render::filetype::is_markdown_path(&t.path) == markdown && t.wrap != on {
                t.wrap = on;
                t.hscroll = 0;
            }
        }
    }

    /// Remembers the active diff's view options (`u`, `z`, `i`) for new diffs.
    fn remember_diff_options(&mut self) {
        let Some(Content::Diff(d)) = self.viewer.active_tab().map(|t| &t.content) else {
            return;
        };
        let (unified, collapse, ws) = (d.unified, d.collapse, d.opts.ignore_whitespace);
        self.remember(|s| {
            s.diff.unified = Some(unified);
            s.diff.collapse = Some(collapse);
            s.diff.ignore_whitespace = Some(ws);
        });
    }

    /// Cell size in pixels used to size images.
    pub fn font_size(&self) -> (u16, u16) {
        self.picker
            .as_ref()
            .map(|p| {
                let fs = p.font_size();
                (fs.width, fs.height)
            })
            .unwrap_or((8, 16))
    }

    // ----- events ----------------------------------------------------------

    pub fn handle_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Term(Event::Key(key)) => {
                if key.kind != KeyEventKind::Release {
                    self.handle_key(key);
                }
            }
            AppEvent::Term(Event::Mouse(m)) => self.handle_mouse(m),
            AppEvent::Term(Event::Paste(text)) if self.focus == Focus::Bottom && self.shell_visible => {
                if let Some(sh) = self.shell.as_mut() {
                    sh.paste(&text);
                }
            }
            AppEvent::Term(Event::Paste(text)) => {
                if self.editing_focused()
                    && let Some(n) = &self.nvim
                {
                    let _ = n.paste(&text);
                }
            }
            AppEvent::Term(_) => {}
            // Terminal tabs belong to the workspace.
            // The Terminal tool window's shell (terminal tabs belong to the workspace).
            AppEvent::TermOutput(pending) => pending.store(false, std::sync::atomic::Ordering::SeqCst),
            AppEvent::TermExited => {
                if let Some(s) = self.shell.as_mut() {
                    s.mark_exited();
                }
            }
            AppEvent::Log { generation, result } => {
                if generation == self.log.generation {
                    match result {
                        Ok(commits) => {
                            self.log.set_commits(commits);
                            let need = self.log.need_files();
                            self.log_outcome(need);
                        }
                        Err(e) => {
                            self.log.loading = false;
                            self.log.error = Some(e);
                        }
                    }
                }
            }
            AppEvent::CommitFiles { hash, result } => {
                self.log.files.insert(hash, result);
            }
            AppEvent::Nvim(e) => self.on_nvim(e),
            AppEvent::DiffLoaded { tab, generation, load } => {
                let picker = self.picker.clone();
                let diff_ui = self.ui.borrow().diff.clone();
                if let Some(t) = self.viewer.tab_by_id_mut(tab)
                    && t.generation == generation
                {
                    let crate::diffview::DiffLoad { doc, images, error } = *load;
                    match (doc, error) {
                        (Some(mut d), _) => {
                            if let (Some((old, new)), crate::diffview::DiffBody::Image { .. }) = (images, &d.body) {
                                let to_state = |img: Option<Box<image::DynamicImage>>| match (&picker, img) {
                                    (Some(p), Some(i)) => ImageState::Ready(Box::new(p.new_resize_protocol(*i))),
                                    (None, _) => ImageState::Disabled,
                                    (_, None) => ImageState::Failed("not present".into()),
                                };
                                d.body = crate::diffview::DiffBody::Image {
                                    old: to_state(old),
                                    new: to_state(new),
                                };
                            }
                            // Keep the view options when a diff reloads; a fresh
                            // diff opens on its first change, like IntelliJ.
                            let fresh = !matches!(t.content, Content::Diff(_));
                            if let Content::Diff(prev) = &t.content {
                                d.unified = prev.unified;
                                d.collapse = prev.collapse;
                                d.opts = prev.opts;
                                d.rebuild();
                            } else {
                                // A new diff opens with the remembered options.
                                d.unified = diff_ui.unified.unwrap_or(false);
                                d.collapse = diff_ui.collapse.unwrap_or(false);
                                d.opts.ignore_whitespace = diff_ui.ignore_whitespace.unwrap_or(false);
                                d.rebuild();
                            }
                            let n = d.len();
                            t.content = Content::Diff(Box::new(d));
                            t.cursor = t.cursor.min(n.saturating_sub(1));
                            t.scroll = t.scroll.min(n.saturating_sub(1));
                            if let Some(first) = t.land.take().or(fresh.then_some(true)) {
                                t.land_on_change(first);
                            }
                        }
                        (None, e) => t.content = Content::Error(e.unwrap_or_default()),
                    }
                }
            }
            AppEvent::CommitDone { result, files, push } => self.on_commit_done(*result, files, push),
            AppEvent::PushDone(res) => {
                self.commit.busy = false;
                match res {
                    Ok(_) => self.notify("Pushed to remote", MsgKind::Info),
                    Err(e) => {
                        self.notify("Push failed", MsgKind::Error);
                        self.commit.output = Some((false, e));
                    }
                }
                self.request_status();
            }
            AppEvent::Find { generation, event } => {
                if let Some(p) = self.find.as_mut()
                    && p.generation == generation
                {
                    match event {
                        vista_search::FindEvent::Hits(h) => p.add_hits(h),
                        vista_search::FindEvent::Done(s) => {
                            p.running = false;
                            p.summary = Some(s);
                        }
                        vista_search::FindEvent::Error(e) => {
                            p.running = false;
                            p.error = Some(e);
                        }
                    }
                }
            }
            AppEvent::Index { index, include_ignored } => {
                self.index_job = false;
                let wanted = self.goto.as_ref().is_some_and(|g| g.include_ignored);
                if include_ignored == wanted {
                    self.file_index = Some(index);
                    self.refresh_goto();
                } else {
                    self.request_index();
                }
            }
            AppEvent::GitStatus(res) => {
                self.status_job.running = false;
                match res {
                    Ok(s) => self.set_status(*s),
                    Err(e) => self.notify(format!("git status failed: {e}"), MsgKind::Error),
                }
                if std::mem::take(&mut self.status_job.pending) {
                    self.request_status();
                }
            }
            AppEvent::Fs(change) => self.on_fs_change(change),
            AppEvent::Loaded {
                tab,
                generation,
                loaded,
            } => {
                if let Some(t) = self.viewer.tab_by_id_mut(tab) {
                    crate::viewer::apply_loaded(t, generation, *loaded);
                }
            }
            AppEvent::ImageDecoded { tab, generation, image } => {
                let proto = match (&self.picker, image) {
                    (Some(p), Ok(img)) => ImageState::Ready(Box::new(p.new_resize_protocol(*img))),
                    (None, _) => ImageState::Disabled,
                    (_, Err(e)) => ImageState::Failed(e),
                };
                if let Some(t) = self.viewer.tab_by_id_mut(tab)
                    && t.generation == generation
                    && let Content::Image { state, .. } = &mut t.content
                {
                    *state = proto;
                }
            }
            AppEvent::MdImageDecoded { path, image } => {
                let state = match (&self.picker, image) {
                    (Some(p), Ok(img)) => ImageState::Ready(Box::new(p.new_resize_protocol(*img))),
                    (_, Err(e)) => ImageState::Failed(e),
                    (None, _) => ImageState::Disabled,
                };
                self.md_images.entries.insert(path, state);
            }
        }
    }

    /// Processes every event already queued (used by the main loop and tests).
    /// Waits for the next event on a project's own channel (a project
    /// running alone, as in tests).
    pub fn recv_timeout(&self, timeout: Duration) -> Result<AppEvent, std::sync::mpsc::RecvTimeoutError> {
        match &self.rx {
            Some(rx) => rx.recv_timeout(timeout).map(|(_, ev)| ev),
            None => Err(std::sync::mpsc::RecvTimeoutError::Disconnected),
        }
    }

    /// Handles everything queued on a project's own channel (a project
    /// running alone; in a workspace the workspace delivers events).
    pub fn drain_events(&mut self) -> usize {
        let mut n = 0;
        while let Some((_, ev)) = self.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.handle_event(ev);
            n += 1;
        }
        self.flush_pending();
        n
    }

    /// Work deferred until queued input is handled (search restarts).
    pub fn flush_pending(&mut self) {
        if std::mem::take(&mut self.find_pending) && self.find.is_some() {
            self.start_find();
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Anything but next/previous change drops a "press again" offer.
        if !matches!(
            self.keymap.action_for(&key),
            Some(Action::NextChange | Action::PrevChange)
        ) {
            self.change_hop = None;
        }
        if self.show_help {
            // Any key closes help; the key itself is consumed.
            self.show_help = false;
            return;
        }
        if let Some(p) = self.prefs.as_mut() {
            match p.handle_key(&key) {
                crate::prefs::PrefsOutcome::Handled => {}
                crate::prefs::PrefsOutcome::Close => self.prefs = None,
                crate::prefs::PrefsOutcome::Change(pref, forward) => self.change_pref(pref, forward),
            }
            return;
        }
        if let Some(c) = self.confirm.take() {
            use ratatui::crossterm::event::KeyCode;
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => self.run_confirmed(c.action),
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {}
                _ => self.confirm = Some(c),
            }
            return;
        }
        if self.goto.is_some() {
            self.goto_key(&key);
            return;
        }
        if self.find.is_some() {
            self.find_key(&key);
            return;
        }
        if self.focus == Focus::Bottom && self.log.visible {
            let outcome = self.log.handle_key(&key);
            if self.log_outcome(outcome) {
                return;
            }
        }
        if self.focus == Focus::Bottom && self.find_window.visible && self.find_window_key(&key) {
            return;
        }
        if let Some(popup) = self.projects_popup.as_mut() {
            match popup.handle_key(&key) {
                crate::projects::PopupOutcome::Handled => {}
                crate::projects::PopupOutcome::Close => self.projects_popup = None,
                crate::projects::PopupOutcome::Request(r) => {
                    self.projects_popup = None;
                    self.ws_request = Some(r);
                }
            }
            return;
        }
        if let Some(popup) = self.recent_popup.as_mut() {
            match popup.handle_key(&key, &self.recent.list) {
                PopupOutcome::Handled => {}
                PopupOutcome::Close => self.recent_popup = None,
                PopupOutcome::Open(p) => {
                    self.recent_popup = None;
                    self.open_file(&p);
                }
            }
            return;
        }
        if self.prompt.is_some() {
            self.prompt_key(&key);
            return;
        }
        if self.focus == Focus::Bottom && self.shell_visible {
            self.shell_key(&key);
            return;
        }
        if self.editing_focused() {
            match self.keymap.action_for(&key) {
                Some(Action::LeaveEditor) => self.leave_editor(),
                Some(
                    a @ (Action::ToggleProject
                    | Action::ToggleCommit
                    | Action::ToggleFind
                    | Action::NextProject
                    | Action::PrevProject),
                ) => self.handle_action(a),
                _ => {
                    if let (Some(n), Some(keys)) = (&self.nvim, editor::key_notation(&key)) {
                        let _ = n.input(&keys);
                    }
                }
            }
            return;
        }
        if self.focus == Focus::Main
            && let Some(tab) = self.viewer.active_tab_mut()
        {
            match tab.handle_key(&key) {
                ViewerOutcome::Handled => {
                    if self.viewer.active_tab().is_some_and(|t| t.diff.is_some()) {
                        self.remember_diff_options();
                    }
                    return;
                }
                ViewerOutcome::Wrap(on) => {
                    let markdown = vista_render::filetype::is_markdown_path(&tab.path);
                    self.set_wrap(markdown, on);
                    let kind = if markdown { "Markdown" } else { "code" };
                    self.notify(
                        if on {
                            format!("Long lines wrap ({kind}, w to scroll sideways)")
                        } else {
                            format!("Long lines scroll sideways ({kind}): ←/→, shift+wheel or the trackpad (w to wrap)")
                        },
                        MsgKind::Info,
                    );
                    return;
                }
                ViewerOutcome::OpenAt(line) => {
                    let path = tab.path.clone();
                    self.open_at(&path, Some(line + 1), false);
                    return;
                }
                ViewerOutcome::DiffFile(forward) => {
                    self.step_diff_file(forward);
                    return;
                }
                ViewerOutcome::ToggleHunk(..) if tab.diff.as_ref().is_some_and(|d| d.revs.is_some()) => {
                    self.notify("History diffs are read-only", MsgKind::Info);
                    return;
                }
                ViewerOutcome::ToggleHunk(id, all) => {
                    let path = tab.path.clone();
                    self.commit.toggle_hunk(&path, id, &all);
                    return;
                }
                ViewerOutcome::Info(m) => {
                    self.notify(m, MsgKind::Info);
                    return;
                }
                ViewerOutcome::Copy(text) => {
                    self.copy_text(&text);
                    return;
                }
                ViewerOutcome::ShowDiff => {
                    let path = tab.path.clone();
                    let change = self.status.change(&path).cloned().unwrap_or(Change {
                        path,
                        status: vista_git::FileStatus::Modified,
                        orig_path: None,
                        staged: false,
                        unstaged: false,
                    });
                    self.open_diff(&change);
                    return;
                }
                ViewerOutcome::GotoLinePrompt => {
                    self.prompt = Some(Prompt {
                        label: "Go to line",
                        input: String::new(),
                    });
                    return;
                }
                ViewerOutcome::Unhandled => {}
            }
        }
        if self.focus == Focus::Left && self.left == Some(LeftPanel::Commit) && self.commit_key(&key) {
            return;
        }
        if self.focus == Focus::Left && self.left == Some(LeftPanel::Project) {
            match self.project.handle_key(&key, &self.status) {
                Outcome::Handled => return,
                Outcome::Open(p) => {
                    self.open_file(&p);
                    return;
                }
                Outcome::Unhandled => {}
            }
        }
        if let Some(action) = self.keymap.action_for(&key) {
            self.handle_action(action);
        }
    }

    fn copy_text(&mut self, text: &str) {
        match crate::selection::copy(text) {
            Ok(how) => {
                let lines = text.lines().count().max(1);
                let what = if lines == 1 {
                    format!("{} characters", text.chars().count())
                } else {
                    format!("{lines} lines")
                };
                self.notify(format!("Copied {what} (via {how})"), MsgKind::Info);
            }
            Err(e) => self.notify(format!("cannot copy: {e}"), MsgKind::Error),
        }
    }

    /// Copies the viewer's selection, if there is one.
    fn copy_selection(&mut self) {
        if let Some(text) = self.viewer.active_tab().and_then(|t| t.selected_text()) {
            self.copy_text(&text);
        }
    }

    /// True when this click repeats the previous one on the same row soon enough.
    fn is_double_click(&mut self, list: ClickList, row: usize) -> bool {
        let now = Instant::now();
        let double =
            matches!(self.last_click, Some((l, r, t)) if l == list && r == row && now.duration_since(t) < DOUBLE_CLICK);
        self.last_click = if double { None } else { Some((list, row, now)) };
        double
    }

    /// Columns where the tree / editor divider can be grabbed: the
    /// separator, or the panel's right edge when there is none.
    fn on_divider(&self, col: u16, row: u16) -> bool {
        let Some(left) = self.areas.left else { return false };
        if !(left.y..left.y + left.height).contains(&row) {
            return false;
        }
        match self.areas.separator {
            Some(sep) => (sep.x..sep.x + sep.width).contains(&col),
            None => col + 1 == left.x + left.width,
        }
    }

    /// Sets the tree / Commit panel width (kept within the window when drawn).
    fn set_left_width(&mut self, width: u16, save: bool) {
        let width = width.max(crate::ui::MIN_LEFT);
        self.ui.borrow_mut().layout.left_width = Some(width);
        // Saved even if unchanged: a drag sets the width as it goes and
        // saves once, on release.
        if save {
            match vista_core::UiState::update(&self.ui_path, |s| s.layout.left_width = Some(width)) {
                Ok(merged) => *self.ui.borrow_mut() = merged,
                Err(e) => self.notify(format!("cannot save settings: {e}"), MsgKind::Error),
            }
        }
    }

    pub fn handle_mouse(&mut self, m: MouseEvent) {
        let pos = Position::new(m.column, m.row);
        // Dragging in the viewer extends the selection (scrolling at the edges).
        if self.selecting {
            match m.kind {
                MouseEventKind::Drag(MouseButton::Left) => {
                    // A drag isn't a click: the next press starts afresh.
                    self.last_click = None;
                    if let Some(tab) = self.viewer.active_tab_mut() {
                        let (top, height) = {
                            let lay = tab.layout.borrow();
                            (lay.y, lay.height)
                        };
                        if m.row < top {
                            tab.scroll_by(-1);
                        } else if m.row >= top + height {
                            tab.scroll_by(1);
                        }
                        if let (Some(p), Some(sel)) = (tab.pos_at(m.column, m.row), tab.selection.as_mut()) {
                            sel.head = p;
                        }
                    }
                    return;
                }
                MouseEventKind::Up(_) => {
                    self.selecting = false;
                    if let Some(tab) = self.viewer.active_tab_mut()
                        && tab.selection.is_some_and(|s| s.is_empty())
                    {
                        tab.selection = None;
                    } else if self.copy_on_select() {
                        self.copy_selection();
                    }
                    return;
                }
                _ => self.selecting = false,
            }
        }
        // Dragging a tab over others moves it there.
        if let Some(from) = self.dragging_tab {
            match m.kind {
                MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some((to, _)) = self.tab_at(m.column)
                        && crate::projects::drag_moves(from, to, m.column, self.drag_col)
                    {
                        self.viewer.move_tab(from, to);
                        self.dragging_tab = Some(to);
                    }
                    self.drag_col = m.column;
                    return;
                }
                MouseEventKind::Up(_) => {
                    self.dragging_tab = None;
                    return;
                }
                _ => self.dragging_tab = None,
            }
        }
        // Dragging the divider resizes the tree; saved on release.
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if self.on_divider(m.column, m.row) => {
                self.dragging_divider = true;
                return;
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging_divider => {
                if let Some(left) = self.areas.left {
                    self.set_left_width(m.column.saturating_sub(left.x), false);
                }
                return;
            }
            MouseEventKind::Up(MouseButton::Left) if self.dragging_divider => {
                self.dragging_divider = false;
                if let Some(left) = self.areas.left {
                    self.set_left_width(m.column.saturating_sub(left.x), true);
                }
                return;
            }
            _ => {}
        }
        if let MouseEventKind::Down(MouseButton::Left) = m.kind {
            // Help closes on any click, as on any key.
            if self.show_help {
                self.show_help = false;
                return;
            }
            let on_project_tab =
                m.row == self.areas.top.y && self.project_hits.iter().any(|h| (h.x.0..h.x.1).contains(&m.column));
            if !on_project_tab && let Some(c) = crate::ui::click::at(&self.buttons, m.column, m.row) {
                self.click(c);
                return;
            }
            // A click beside the Preferences page closes it.
            if self.prefs.is_some() {
                self.prefs = None;
                return;
            }
            if self.projects_popup.is_some() {
                if let Some(r) = self.areas.projects_list.filter(|r| r.contains(pos)) {
                    let row = (m.row - r.y) as usize;
                    let outcome = self.projects_popup.as_mut().map(|p| p.click(row));
                    if let Some(crate::projects::PopupOutcome::Request(req)) = outcome {
                        self.projects_popup = None;
                        self.ws_request = Some(req);
                    }
                } else {
                    self.projects_popup = None;
                }
                return;
            }
            if m.row == self.areas.top.y
                && let Some(h) = self.project_hits.iter().find(|h| (h.x.0..h.x.1).contains(&m.column))
            {
                match h.request(m.column) {
                    Some(req) => self.ws_request = Some(req),
                    None => self.handle_action(Action::Projects),
                }
                return;
            }
        }
        if let MouseEventKind::Down(MouseButton::Left) = m.kind
            && self.areas.reveal_button.is_some_and(|r| r.contains(pos))
            && self.find.is_none()
            && self.goto.is_none()
        {
            self.handle_action(Action::RevealFile);
            return;
        }
        let inside = |r: Option<Rect>| r.is_some_and(|r| r.contains(pos));
        let tree = self.areas.left_inner.filter(|_| self.left == Some(LeftPanel::Project));
        let viewer = self.areas.viewer;
        if let Some(r) = self
            .areas
            .commit_list
            .filter(|r| r.contains(pos) && self.left == Some(LeftPanel::Commit))
        {
            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                self.focus = Focus::Left;
                let row = (m.row - r.y) as usize;
                let col = (m.column - r.x) as usize;
                let i = self.commit.scroll + row;
                let checkbox_at = match self.commit.rows.get(i) {
                    Some(crate::commit::Row::File { depth, .. }) => 1 + depth * 2,
                    Some(_) => 3,
                    None => usize::MAX,
                };
                let on_box = (checkbox_at..checkbox_at.saturating_add(3)).contains(&col);
                self.commit.click(row, &self.status, on_box);
                if !on_box && self.is_double_click(ClickList::Commit, i) {
                    self.commit_key(&KeyEvent::from(KeyCode::Enter));
                }
            }
            return;
        }
        if self.areas.commit_message.is_some_and(|r| r.contains(pos)) && self.left == Some(LeftPanel::Commit) {
            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                self.focus = Focus::Left;
                self.commit.editing_message = true;
            }
            return;
        }
        if self.find.is_some() {
            if let Some(r) = self.areas.find_list.filter(|r| r.contains(pos)) {
                let row = (m.row - r.y) as usize;
                let double = matches!(m.kind, MouseEventKind::Down(MouseButton::Left))
                    && self.is_double_click(ClickList::FindPopup, row);
                let Some(p) = self.find.as_mut() else { return };
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => p.click(row),
                    MouseEventKind::ScrollDown => p.scroll_by(3),
                    MouseEventKind::ScrollUp => p.scroll_by(-3),
                    _ => {}
                }
                if double {
                    self.handle_key(KeyEvent::from(KeyCode::Enter));
                }
            }
            return;
        }
        if let Some(r) = self.areas.shell.filter(|r| r.contains(pos)) {
            if matches!(m.kind, MouseEventKind::Down(_)) {
                self.focus = Focus::Bottom;
            }
            if let Some(sh) = self.shell.as_mut() {
                sh.mouse(&m, m.column - r.x, m.row - r.y);
            }
            return;
        }
        for (rect, files) in [(self.areas.log_commits, false), (self.areas.log_files, true)] {
            let Some(r) = rect.filter(|r| r.contains(pos)) else {
                continue;
            };
            match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    self.focus = Focus::Bottom;
                    self.log.on_files = files;
                    let row = (m.row - r.y) as usize;
                    if files {
                        let i = self.log.file_scroll + row;
                        if i < self.log.current_files().len() {
                            self.log.file_selected = i;
                            if self.is_double_click(ClickList::LogFiles, i) {
                                let open = self.log.open_selected();
                                self.log_outcome(open);
                            }
                        }
                    } else {
                        let i = self.log.scroll + row;
                        if i < self.log.commits.len() {
                            let need = self.log.select_commit(i);
                            self.log_outcome(need);
                        }
                    }
                }
                MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                    let down = matches!(m.kind, MouseEventKind::ScrollDown);
                    let code = if down {
                        ratatui::crossterm::event::KeyCode::Down
                    } else {
                        ratatui::crossterm::event::KeyCode::Up
                    };
                    let saved = self.log.on_files;
                    self.log.on_files = files;
                    for _ in 0..3 {
                        let o = self.log.handle_key(&KeyEvent::from(code));
                        self.log_outcome(o);
                    }
                    self.log.on_files = saved;
                }
                _ => {}
            }
            return;
        }
        if let Some(r) = self.areas.find_window.filter(|r| r.contains(pos)) {
            let h = self.find_window.view_height;
            let mut open = false;
            if let Some(tab) = self.find_window.active_tab_mut() {
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        self.focus = Focus::Bottom;
                        let i = tab.scroll + (m.row - r.y) as usize;
                        if i < tab.rows.len() {
                            tab.selected = i;
                            open = true;
                        }
                    }
                    MouseEventKind::ScrollDown => tab.move_by(3, h),
                    MouseEventKind::ScrollUp => tab.move_by(-3, h),
                    _ => {}
                }
            }
            let row = (m.row - r.y) as usize;
            if open && self.is_double_click(ClickList::FindWindow, row) {
                self.handle_key(KeyEvent::from(KeyCode::Enter));
            }
            return;
        }
        if self.goto.is_some() {
            if let Some(r) = self.areas.goto_list.filter(|r| r.contains(pos)) {
                let row = (m.row - r.y) as usize;
                let double = matches!(m.kind, MouseEventKind::Down(MouseButton::Left))
                    && self.is_double_click(ClickList::Goto, row);
                let Some(g) = self.goto.as_mut() else { return };
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => g.click(row),
                    MouseEventKind::ScrollDown => g.scroll_by(3),
                    MouseEventKind::ScrollUp => g.scroll_by(-3),
                    _ => {}
                }
                if double {
                    self.handle_key(KeyEvent::from(KeyCode::Enter));
                }
            }
            return;
        }
        if let Some(r) = viewer.filter(|r| r.contains(pos))
            && self.viewer.active_tab().is_some_and(|t| t.editing)
            && let Some(n) = &self.nvim
        {
            editor::send_mouse(n, &m, m.row - r.y, m.column - r.x);
            if matches!(m.kind, MouseEventKind::Down(_)) {
                self.focus = Focus::Main;
            }
            return;
        }
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if inside(self.areas.tabs) => {
                self.focus = Focus::Main;
                match self.tab_at(m.column) {
                    Some((i, true)) => self.close_tab(i),
                    Some((i, false)) => {
                        self.viewer.activate(i);
                        self.sync_editor_buffer();
                        // Dragging from here moves the tab.
                        self.dragging_tab = Some(i);
                        self.drag_col = m.column;
                    }
                    None => {}
                }
            }
            MouseEventKind::Down(MouseButton::Left)
                if inside(viewer) && self.viewer.active_tab().is_some_and(|t| t.selectable()) =>
            {
                // Press and drag selects; a double click selects a word.
                self.focus = Focus::Main;
                let double = self.is_double_click(ClickList::Viewer, m.row as usize);
                if let Some(tab) = self.viewer.active_tab_mut()
                    && let Some(p) = tab.pos_at(m.column, m.row)
                {
                    if tab.has_cursor() && p.0 != tab.cursor {
                        tab.set_cursor(p.0);
                    }
                    if double {
                        tab.select_word(p);
                        if self.copy_on_select() {
                            self.copy_selection();
                        }
                    } else {
                        tab.selection = Some(crate::selection::Selection::at(p));
                        self.selecting = true;
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Left) if inside(viewer) => {
                self.focus = Focus::Main;
                let r = viewer.expect("checked");
                if let Some(tab) = self.viewer.active_tab_mut()
                    && tab.has_cursor()
                    && !tab.wrap
                {
                    let line = tab.scroll + (m.row - r.y) as usize;
                    tab.set_cursor(line);
                }
            }
            MouseEventKind::ScrollDown
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight
                if inside(viewer) =>
            {
                // Shift+wheel scrolls sideways, like most editors; trackpads
                // send left/right directly.
                let shift = m.modifiers.contains(KeyModifiers::SHIFT);
                if let Some(t) = self.viewer.active_tab_mut() {
                    match (m.kind, shift) {
                        (MouseEventKind::ScrollDown, false) => t.scroll_by(3),
                        (MouseEventKind::ScrollUp, false) => t.scroll_by(-3),
                        (MouseEventKind::ScrollRight, _) | (MouseEventKind::ScrollDown, true) => t.hscroll_by(6),
                        _ => t.hscroll_by(-6),
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if inside(tree) {
                    let r = tree.expect("checked");
                    self.focus = Focus::Left;
                    let outcome = self
                        .project
                        .click((m.row - r.y) as usize, (m.column - r.x) as usize, &self.status);
                    if let Outcome::Open(p) = outcome {
                        self.open_file(&p);
                    }
                } else if inside(self.areas.left) {
                    self.focus = Focus::Left;
                } else if self.areas.main.contains(pos) {
                    self.focus = Focus::Main;
                }
            }
            MouseEventKind::ScrollDown if inside(tree) => self.project.scroll_by(3),
            MouseEventKind::ScrollUp if inside(tree) => self.project.scroll_by(-3),
            _ => {}
        }
    }

    /// Opens `config.toml` for editing, written out with every default first
    /// when there is none.
    fn open_config_file(&mut self) {
        let path = self.paths.config_file();
        if !path.exists() {
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&path, vista_core::config::TEMPLATE));
            if let Err(e) = written {
                self.notify(format!("cannot create {}: {e}", path.display()), MsgKind::Error);
                return;
            }
        }
        self.open_at(&path, None, true);
        self.notify("Settings apply when vista starts again", MsgKind::Info);
    }

    // ----- preferences ------------------------------------------------------

    /// What a setting is now, as the Preferences page shows it.
    pub fn pref_value(&self, p: crate::prefs::Pref) -> String {
        use crate::prefs::Pref::*;
        let ui = self.ui.borrow();
        let on = |b: bool| if b { "On" } else { "Off" }.to_string();
        match p {
            Look if self.follows_herdr() => format!("Follow herdr ({})", self.theme.name),
            Look => self.theme.name.clone(),
            FillBackground => on(ui.looks.fill_background.unwrap_or(false)),
            LineNumbers => on(ui.viewer.line_numbers.unwrap_or(true)),
            WrapMarkdown => on(ui.viewer.wrap_markdown.unwrap_or(true)),
            WrapCode => on(ui.viewer.wrap_code.unwrap_or(false)),
            DiffUnified => on(ui.diff.unified.unwrap_or(false)),
            DiffCollapse => on(ui.diff.collapse.unwrap_or(false)),
            DiffWhitespace => on(ui.diff.ignore_whitespace.unwrap_or(false)),
            ShowIgnored => on(self.project.show_ignored),
            GroupByFolder => on(self.commit.by_dir),
            FindCase => on(ui.find.case.unwrap_or(false)),
            FindWords => on(ui.find.words.unwrap_or(false)),
            FindRegex => on(ui.find.regex.unwrap_or(false)),
            CopyOnSelect => on(ui.mouse.copy_on_select.unwrap_or(self.config.mouse.copy_on_select)),
            EditConfig => String::new(),
        }
    }

    /// Changes a setting from the Preferences page: on / off settings flip;
    /// the look steps forward or back. It applies at once and is remembered.
    pub fn change_pref(&mut self, p: crate::prefs::Pref, forward: bool) {
        use crate::prefs::Pref::*;
        let flip = |b: Option<bool>, default: bool| Some(!b.unwrap_or(default));
        match p {
            Look => self.step_look(forward),
            FillBackground => {
                self.remember(|s| s.looks.fill_background = flip(s.looks.fill_background, false));
                let id = self.theme.id.clone();
                if let Err(e) = self.set_theme(&id) {
                    self.notify(e, MsgKind::Error);
                }
            }
            LineNumbers => self.handle_action(Action::ToggleLineNumbers),
            WrapMarkdown | WrapCode => {
                let markdown = p == WrapMarkdown;
                let on = !self.wrap_default(markdown);
                self.set_wrap(markdown, on);
            }
            DiffUnified => self.remember(|s| s.diff.unified = flip(s.diff.unified, false)),
            DiffCollapse => self.remember(|s| s.diff.collapse = flip(s.diff.collapse, false)),
            DiffWhitespace => self.remember(|s| s.diff.ignore_whitespace = flip(s.diff.ignore_whitespace, false)),
            ShowIgnored => self.handle_action(Action::ToggleIgnored),
            GroupByFolder => {
                self.commit.by_dir = !self.commit.by_dir;
                let (on, root) = (self.commit.by_dir, self.root.clone());
                self.commit.rebuild(&self.status, &root);
                self.remember(|s| s.commit.group_by_folder = Some(on));
            }
            FindCase => self.remember(|s| s.find.case = flip(s.find.case, false)),
            FindWords => self.remember(|s| s.find.words = flip(s.find.words, false)),
            FindRegex => self.remember(|s| s.find.regex = flip(s.find.regex, false)),
            CopyOnSelect => {
                let on = !self.copy_on_select();
                self.remember(|s| s.mouse.copy_on_select = Some(on));
            }
            EditConfig => {
                self.prefs = None;
                self.open_config_file();
            }
        }
    }

    /// The next (or previous) look: Follow herdr first, then every look.
    fn step_look(&mut self, forward: bool) {
        let mut ids = vec![HERDR.to_string()];
        ids.extend(self.theme_loader.available());
        let current = if self.follows_herdr() {
            HERDR
        } else {
            self.theme.id.as_str()
        };
        let pos = ids.iter().position(|i| i == current).unwrap_or(0);
        let n = ids.len();
        let next = ids[if forward { (pos + 1) % n } else { (pos + n - 1) % n }].clone();
        let look = if next == HERDR {
            vista_core::herdr::current_theme()
        } else {
            next.clone()
        };
        self.remember(|s| s.look = Some(next.clone()));
        if let Err(e) = self.set_theme(&look) {
            self.notify(
                format!("herdr's theme \"{look}\" has no vista look ({e})"),
                MsgKind::Error,
            );
        }
    }

    /// A click on a label or button: does what its key does.
    pub fn click(&mut self, c: crate::ui::click::Click) {
        use crate::ui::click::Click;
        let tab = KeyEvent::from(KeyCode::Tab);
        match c {
            Click::Action(a) => self.handle_action(a),
            Click::Key(key, focus) => {
                if let Some(f) = focus {
                    self.focus = f;
                }
                self.handle_key(key);
            }
            // Scopes and tabs step round with their key until the clicked one.
            Click::FindScope(s) => {
                let alt_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT);
                for _ in crate::find::ScopeKind::ALL {
                    if self.find.as_ref().is_none_or(|p| p.scope == s) {
                        break;
                    }
                    self.find_key(&alt_s);
                }
            }
            Click::FindField(field) => {
                if self.find.as_ref().is_some_and(|p| p.field != field) {
                    self.find_key(&tab);
                }
            }
            Click::GotoTab(t) => {
                if let Some(outcome) = self.goto.as_mut().map(|g| g.set_tab(t)) {
                    self.goto_outcome(outcome);
                }
            }
            Click::FindWindowTab(i) => {
                if i < self.find_window.tabs.len() {
                    self.find_window.active = i;
                    self.focus = Focus::Bottom;
                }
            }
            Click::Pref(i) => {
                if let Some(p) = self.prefs.as_mut() {
                    p.selected = i;
                    let pref = p.selected_pref();
                    self.change_pref(pref, true);
                }
            }
            Click::Amend => {
                self.focus = Focus::Left;
                self.commit.editing_message = false;
                self.commit_key(&KeyEvent::from(KeyCode::Char('m')));
            }
        }
    }

    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => {
                let unsaved = self
                    .nvim
                    .as_ref()
                    .and_then(|n| n.modified_buffers().ok())
                    .unwrap_or_default();
                let confirmed = self.quit_confirm.is_some_and(|t| t.elapsed() < Duration::from_secs(4));
                if unsaved.is_empty() || confirmed {
                    self.should_quit = true;
                } else {
                    self.quit_confirm = Some(Instant::now());
                    self.notify(
                        format!(
                            "{} file(s) with unsaved changes in the editor; press q again to discard",
                            unsaved.len()
                        ),
                        MsgKind::Error,
                    );
                }
            }
            Action::Edit => self.edit_action(),
            Action::Commit => {
                self.left = Some(LeftPanel::Commit);
                self.focus = Focus::Left;
                self.commit.editing_message = true;
            }
            Action::GotoFile => self.open_goto(GotoTab::Files),
            Action::FindInFiles => {
                let q = self
                    .viewer
                    .active_tab()
                    .and_then(|t| t.search.as_ref().map(|s| s.query.clone()))
                    .filter(|q| !q.is_empty())
                    .or_else(|| self.find_history.first().cloned())
                    .unwrap_or_default();
                self.open_find(q);
            }
            Action::ToggleFind => {
                self.log.visible = false;
                self.shell_visible = false;
                if self.find_window.tabs.is_empty() {
                    self.notify("No Find results yet (Ctrl-F, then Alt-O)", MsgKind::Info);
                } else if self.find_window.visible && self.focus == Focus::Bottom {
                    self.find_window.visible = false;
                    self.focus = Focus::Main;
                } else {
                    self.find_window.visible = true;
                    self.focus = Focus::Bottom;
                }
            }
            Action::NextOccurrence => self.step_occurrence(true),
            Action::PrevOccurrence => self.step_occurrence(false),
            Action::LeaveEditor => self.leave_editor(),
            Action::Help => self.show_help = true,
            Action::FocusNext | Action::FocusPrev => self.cycle_focus(),
            Action::ToggleProject => self.toggle_left(LeftPanel::Project),
            Action::ToggleCommit => self.toggle_left(LeftPanel::Commit),
            Action::CycleTheme => self.cycle_theme(),
            Action::OpenSettings => self.prefs = Some(crate::prefs::PrefsPage::default()),
            Action::ToggleIgnored => {
                self.project.show_ignored = !self.project.show_ignored;
                let shown = self.project.show_ignored;
                self.remember(|s| s.tree.show_ignored = Some(shown));
                self.project.rebuild(&self.status);
                let state = if self.project.show_ignored { "shown" } else { "hidden" };
                self.notify(format!("Ignored files {state}"), MsgKind::Info);
            }
            Action::CollapseAll => self.project.collapse_all(&self.status),
            Action::Refresh => {
                self.project.tree.refresh_all();
                self.project.rebuild(&self.status);
                self.request_status();
                let paths: Vec<PathBuf> = self.viewer.tabs.iter().map(|t| t.path.clone()).collect();
                let ctx = self.load_ctx();
                self.viewer.reload_paths(&paths, &ctx);
            }
            Action::CloseTab if self.focus == Focus::Bottom => {
                self.find_window.close_active();
                if !self.find_window.visible {
                    self.focus = Focus::Main;
                }
            }
            Action::CloseTab => {
                if let Some(i) = self.viewer.active {
                    self.close_tab(i);
                }
            }
            Action::NextTab => {
                self.viewer.cycle(1);
                self.sync_editor_buffer();
            }
            Action::PrevTab => {
                self.viewer.cycle(-1);
                self.sync_editor_buffer();
            }
            Action::RecentFiles => self.recent_popup = Some(RecentPopup::new(self.recent.list.len())),
            Action::ToggleTerminal => {
                if self.shell_visible && self.focus == Focus::Bottom {
                    self.shell_visible = false;
                    self.focus = Focus::Main;
                } else {
                    self.show_shell();
                }
            }
            Action::ToggleLog => {
                if self.log.visible && self.focus == Focus::Bottom {
                    self.log.visible = false;
                    self.focus = Focus::Main;
                } else {
                    let path = self.log.path.clone();
                    self.show_log(path);
                }
            }
            Action::FileHistory => match self.viewer.active_tab().map(|t| t.path.clone()) {
                Some(path) => self.show_log(Some(path)),
                None => self.notify("No file open", MsgKind::Info),
            },
            Action::Projects => self.projects_popup = Some(crate::projects::ProjectsPopup::new(self.projects.clone())),
            Action::NextProject => self.ws_request = Some(crate::projects::WsRequest::Cycle(true)),
            Action::PrevProject => self.ws_request = Some(crate::projects::WsRequest::Cycle(false)),
            Action::GrowPanel | Action::ShrinkPanel => match self.areas.left {
                Some(left) => {
                    // From the stored width, not the last frame: several key
                    // repeats can arrive between two draws.
                    let max = (left.width + self.areas.main.width).saturating_sub(crate::ui::MIN_MAIN);
                    let current = self.ui.borrow().layout.left_width.unwrap_or(left.width).min(max);
                    let w = if action == Action::GrowPanel {
                        current.saturating_add(4).min(max)
                    } else {
                        current.saturating_sub(4)
                    };
                    self.set_left_width(w, true);
                }
                None => self.notify("No tool window open (Alt-1, Alt-2)", MsgKind::Info),
            },
            Action::ToggleLineNumbers => {
                let on = !self.line_numbers();
                self.remember(|s| s.viewer.line_numbers = Some(on));
                self.notify(if on { "Line numbers on" } else { "Line numbers off" }, MsgKind::Info);
            }
            Action::RevealFile => match self.viewer.active_tab().map(|t| t.path.clone()) {
                Some(path) => {
                    self.left = Some(LeftPanel::Project);
                    self.focus = Focus::Left;
                    self.project.reveal(&path, &self.status);
                }
                None => self.notify("No file open", MsgKind::Info),
            },
            Action::NextChange | Action::PrevChange => {
                let forward = action == Action::NextChange;
                match self.viewer.active_tab_mut().map(|t| t.jump_to_change(forward)) {
                    Some(true) => self.change_hop = None,
                    Some(false) => self.hop_to_changed_file(forward),
                    None => {}
                }
            }
        }
    }

    /// Opens a file in the main area and focuses it.
    pub fn open_file(&mut self, path: &Path) {
        let ctx = self.load_ctx();
        self.viewer.open(path, &ctx);
        self.recent.push(path);
        self.focus = Focus::Main;
    }

    /// Which tab is at screen column `x`, and whether on its × (from the
    /// last draw of the tab strip).
    fn tab_at(&self, x: u16) -> Option<(usize, bool)> {
        self.tab_hits.iter().enumerate().find_map(|(i, h)| {
            (h.label.0..h.label.1)
                .contains(&x)
                .then(|| (i, (h.close.0..h.close.1).contains(&x)))
        })
    }

    /// Closes tab `i`; Neovim drops the side-by-side diff if that tab had it.
    fn close_tab(&mut self, i: usize) {
        let diff_editing = self.viewer.tabs.get(i).is_some_and(|t| t.editing && t.diff.is_some());
        self.viewer.close(i);
        if diff_editing && let Some(n) = &self.nvim {
            let _ = n.close_diff();
        }
        self.sync_editor_buffer();
        if self.viewer.active.is_none() && self.left.is_some() {
            self.focus = Focus::Left;
        }
    }

    fn prompt_key(&mut self, key: &KeyEvent) {
        use ratatui::crossterm::event::KeyCode;
        let Some(p) = self.prompt.as_mut() else { return };
        match key.code {
            KeyCode::Esc => self.prompt = None,
            KeyCode::Backspace => {
                p.input.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => p.input.push(c),
            KeyCode::Enter => {
                let n: usize = p.input.parse().unwrap_or(0);
                self.prompt = None;
                if let Some(t) = self.viewer.active_tab_mut()
                    && n > 0
                {
                    t.set_cursor(n - 1);
                    let h = t.view_height.max(1);
                    t.scroll = (n - 1).saturating_sub(h / 2);
                }
            }
            _ => {}
        }
    }

    fn cycle_focus(&mut self) {
        let bottom =
            (self.find_window.visible && !self.find_window.tabs.is_empty()) || self.log.visible || self.shell_visible;
        self.focus = match (self.focus, self.left, bottom) {
            (Focus::Left, _, _) => Focus::Main,
            (Focus::Main, _, true) => Focus::Bottom,
            (Focus::Main, Some(_), false) | (Focus::Bottom, Some(_), _) => Focus::Left,
            (Focus::Main, None, false) | (Focus::Bottom, None, _) => Focus::Main,
        };
    }

    /// IntelliJ semantics: activating the visible, focused tool window hides
    /// it; otherwise it is shown and focused.
    fn toggle_left(&mut self, panel: LeftPanel) {
        if self.left == Some(panel) && self.focus == Focus::Left {
            self.left = None;
            self.focus = Focus::Main;
        } else {
            self.left = Some(panel);
            self.focus = Focus::Left;
        }
    }

    // ----- looks -----------------------------------------------------------

    pub fn set_theme(&mut self, id: &str) -> Result<(), String> {
        let filled = self.fill_background();
        let theme = load_look(&self.theme_loader, id, filled).map_err(|e| e.to_string())?;
        self.theme_stamp = stamp(self.theme_loader.user_path(id).as_deref());
        self.theme = theme;
        self.theme_filled = filled;
        Ok(())
    }

    /// The look follows herdr's theme: `herdr` was picked (or configured),
    /// and `--theme` / `VISTA_THEME` didn't choose another.
    pub fn follows_herdr(&self) -> bool {
        let picked = self.ui.borrow().look.clone();
        !self.theme_forced && picked.as_deref().unwrap_or(&self.config.theme) == HERDR
    }

    /// Paint looks' own background where they'd use the terminal's.
    pub fn fill_background(&self) -> bool {
        self.ui.borrow().looks.fill_background.unwrap_or(false)
    }

    /// Copy a mouse selection on release: remembered, else `[mouse] copy_on_select`.
    pub fn copy_on_select(&self) -> bool {
        let remembered = self.ui.borrow().mouse.copy_on_select;
        remembered.unwrap_or(self.config.mouse.copy_on_select)
    }

    fn cycle_theme(&mut self) {
        let ids = self.theme_loader.available();
        let pos = ids.iter().position(|i| *i == self.theme.id).unwrap_or(0);
        let next = ids[(pos + 1) % ids.len()].clone();
        match self.set_theme(&next) {
            Ok(()) => {
                self.remember(|s| s.look = Some(next.clone()));
                self.notify(format!("Look: {}", self.theme.name), MsgKind::Info);
            }
            Err(e) => self.notify(e, MsgKind::Error),
        }
    }

    /// Periodic housekeeping: theme hot reload, expiring status messages.
    pub fn tick(&mut self) {
        self.last_tick = Instant::now();
        self.reload_theme_if_changed();
        if self.change_hop.as_ref().is_some_and(|h| h.at.elapsed() > HOP_TTL) {
            self.change_hop = None;
        }
        if self.message.as_ref().is_some_and(|s| s.at.elapsed() > STATUS_TTL) {
            self.message = None;
        }
    }

    pub fn tick_due(&self, every: Duration) -> bool {
        self.last_tick.elapsed() >= every
    }

    fn reload_theme_if_changed(&mut self) {
        let path = self.theme_loader.user_path(&self.theme.id);
        let now = stamp(path.as_deref());
        if now == self.theme_stamp {
            return;
        }
        self.theme_stamp = now;
        let id = self.theme.id.clone();
        match load_look(&self.theme_loader, &id, self.theme_filled) {
            Ok(t) => {
                self.theme = t;
                self.notify(format!("Reloaded look `{id}`"), MsgKind::Info);
            }
            Err(e) => self.notify(e.to_string(), MsgKind::Error),
        }
    }
}

/// The image encoder for a configured protocol, without querying the
/// terminal (`auto` is resolved by [`crate::run`] once the terminal is up).
pub fn default_picker(protocol: ImageProtocol) -> Option<Picker> {
    use ratatui_image::picker::ProtocolType;
    match protocol {
        ImageProtocol::Off => None,
        ImageProtocol::Halfblocks | ImageProtocol::Auto => Some(Picker::halfblocks()),
        ImageProtocol::Kitty => {
            #[allow(deprecated)]
            let mut p = Picker::from_fontsize(ratatui_image::FontSize::new(8, 16));
            p.set_protocol_type(ProtocolType::Kitty);
            Some(p)
        }
    }
}

/// Loads look `id`, its "terminal" colours filled in when `filled`.
fn load_look(loader: &ThemeLoader, id: &str, filled: bool) -> Result<Theme, vista_theme::ThemeError> {
    let mut theme = loader.load(id)?;
    if filled {
        theme.fill_terminal();
    }
    Ok(theme)
}

fn stamp(path: Option<&Path>) -> Option<SystemTime> {
    path.and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_at;
    use ratatui::crossterm::event::KeyCode;

    fn app() -> (tempfile::TempDir, App) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("p");
        std::fs::create_dir_all(&root).unwrap();
        let app = app_at(&root, tmp.path(), None);
        (tmp, app)
    }

    #[test]
    fn dragging_the_divider_resizes_and_saves_on_release() {
        let (t, mut app) = app();
        crate::testing::render(&mut app, 120, 30);
        let sep = app.areas.separator.expect("darcula has a separator").x;
        let mouse = |app: &mut App, kind: MouseEventKind, col: u16| {
            app.handle_mouse(MouseEvent {
                kind,
                column: col,
                row: 10,
                modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
            });
        };
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), sep);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), sep + 5);
        mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), sep + 10);
        let want = sep + 10 - app.areas.left.unwrap().x;
        assert_eq!(app.ui.borrow().layout.left_width, Some(want));
        let state = t.path().join("config/state.toml");
        assert!(!state.exists(), "not saved while dragging");
        mouse(&mut app, MouseEventKind::Up(MouseButton::Left), sep + 10);
        let text = std::fs::read_to_string(&state).unwrap();
        assert!(text.contains(&format!("left_width = {want}")), "{text}");
        // The next frame puts the divider where the mouse let go.
        crate::testing::render(&mut app, 120, 30);
        assert_eq!(app.areas.separator.unwrap().x, sep + 10);
    }

    #[test]
    fn clicking_project_tabs_asks_the_workspace() {
        use crate::projects::WsRequest;
        let (t, mut app) = app();
        app.projects = crate::projects::ProjectsBar {
            open: vec![
                ("alpha".into(), t.path().join("a")),
                ("beta".into(), t.path().join("b")),
            ],
            active: 1,
            recent: vec![],
        };
        crate::testing::render(&mut app, 120, 30);
        let click = |app: &mut App, col: u16| {
            app.handle_mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: col,
                row: 0,
                modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
            });
            app.ws_request.take()
        };
        // " alpha × " starts at column 1, " beta × " after it.
        assert_eq!(click(&mut app, 2), Some(WsRequest::Switch(0)), "{:?}", app.project_hits);
        assert_eq!(click(&mut app, 8), Some(WsRequest::Close(0)), "{:?}", app.project_hits);
        let plus = app.project_hits.last().unwrap().x.0;
        assert_eq!(click(&mut app, plus + 1), None);
        assert!(app.projects_popup.is_some(), "+ opens the Projects popup");
    }

    #[test]
    fn starts_with_project_focused() {
        let (_t, app) = app();
        assert_eq!(app.left, Some(LeftPanel::Project));
        assert_eq!(app.focus, Focus::Left);
        assert_eq!(app.project_name, "p");
        assert_eq!(app.theme.id, "darcula");
    }

    #[test]
    fn tool_window_toggling_follows_intellij() {
        let (_t, mut app) = app();
        app.handle_action(Action::ToggleProject);
        assert_eq!((app.left, app.focus), (None, Focus::Main), "visible+focused => hide");
        app.handle_action(Action::ToggleProject);
        assert_eq!((app.left, app.focus), (Some(LeftPanel::Project), Focus::Left));
        app.handle_action(Action::FocusNext);
        assert_eq!(app.focus, Focus::Main);
        app.handle_action(Action::ToggleProject);
        assert_eq!(
            (app.left, app.focus),
            (Some(LeftPanel::Project), Focus::Left),
            "visible but unfocused => focus"
        );
        app.handle_action(Action::ToggleCommit);
        assert_eq!(app.left, Some(LeftPanel::Commit));
    }

    #[test]
    fn focus_stays_on_main_without_a_tool_window() {
        let (_t, mut app) = app();
        app.handle_action(Action::ToggleProject);
        app.handle_action(Action::FocusNext);
        assert_eq!(app.focus, Focus::Main);
    }

    #[test]
    fn help_swallows_the_next_key() {
        let (_t, mut app) = app();
        app.handle_action(Action::Help);
        assert!(app.show_help);
        app.handle_key(KeyEvent::from(KeyCode::Char('q')));
        assert!(!app.show_help);
        assert!(!app.should_quit);
    }

    #[test]
    fn status_messages_expire() {
        let (_t, mut app) = app();
        app.notify("hello", MsgKind::Info);
        app.message.as_mut().unwrap().at -= STATUS_TTL + Duration::from_secs(1);
        app.tick();
        assert!(app.message.is_none());
    }

    #[test]
    fn status_requests_are_coalesced() {
        let (_t, mut app) = app();
        app.git = Some(Git {
            toplevel: app.root.clone(),
        });
        app.request_status();
        app.request_status();
        app.request_status();
        assert!(app.status_job.running && app.status_job.pending);
        // First result arrives (git fails: not a repo) and triggers exactly one follow-up.
        let ev = app.recv_timeout(Duration::from_secs(5)).unwrap();
        app.handle_event(ev);
        assert!(app.status_job.running && !app.status_job.pending);
    }

    #[test]
    fn rejects_files_as_root() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("file.txt");
        std::fs::write(&f, "").unwrap();
        let paths = Paths {
            config_dir: tmp.path().join("c"),
            cache_dir: tmp.path().join("cache"),
            log_file: tmp.path().join("log"),
        };
        let res = App::new(AppOptions {
            events: None,
            ui: None,
            root: f,
            config: Config::default(),
            paths,
            theme: None,
        });
        assert!(res.is_err());
    }
}
