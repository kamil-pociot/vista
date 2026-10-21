//! Events flowing into the app's single event loop.

use ratatui::crossterm::event::Event;
use vista_core::FsChange;
use vista_git::StatusSnapshot;

/// Everything the main loop reacts to. Workers (git, search, highlighting,
/// Neovim) post their results here so the UI thread never blocks.
pub enum AppEvent {
    /// Input or resize from the terminal.
    Term(Event),
    /// A background `git status` finished.
    GitStatus(Result<Box<StatusSnapshot>, String>),
    /// Files changed on disk.
    Fs(FsChange),
    /// Highlighting and gutter markers for an open tab are ready.
    Loaded {
        tab: u64,
        generation: u64,
        loaded: Box<crate::viewer::Loaded>,
    },
    /// An image tab finished decoding.
    ImageDecoded {
        tab: u64,
        generation: u64,
        image: Result<Box<image::DynamicImage>, String>,
    },
    /// Something from the embedded Neovim.
    Nvim(vista_nvim::NvimEvent),
    /// An image referenced from Markdown finished decoding.
    MdImageDecoded {
        path: std::path::PathBuf,
        image: Result<Box<image::DynamicImage>, String>,
    },
}
