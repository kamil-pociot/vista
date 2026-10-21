//! vista: a lean, IntelliJ-style terminal file browser.

pub mod app;
pub mod args;
pub mod event;
pub mod keymap;
pub mod probe;
pub mod project;
pub mod run;
pub mod style;
pub mod terminal;
pub mod testing;
pub mod ui;

pub use app::{App, AppOptions};
