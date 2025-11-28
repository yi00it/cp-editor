//! LSP-related types for storing language server data in the editor.

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

/// A diagnostic message (error, warning, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Start line (0-indexed).
    pub start_line: usize,
    /// Start column (0-indexed).
    pub start_col: usize,
    /// End line (0-indexed).
    pub end_line: usize,
    /// End column (0-indexed).
    pub end_col: usize,
    /// Severity level.
    pub severity: DiagnosticSeverity,
    /// Diagnostic message.
    pub message: String,
    /// Optional error code.
    pub code: Option<String>,
    /// Optional source (e.g., "rust-analyzer").
    pub source: Option<String>,
}

impl Diagnostic {
    pub fn new(
        start_line: usize,
        start_col: usize,
        end_line: usize,
        end_col: usize,
        severity: DiagnosticSeverity,
        message: String,
    ) -> Self {
        Self {
            start_line,
            start_col,
            end_line,
            end_col,
            severity,
            message,
            code: None,
            source: None,
        }
    }

    /// Returns true if the diagnostic is on the given line.
    pub fn on_line(&self, line: usize) -> bool {
        line >= self.start_line && line <= self.end_line
    }

    /// Returns true if the diagnostic covers the given position.
    pub fn contains(&self, line: usize, col: usize) -> bool {
        if line < self.start_line || line > self.end_line {
            return false;
        }
        if line == self.start_line && col < self.start_col {
            return false;
        }
        if line == self.end_line && col > self.end_col {
            return false;
        }
        true
    }
}

/// Hover information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverInfo {
    /// The hover content (may contain markdown).
    pub contents: String,
    /// Start line of the range.
    pub start_line: Option<usize>,
    /// Start column.
    pub start_col: Option<usize>,
    /// End line.
    pub end_line: Option<usize>,
    /// End column.
    pub end_col: Option<usize>,
}

impl HoverInfo {
    pub fn new(contents: String) -> Self {
        Self {
            contents,
            start_line: None,
            start_col: None,
            end_line: None,
            end_col: None,
        }
    }
}

/// A completion item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    /// Label displayed in the completion list.
    pub label: String,
    /// Kind icon identifier.
    pub kind: Option<CompletionKind>,
    /// Detailed information (type signature, etc.).
    pub detail: Option<String>,
    /// Text to insert when this item is selected.
    pub insert_text: Option<String>,
}

/// Completion item kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    Keyword,
    Snippet,
    Constant,
    Struct,
    Enum,
    EnumMember,
    TypeParameter,
    Other,
}
