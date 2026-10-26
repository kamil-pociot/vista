//! Events flowing into the single event loop. All projects share one
//! channel; each event is tagged with the project it belongs to.

use std::sync::mpsc::{self, Receiver, SendError, Sender};

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
    /// The project file index was (re)built.
    Index {
        index: std::sync::Arc<vista_search::FileIndex>,
        include_ignored: bool,
    },
    /// Streaming results of a Find in Files run.
    Find {
        generation: u64,
        event: vista_search::FindEvent,
    },
    /// A commit finished — one per repository with ticked files — or
    /// failed with git's / the hooks' output.
    CommitDone {
        result: Box<Result<Vec<(vista_git::Git, vista_git::write::CommitResult)>, String>>,
        files: usize,
        push: bool,
    },
    PushDone(Result<String, String>),
    /// A diff tab finished loading.
    DiffLoaded {
        tab: u64,
        generation: u64,
        load: Box<crate::diffview::DiffLoad>,
    },
    /// Something from the embedded Neovim.
    Nvim(vista_nvim::NvimEvent),
    /// An image referenced from Markdown finished decoding.
    MdImageDecoded {
        path: std::path::PathBuf,
        image: Result<Box<image::DynamicImage>, String>,
    },
    /// A terminal tab has new output (clear the flag once drawn).
    TermOutput(std::sync::Arc<std::sync::atomic::AtomicBool>),
    /// A terminal tab's program ended.
    TermExited,
    /// A control-socket request, for the workspace (it replies once applied).
    Control(Box<crate::control::Call>),
    /// The Review panel's list, refreshed (`generation` drops stale ones).
    Review {
        generation: u64,
        result: Result<vista_git::review::Review, String>,
    },
    /// The commit log (Git window) loaded.
    Log {
        generation: u64,
        result: Result<Vec<vista_git::Commit>, String>,
    },
    /// The files a commit changed.
    CommitFiles {
        hash: String,
        result: Result<Vec<vista_git::CommitFile>, String>,
    },
}

/// Which open project an event belongs to.
pub type ProjectId = u64;

/// Terminal input: goes to whichever project is active.
pub const INPUT: ProjectId = 0;

/// The receiving end of the shared channel.
pub type EventRx = Receiver<(ProjectId, AppEvent)>;

/// Sends events to one project (cloned into its worker threads).
#[derive(Clone)]
pub struct EventTx {
    project: ProjectId,
    tx: Sender<(ProjectId, AppEvent)>,
}

impl EventTx {
    pub fn new(project: ProjectId, tx: Sender<(ProjectId, AppEvent)>) -> EventTx {
        EventTx { project, tx }
    }

    pub fn send(&self, ev: AppEvent) -> Result<(), SendError<AppEvent>> {
        self.tx.send((self.project, ev)).map_err(|e| SendError(e.0.1))
    }

    /// The same channel, for another project (or for terminal input).
    pub fn for_project(&self, project: ProjectId) -> EventTx {
        EventTx {
            project,
            tx: self.tx.clone(),
        }
    }
}

/// A new channel; the sender is for `project`.
pub fn channel(project: ProjectId) -> (EventTx, EventRx) {
    let (tx, rx) = mpsc::channel();
    (EventTx::new(project, tx), rx)
}
