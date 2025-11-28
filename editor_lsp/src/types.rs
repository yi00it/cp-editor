//! UI-friendly types for LSP features.
//!
//! These types are simplified versions of lsp-types for use in the editor.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A position in a text document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    /// Line number (0-indexed).
    pub line: u32,
    /// Column (0-indexed, UTF-16 code units in LSP, but we'll convert).
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

impl From<lsp_types::Position> for Position {
    fn from(pos: lsp_types::Position) -> Self {
        Self {
            line: pos.line,
            character: pos.character,
        }
    }
}

impl From<Position> for lsp_types::Position {
    fn from(pos: Position) -> Self {
        Self {
            line: pos.line,
            character: pos.character,
        }
    }
}

/// A range in a text document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

impl From<lsp_types::Range> for Range {
    fn from(range: lsp_types::Range) -> Self {
        Self {
            start: range.start.into(),
            end: range.end.into(),
        }
    }
}

impl From<Range> for lsp_types::Range {
    fn from(range: Range) -> Self {
        Self {
            start: range.start.into(),
            end: range.end.into(),
        }
    }
}

/// A location in a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// File path.
    pub path: PathBuf,
    /// Range within the file.
    pub range: Range,
}

impl Location {
    pub fn new(path: PathBuf, range: Range) -> Self {
        Self { path, range }
    }
}

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

impl From<lsp_types::DiagnosticSeverity> for DiagnosticSeverity {
    fn from(severity: lsp_types::DiagnosticSeverity) -> Self {
        match severity {
            lsp_types::DiagnosticSeverity::ERROR => Self::Error,
            lsp_types::DiagnosticSeverity::WARNING => Self::Warning,
            lsp_types::DiagnosticSeverity::INFORMATION => Self::Information,
            lsp_types::DiagnosticSeverity::HINT => Self::Hint,
            _ => Self::Information,
        }
    }
}

/// A diagnostic message (error, warning, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Range of the diagnostic.
    pub range: Range,
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Diagnostic message.
    pub message: String,
    /// Optional error code.
    pub code: Option<String>,
    /// Optional source (e.g., "rust-analyzer").
    pub source: Option<String>,
}

impl From<lsp_types::Diagnostic> for Diagnostic {
    fn from(diag: lsp_types::Diagnostic) -> Self {
        Self {
            range: diag.range.into(),
            severity: diag
                .severity
                .map(|s| s.into())
                .unwrap_or(DiagnosticSeverity::Information),
            message: diag.message,
            code: diag.code.map(|c| match c {
                lsp_types::NumberOrString::Number(n) => n.to_string(),
                lsp_types::NumberOrString::String(s) => s,
            }),
            source: diag.source,
        }
    }
}

/// Hover information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoverInfo {
    /// The hover content (may contain markdown).
    pub contents: String,
    /// Optional range that the hover applies to.
    pub range: Option<Range>,
}

/// Completion item kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompletionKind {
    Text,
    Method,
    Function,
    Constructor,
    Field,
    Variable,
    Class,
    Interface,
    Module,
    Property,
    Unit,
    Value,
    Enum,
    Keyword,
    Snippet,
    Color,
    File,
    Reference,
    Folder,
    EnumMember,
    Constant,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

impl From<lsp_types::CompletionItemKind> for CompletionKind {
    fn from(kind: lsp_types::CompletionItemKind) -> Self {
        match kind {
            lsp_types::CompletionItemKind::TEXT => Self::Text,
            lsp_types::CompletionItemKind::METHOD => Self::Method,
            lsp_types::CompletionItemKind::FUNCTION => Self::Function,
            lsp_types::CompletionItemKind::CONSTRUCTOR => Self::Constructor,
            lsp_types::CompletionItemKind::FIELD => Self::Field,
            lsp_types::CompletionItemKind::VARIABLE => Self::Variable,
            lsp_types::CompletionItemKind::CLASS => Self::Class,
            lsp_types::CompletionItemKind::INTERFACE => Self::Interface,
            lsp_types::CompletionItemKind::MODULE => Self::Module,
            lsp_types::CompletionItemKind::PROPERTY => Self::Property,
            lsp_types::CompletionItemKind::UNIT => Self::Unit,
            lsp_types::CompletionItemKind::VALUE => Self::Value,
            lsp_types::CompletionItemKind::ENUM => Self::Enum,
            lsp_types::CompletionItemKind::KEYWORD => Self::Keyword,
            lsp_types::CompletionItemKind::SNIPPET => Self::Snippet,
            lsp_types::CompletionItemKind::COLOR => Self::Color,
            lsp_types::CompletionItemKind::FILE => Self::File,
            lsp_types::CompletionItemKind::REFERENCE => Self::Reference,
            lsp_types::CompletionItemKind::FOLDER => Self::Folder,
            lsp_types::CompletionItemKind::ENUM_MEMBER => Self::EnumMember,
            lsp_types::CompletionItemKind::CONSTANT => Self::Constant,
            lsp_types::CompletionItemKind::STRUCT => Self::Struct,
            lsp_types::CompletionItemKind::EVENT => Self::Event,
            lsp_types::CompletionItemKind::OPERATOR => Self::Operator,
            lsp_types::CompletionItemKind::TYPE_PARAMETER => Self::TypeParameter,
            _ => Self::Text,
        }
    }
}

/// A completion item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionItem {
    /// Label displayed in the completion list.
    pub label: String,
    /// Kind of completion.
    pub kind: Option<CompletionKind>,
    /// Detailed information (type signature, etc.).
    pub detail: Option<String>,
    /// Documentation.
    pub documentation: Option<String>,
    /// Text to insert when this item is selected.
    pub insert_text: Option<String>,
    /// Whether this is a snippet.
    pub is_snippet: bool,
}

impl From<lsp_types::CompletionItem> for CompletionItem {
    fn from(item: lsp_types::CompletionItem) -> Self {
        let documentation = item.documentation.map(|doc| match doc {
            lsp_types::Documentation::String(s) => s,
            lsp_types::Documentation::MarkupContent(m) => m.value,
        });

        let is_snippet = item.insert_text_format
            == Some(lsp_types::InsertTextFormat::SNIPPET);

        Self {
            label: item.label,
            kind: item.kind.map(|k| k.into()),
            detail: item.detail,
            documentation,
            insert_text: item.insert_text,
            is_snippet,
        }
    }
}

/// A text edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    /// Range to replace.
    pub range: Range,
    /// New text.
    pub new_text: String,
}

impl From<lsp_types::TextEdit> for TextEdit {
    fn from(edit: lsp_types::TextEdit) -> Self {
        Self {
            range: edit.range.into(),
            new_text: edit.new_text,
        }
    }
}

/// A workspace edit (changes to multiple files).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceEdit {
    /// Edits per file.
    pub changes: Vec<(PathBuf, Vec<TextEdit>)>,
}
