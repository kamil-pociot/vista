//! Application state and event handling. Drawing lives in [`crate::ui`].

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use ratatui::crossterm::event::{Event, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use vista_core::{Config, FsChange, Paths, Watcher};
use vista_git::{Git, StatusSnapshot};
use vista_theme::{Theme, ThemeLoader};

use crate::editor;
use crate::event::AppEvent;
use crate::goto::{GotoOutcome, GotoPopup, GotoTab};
use crate::keymap::{Action, Keymap};
use crate::project::{Outcome, ProjectView};
use crate::recent::{PopupOutcome, Prompt, Recent, RecentPopup};
use crate::viewer::{Content, ImageState, LoadCtx, MdImages, Viewer, ViewerOutcome};
use ratatui_image::picker::Picker;
use std::collections::HashSet;
use std::sync::Arc;
use vista_core::ImageProtocol;
use vista_nvim::{Grid, Nvim, NvimEvent};
use vista_search::FileIndex;

/// Which panel has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The left tool window (Project or Commit).
    Left,
    /// The main area (viewer / editor / diff).
    Main,
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

pub struct AppOptions {
    pub root: PathBuf,
    pub config: Config,
    pub paths: Paths,
    /// `--theme` on the command line wins over the config file.
    pub theme: Option<String>,
}

/// Background `git status` runs: at most one at a time; requests that arrive
/// meanwhile are coalesced into one follow-up run.
#[derive(Debug, Default)]
struct StatusJob {
    running: bool,
    pending: bool,
}

pub struct App {
    pub root: PathBuf,
    pub project_name: String,
    pub config: Config,
    pub paths: Paths,
    pub theme: Theme,
    theme_loader: ThemeLoader,
    theme_stamp: Option<SystemTime>,
    pub keymap: Keymap,
    pub focus: Focus,
    pub left: Option<LeftPanel>,
    pub show_help: bool,
    pub should_quit: bool,
    /// Transient message in the status bar.
    pub message: Option<StatusMsg>,
    pub tx: Sender<AppEvent>,
    pub rx: Receiver<AppEvent>,
    /// Screen regions from the last draw (mouse hit-testing).
    pub areas: crate::ui::Areas,
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
    /// A command to run with the terminal handed over (`$EDITOR` fallback).
    pending_external: Option<(String, PathBuf)>,
    pub goto: Option<GotoPopup>,
    /// All project files for Go to File (built in the background).
    pub file_index: Option<Arc<FileIndex>>,
    index_job: bool,
    index_dirty: bool,
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
        let theme_id = opts.theme.clone().unwrap_or_else(|| opts.config.theme.clone());
        let mut startup_error = None;
        let theme = match theme_loader.load(&theme_id) {
            Ok(t) => t,
            Err(e) => {
                startup_error = Some(format!("{e}; using darcula"));
                Theme::default_theme()
            }
        };
        let theme_stamp = stamp(theme_loader.user_path(&theme.id).as_deref());
        let (keymap, warnings) = Keymap::with_overrides(&opts.config.keys);
        let (tx, rx) = mpsc::channel();
        let project = ProjectView::new(&root, opts.config.tree.show_ignored);
        let git = Git::discover(&root).ok();
        let recent = Recent::load(&opts.paths.cache_dir, &root);
        let picker = default_picker(opts.config.images.protocol);
        let mut app = App {
            root,
            project_name,
            config: opts.config,
            paths: opts.paths,
            theme,
            theme_loader,
            theme_stamp,
            keymap,
            focus: Focus::Left,
            left: Some(LeftPanel::Project),
            show_help: false,
            should_quit: false,
            message: None,
            tx,
            rx,
            areas: Default::default(),
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
            pending_external: None,
            goto: None,
            file_index: None,
            index_job: false,
            index_dirty: true,
            last_tick: Instant::now(),
        };
        if let Some(w) = warnings.first() {
            app.notify(format!("config [keys]: {}", w.0), MsgKind::Error);
        }
        if let Some(e) = startup_error {
            app.notify(e, MsgKind::Error);
        }
        Ok(app)
    }

    /// Called once the terminal is ready, before the first event: starts the
    /// file watcher and the first `git status`.
    pub fn on_start(&mut self) {
        vista_render::Highlighter::preload();
        self.request_index();
        if std::env::var_os("VISTA_NO_WATCH").is_none() {
            let tx = self.tx.clone();
            match Watcher::new(&self.root, Duration::from_millis(120), move |c| {
                let _ = tx.send(AppEvent::Fs(c));
            }) {
                Ok(w) => self.watcher = Some(w),
                Err(e) => self.notify(format!("file watching unavailable: {e}"), MsgKind::Error),
            }
        }
        self.request_status();
    }

    /// Called after the main loop ends, while the terminal is still set up.
    pub fn on_exit(&mut self) {
        self.watcher = None;
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
        let target = match self.focus {
            Focus::Left if self.left == Some(LeftPanel::Project) => self
                .project
                .selected_row()
                .filter(|r| !r.is_dir)
                .map(|r| (r.path.clone(), None)),
            _ => self
                .viewer
                .active_tab()
                .filter(|t| !t.editing)
                .map(|t| (t.path.clone(), t.has_cursor().then_some(t.cursor + 1))),
        };
        if let Some((path, line)) = target {
            self.start_editing(&path, line);
        }
    }

    /// Opens `path` in the embedded Neovim (starting it if needed), or hands
    /// the terminal to `$EDITOR` when Neovim isn't available.
    pub fn start_editing(&mut self, path: &Path, line: Option<usize>) {
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
                    return;
                }
            }
        }
        match self.viewer.position(path) {
            Some(i) => self.viewer.activate(i),
            None => self.open_file(path),
        }
        let res = self.nvim.as_ref().map(|n| n.open(path, line));
        if let Some(Err(e)) = res {
            self.notify(format!("editor: {e}"), MsgKind::Error);
            return;
        }
        if let Some(t) = self.viewer.active_tab_mut() {
            t.editing = true;
        }
        self.focus = Focus::Main;
    }

    fn leave_editor(&mut self) {
        if self.left.is_none() {
            self.left = Some(LeftPanel::Project);
        }
        self.focus = Focus::Left;
    }

    /// Keeps Neovim on the buffer of the active tab.
    fn sync_editor_buffer(&mut self) {
        if let (Some(n), Some(t)) = (&self.nvim, self.viewer.active_tab())
            && t.editing
        {
            let _ = n.open(&t.path, None);
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

    /// Opens Find in Files with a query (replaced by the real panel in M5).
    pub fn open_find(&mut self, query: String) {
        let _ = query;
        self.notify("Find in Files is not available yet", MsgKind::Info);
    }

    fn goto_key(&mut self, key: &KeyEvent) {
        let root = self.root.clone();
        let Some(g) = self.goto.as_mut() else { return };
        match g.handle_key(key, &root) {
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
        }
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
            AppEvent::Term(Event::Paste(text)) => {
                if self.editing_focused()
                    && let Some(n) = &self.nvim
                {
                    let _ = n.paste(&text);
                }
            }
            AppEvent::Term(_) => {}
            AppEvent::Nvim(e) => self.on_nvim(e),
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
    pub fn drain_events(&mut self) -> usize {
        let mut n = 0;
        while let Ok(ev) = self.rx.try_recv() {
            self.handle_event(ev);
            n += 1;
        }
        n
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.show_help {
            // Any key closes help; the key itself is consumed.
            self.show_help = false;
            return;
        }
        if self.goto.is_some() {
            self.goto_key(&key);
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
        if self.editing_focused() {
            match self.keymap.action_for(&key) {
                Some(Action::LeaveEditor) => self.leave_editor(),
                Some(a @ (Action::ToggleProject | Action::ToggleCommit | Action::ToggleFind)) => self.handle_action(a),
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
                ViewerOutcome::Handled => return,
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

    pub fn handle_mouse(&mut self, m: MouseEvent) {
        let pos = Position::new(m.column, m.row);
        let inside = |r: Option<Rect>| r.is_some_and(|r| r.contains(pos));
        let tree = self.areas.left_inner.filter(|_| self.left == Some(LeftPanel::Project));
        let viewer = self.areas.viewer;
        if let Some(g) = self.goto.as_mut() {
            if let Some(r) = self.areas.goto_list.filter(|r| r.contains(pos)) {
                match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => g.click((m.row - r.y) as usize),
                    MouseEventKind::ScrollDown => g.scroll_by(3),
                    MouseEventKind::ScrollUp => g.scroll_by(-3),
                    _ => {}
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
                if let Some(i) = self.tab_at(m.column) {
                    self.viewer.activate(i);
                    self.sync_editor_buffer();
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
            MouseEventKind::ScrollDown if inside(viewer) => {
                if let Some(t) = self.viewer.active_tab_mut() {
                    t.scroll_by(3);
                }
            }
            MouseEventKind::ScrollUp if inside(viewer) => {
                if let Some(t) = self.viewer.active_tab_mut() {
                    t.scroll_by(-3);
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
            Action::GotoFile => self.open_goto(GotoTab::Files),
            Action::LeaveEditor => self.leave_editor(),
            Action::Help => self.show_help = true,
            Action::FocusNext | Action::FocusPrev => self.cycle_focus(),
            Action::ToggleProject => self.toggle_left(LeftPanel::Project),
            Action::ToggleCommit => self.toggle_left(LeftPanel::Commit),
            Action::CycleTheme => self.cycle_theme(),
            Action::ToggleIgnored => {
                self.project.show_ignored = !self.project.show_ignored;
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
            Action::CloseTab => {
                if let Some(i) = self.viewer.active {
                    self.viewer.close(i);
                }
                if self.viewer.active.is_none() && self.left.is_some() {
                    self.focus = Focus::Left;
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
            Action::RevealFile => {
                if let Some(path) = self.viewer.active_tab().map(|t| t.path.clone()) {
                    self.left = Some(LeftPanel::Project);
                    self.focus = Focus::Left;
                    self.project.reveal(&path, &self.status);
                }
            }
            Action::NextChange | Action::PrevChange => {
                let forward = action == Action::NextChange;
                if let Some(t) = self.viewer.active_tab_mut()
                    && !t.jump_to_change(forward)
                {
                    self.notify("No more changes", MsgKind::Info);
                }
            }
            _ => {}
        }
    }

    /// Opens a file in the main area and focuses it.
    pub fn open_file(&mut self, path: &Path) {
        let ctx = self.load_ctx();
        self.viewer.open(path, &ctx);
        self.recent.push(path);
        self.focus = Focus::Main;
    }

    /// Which tab label is at screen column `x` (mirrors `chrome::tab_strip`).
    fn tab_at(&self, x: u16) -> Option<usize> {
        let strip = self.areas.tabs?;
        let gap = usize::from(self.theme.chrome.tabs != vista_theme::Tabs::Underline);
        let mut col = strip.x as usize;
        for (i, t) in self.viewer.tabs.iter().enumerate() {
            let name = t
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let w = unicode_width::UnicodeWidthStr::width(name.as_str()) + 2;
            if (col..col + w).contains(&(x as usize)) {
                return Some(i);
            }
            col += w + gap;
        }
        None
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
        self.focus = match (self.focus, self.left) {
            (Focus::Left, _) => Focus::Main,
            (Focus::Main, Some(_)) => Focus::Left,
            (Focus::Main, None) => Focus::Main,
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
        let theme = self.theme_loader.load(id).map_err(|e| e.to_string())?;
        self.theme_stamp = stamp(self.theme_loader.user_path(id).as_deref());
        self.theme = theme;
        Ok(())
    }

    fn cycle_theme(&mut self) {
        let ids = self.theme_loader.available();
        let pos = ids.iter().position(|i| *i == self.theme.id).unwrap_or(0);
        let next = ids[(pos + 1) % ids.len()].clone();
        match self.set_theme(&next) {
            Ok(()) => self.notify(format!("Look: {}", self.theme.name), MsgKind::Info),
            Err(e) => self.notify(e, MsgKind::Error),
        }
    }

    /// Periodic housekeeping: theme hot reload, expiring status messages.
    pub fn tick(&mut self) {
        self.last_tick = Instant::now();
        self.reload_theme_if_changed();
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
        match self.theme_loader.load(&id) {
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
        let ev = app.rx.recv_timeout(Duration::from_secs(5)).unwrap();
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
            root: f,
            config: Config::default(),
            paths,
            theme: None,
        });
        assert!(res.is_err());
    }
}
