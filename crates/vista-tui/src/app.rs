//! Application state and event handling. Drawing lives in [`crate::ui`].

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use ratatui::crossterm::event::{Event, KeyEvent, KeyEventKind, MouseEvent};
use vista_core::{Config, Paths};
use vista_theme::{Theme, ThemeLoader};

use crate::event::AppEvent;
use crate::keymap::{Action, Keymap};

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
    pub status: Option<StatusMsg>,
    pub tx: Sender<AppEvent>,
    pub rx: Receiver<AppEvent>,
    /// Screen regions from the last draw (mouse hit-testing).
    pub areas: crate::ui::Areas,
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
            status: None,
            tx,
            rx,
            areas: Default::default(),
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

    /// Called once the terminal is ready, before the first event.
    pub fn on_start(&mut self) {}

    /// Called after the main loop ends, while the terminal is still set up.
    pub fn on_exit(&mut self) {}

    pub fn notify(&mut self, text: impl Into<String>, kind: MsgKind) {
        let text = text.into();
        match kind {
            MsgKind::Error => tracing::warn!("{text}"),
            MsgKind::Info => tracing::info!("{text}"),
        }
        self.status = Some(StatusMsg {
            text,
            kind,
            at: Instant::now(),
        });
    }

    pub fn handle_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Term(Event::Key(key)) => {
                if key.kind != KeyEventKind::Release {
                    self.handle_key(key);
                }
            }
            AppEvent::Term(Event::Mouse(m)) => self.handle_mouse(m),
            AppEvent::Term(_) => {}
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
        if let Some(action) = self.keymap.action_for(&key) {
            self.handle_action(action);
        }
    }

    pub fn handle_mouse(&mut self, _m: MouseEvent) {}

    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Help => self.show_help = true,
            Action::FocusNext | Action::FocusPrev => self.cycle_focus(),
            Action::ToggleProject => self.toggle_left(LeftPanel::Project),
            Action::ToggleCommit => self.toggle_left(LeftPanel::Commit),
            Action::CycleTheme => self.cycle_theme(),
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
        if self.status.as_ref().is_some_and(|s| s.at.elapsed() > STATUS_TTL) {
            self.status = None;
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
        app.status.as_mut().unwrap().at -= STATUS_TTL + Duration::from_secs(1);
        app.tick();
        assert!(app.status.is_none());
    }
}
