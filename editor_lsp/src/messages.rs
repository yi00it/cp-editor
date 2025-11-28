//! Message types for LSP client-UI communication.
//!
//! These messages are sent over channels between the UI thread and LSP thread.

use crate::types::{CompletionItem, Diagnostic, HoverInfo, Location, Position, WorkspaceEdit};
use std::path::PathBuf;

/// Request ID for correlating responses.
pub type RequestId = u64;

/// Requests sent from UI to LSP client.
#[derive(Debug, Clone)]
pub enum LspRequest {
    /// Initialize the LSP server for a workspace.
    Initialize {
        id: RequestId,
        root_path: PathBuf,
    },
    /// Shutdown the LSP server.
    Shutdown,
    /// Notify that a document was opened.
    DidOpen {
        path: PathBuf,
        language_id: String,
        version: i32,
        text: String,
    },
    /// Notify that a document was changed.
    DidChange {
        path: PathBuf,
        version: i32,
        text: String,
    },
    /// Notify that a document was saved.
    DidSave {
        path: PathBuf,
    },
    /// Notify that a document was closed.
    DidClose {
        path: PathBuf,
    },
    /// Request hover information.
    Hover {
        id: RequestId,
        path: PathBuf,
        position: Position,
    },
    /// Request completions.
    Completion {
        id: RequestId,
        path: PathBuf,
        position: Position,
    },
    /// Request go to definition.
    GotoDefinition {
        id: RequestId,
        path: PathBuf,
        position: Position,
    },
    /// Request find references.
    FindReferences {
        id: RequestId,
        path: PathBuf,
        position: Position,
        include_declaration: bool,
    },
    /// Request rename symbol.
    Rename {
        id: RequestId,
        path: PathBuf,
        position: Position,
        new_name: String,
    },
    /// Request document symbols.
    DocumentSymbols {
        id: RequestId,
        path: PathBuf,
    },
}

/// Responses from LSP client to UI.
#[derive(Debug, Clone)]
pub enum LspResponse {
    /// Server initialized successfully.
    Initialized {
        id: RequestId,
        /// Server capabilities description.
        capabilities_summary: String,
    },
    /// Initialization failed.
    InitializeFailed {
        id: RequestId,
        error: String,
    },
    /// Hover response.
    Hover {
        id: RequestId,
        info: Option<HoverInfo>,
    },
    /// Completion response.
    Completion {
        id: RequestId,
        items: Vec<CompletionItem>,
    },
    /// Go to definition response.
    GotoDefinition {
        id: RequestId,
        locations: Vec<Location>,
    },
    /// Find references response.
    References {
        id: RequestId,
        locations: Vec<Location>,
    },
    /// Rename response.
    Rename {
        id: RequestId,
        edit: Option<WorkspaceEdit>,
    },
    /// Document symbols response.
    DocumentSymbols {
        id: RequestId,
        symbols: Vec<DocumentSymbol>,
    },
    /// Generic error response.
    Error {
        id: RequestId,
        message: String,
    },
}

/// Notifications from LSP server (not correlated with requests).
#[derive(Debug, Clone)]
pub enum LspNotification {
    /// Server is ready.
    ServerReady,
    /// Server has exited.
    ServerExited {
        code: Option<i32>,
    },
    /// Diagnostics for a file.
    Diagnostics {
        path: PathBuf,
        diagnostics: Vec<Diagnostic>,
    },
    /// Progress notification.
    Progress {
        token: String,
        message: Option<String>,
        percentage: Option<u32>,
    },
    /// Log message from server.
    LogMessage {
        level: LogLevel,
        message: String,
    },
}

/// Log level for server messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warning,
    Info,
    Log,
}

/// A document symbol (function, class, etc.).
#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    /// Symbol name.
    pub name: String,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// Range of the symbol.
    pub range: crate::types::Range,
    /// Selection range (identifier).
    pub selection_range: crate::types::Range,
    /// Children symbols.
    pub children: Vec<DocumentSymbol>,
}

/// Symbol kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    File,
    Module,
    Namespace,
    Package,
    Class,
    Method,
    Property,
    Field,
    Constructor,
    Enum,
    Interface,
    Function,
    Variable,
    Constant,
    String,
    Number,
    Boolean,
    Array,
    Object,
    Key,
    Null,
    EnumMember,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

impl From<lsp_types::SymbolKind> for SymbolKind {
    fn from(kind: lsp_types::SymbolKind) -> Self {
        match kind {
            lsp_types::SymbolKind::FILE => Self::File,
            lsp_types::SymbolKind::MODULE => Self::Module,
            lsp_types::SymbolKind::NAMESPACE => Self::Namespace,
            lsp_types::SymbolKind::PACKAGE => Self::Package,
            lsp_types::SymbolKind::CLASS => Self::Class,
            lsp_types::SymbolKind::METHOD => Self::Method,
            lsp_types::SymbolKind::PROPERTY => Self::Property,
            lsp_types::SymbolKind::FIELD => Self::Field,
            lsp_types::SymbolKind::CONSTRUCTOR => Self::Constructor,
            lsp_types::SymbolKind::ENUM => Self::Enum,
            lsp_types::SymbolKind::INTERFACE => Self::Interface,
            lsp_types::SymbolKind::FUNCTION => Self::Function,
            lsp_types::SymbolKind::VARIABLE => Self::Variable,
            lsp_types::SymbolKind::CONSTANT => Self::Constant,
            lsp_types::SymbolKind::STRING => Self::String,
            lsp_types::SymbolKind::NUMBER => Self::Number,
            lsp_types::SymbolKind::BOOLEAN => Self::Boolean,
            lsp_types::SymbolKind::ARRAY => Self::Array,
            lsp_types::SymbolKind::OBJECT => Self::Object,
            lsp_types::SymbolKind::KEY => Self::Key,
            lsp_types::SymbolKind::NULL => Self::Null,
            lsp_types::SymbolKind::ENUM_MEMBER => Self::EnumMember,
            lsp_types::SymbolKind::STRUCT => Self::Struct,
            lsp_types::SymbolKind::EVENT => Self::Event,
            lsp_types::SymbolKind::OPERATOR => Self::Operator,
            lsp_types::SymbolKind::TYPE_PARAMETER => Self::TypeParameter,
            _ => Self::Variable,
        }
    }
}
