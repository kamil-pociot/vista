//! Core, UI-agnostic pieces of vista: configuration, paths, the file tree and
//! file watching.

pub mod config;

pub use config::{Config, EditorConfigMode, ImageProtocol, Paths};
