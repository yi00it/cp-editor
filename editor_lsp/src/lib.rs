//! LSP Client - Language Server Protocol client for CP Editor.
//!
//! This module provides async LSP communication isolated from the render loop.
//! All LSP operations run on a separate thread, communicating with the UI
//! via channels.

pub mod client;
pub mod messages;
pub mod transport;
pub mod types;

pub use client::{LspClient, LspHandle, ServerConfig};
pub use messages::{LspNotification, LspRequest, LspResponse};
pub use types::{
    CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity, HoverInfo, Location,
    Position, Range, TextEdit, WorkspaceEdit,
};
