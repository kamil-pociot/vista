//! Search for vista: "Go to File" (fuzzy file names) and "Find in Files"
//! (text inside files).

pub mod find;
pub mod fuzzy;

pub use find::{FindEvent, FindOptions, Hit, Scope, Summary};
pub use fuzzy::{FileIndex, FileMatch, Query};
