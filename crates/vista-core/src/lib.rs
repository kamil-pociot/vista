//! Core, UI-agnostic pieces of vista: configuration, paths, the file tree and
//! file watching.

pub mod config;
pub mod state;
pub mod tree;
pub mod watch;

pub use config::{Config, EditorConfigMode, ImageProtocol, Paths};
pub use state::UiState;
pub use tree::{FileTree, Row};
pub use watch::{FsChange, Watcher};
