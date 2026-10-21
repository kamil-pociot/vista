//! Search for vista: "Go to File" (fuzzy file names) and "Find in Files"
//! (text inside files).

pub mod fuzzy;

pub use fuzzy::{FileIndex, FileMatch, Query};
