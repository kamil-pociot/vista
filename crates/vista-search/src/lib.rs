//! Search for vista: "Go to File" (fuzzy file names), "Find in Files"
//! (text inside files) and speed search (type-to-search in the tree).

pub mod find;
pub mod fuzzy;
pub mod speed;

pub use find::{FindEvent, FindOptions, Hit, Scope, Summary};
pub use fuzzy::{FileIndex, FileMatch, Query};
pub use speed::speed_match;
