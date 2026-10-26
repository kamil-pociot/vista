//! Rendering for vista's viewer: syntax highlighting, Markdown and file-type
//! detection. Output is styled with theme tokens; the UI maps them to colours.

pub mod filetype;
pub mod highlight;
pub mod markdown;
pub mod mermaid;
pub mod styled;

pub use filetype::FileKind;
pub use highlight::Highlighter;
pub use markdown::{ImageSlot, MdLine};
pub use styled::{StyledLine, StyledSpan, TokStyle};
