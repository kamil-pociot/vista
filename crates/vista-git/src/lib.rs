//! Git integration for vista.
//!
//! Everything goes through the `git` CLI: reads use stable machine-readable
//! formats (`status --porcelain=v2 -z`, `cat-file`), and writes run the same
//! commands a user would, so hooks, signing and credential helpers keep
//! working.

pub mod cmd;
pub mod status;

pub use cmd::{Git, GitError};
pub use status::{BranchInfo, Change, FileStatus, StatusSnapshot};
