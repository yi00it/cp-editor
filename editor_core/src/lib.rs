//! Editor Core - Pure text editor logic.
//!
//! This crate contains all editor state and behavior without any
//! dependencies on windowing or rendering systems.

pub mod buffer;
pub mod cursor;
pub mod editor;
pub mod history;
pub mod workspace;

pub use buffer::TextBuffer;
pub use cursor::{Cursor, Position, Selection};
pub use editor::Editor;
pub use history::{EditOperation, History};
pub use workspace::{BufferId, TabInfo, Workspace};
