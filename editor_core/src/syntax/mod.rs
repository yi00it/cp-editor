//! Syntax highlighting module.
//!
//! Provides incremental syntax highlighting using tree-sitter.

mod highlighter;
mod language;
mod theme;

pub use highlighter::{HighlightSpan, LineHighlights, SyntaxHighlighter};
pub use language::Language;
pub use theme::{Theme, TokenStyle};
