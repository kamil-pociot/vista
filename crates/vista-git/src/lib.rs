//! Git integration for vista.
//!
//! Everything goes through the `git` CLI: reads use stable machine-readable
//! formats (`status --porcelain=v2 -z`, `cat-file`), and writes run the same
//! commands a user would, so hooks, signing and credential helpers keep
//! working.

pub mod cmd;
pub mod log;
pub mod review;
pub mod status;
pub mod write;

pub use cmd::{Git, GitError};
pub use log::{Commit, CommitFile};
pub use status::{BranchInfo, Change, FileStatus, StatusSnapshot};
