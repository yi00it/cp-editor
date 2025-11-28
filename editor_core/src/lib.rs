//! Editor Core - Pure text editor logic.
//!
//! This crate contains all editor state and behavior without any
//! dependencies on windowing or rendering systems.

pub mod buffer;
pub mod cursor;
pub mod editor;
pub mod history;
pub mod lsp_types;
pub mod search;
pub mod syntax;
pub mod workspace;

pub use buffer::TextBuffer;
pub use cursor::{BlockSelection, Cursor, MultiCursor, Position, Selection, SelectionMode};
pub use editor::Editor;
pub use history::{EditOperation, History};
pub use lsp_types::{CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity, HoverInfo};
pub use search::{Search, SearchMatch};
pub use syntax::{Language, SyntaxHighlighter, Theme, TokenStyle};
pub use workspace::{BufferId, TabInfo, Workspace};
