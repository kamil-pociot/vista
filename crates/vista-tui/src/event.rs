//! Events flowing into the app's single event loop.

use ratatui::crossterm::event::Event;

/// Everything the main loop reacts to. Workers (git, search, highlighting,
/// Neovim) post their results here so the UI thread never blocks.
#[derive(Debug)]
pub enum AppEvent {
    /// Input or resize from the terminal.
    Term(Event),
}
