//! vista: a lean, IntelliJ-style terminal file browser.

pub mod app;
pub mod args;
pub mod commit;
pub mod diffview;
pub mod editor;
pub mod event;
pub mod find;
pub mod goto;
pub mod keymap;
pub mod log;
pub mod prefs;
pub mod probe;
pub mod project;
pub mod projects;
pub mod recent;
pub mod run;
pub mod selection;
pub mod style;
pub mod term_tab;
pub mod terminal;
pub mod testing;
pub mod ui;
pub mod viewer;
pub mod workspace;

pub use app::{App, AppOptions};
pub use workspace::Workspace;
