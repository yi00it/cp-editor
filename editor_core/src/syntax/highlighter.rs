//! Syntax highlighter using tree-sitter.
//!
//! Provides incremental syntax highlighting with tree-sitter parsing.

use super::language::Language;
use super::theme::{Theme, TokenStyle};
use tree_sitter::{Node, Parser, Tree, TreeCursor};

/// A highlighted span representing a range of text with a style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HighlightSpan {
    /// Start byte offset in the source.
    pub start_byte: usize,
    /// End byte offset in the source.
    pub end_byte: usize,
    /// Token style for this span.
    pub style: TokenStyle,
}

impl HighlightSpan {
    /// Creates a new highlight span.
    pub fn new(start_byte: usize, end_byte: usize, style: TokenStyle) -> Self {
        Self {
            start_byte,
            end_byte,
            style,
        }
    }
}

/// Line-based highlight cache for efficient rendering.
#[derive(Debug, Clone)]
pub struct LineHighlights {
    /// Character spans with their styles for this line.
    /// Each entry is (start_col, end_col, style).
    spans: Vec<(usize, usize, TokenStyle)>,
}

impl LineHighlights {
    /// Creates empty line highlights.
    pub fn new() -> Self {
        Self { spans: Vec::new() }
    }

    /// Adds a span to the line.
    pub fn add_span(&mut self, start_col: usize, end_col: usize, style: TokenStyle) {
        self.spans.push((start_col, end_col, style));
    }

    /// Returns the style for a given column, or None if no highlight.
    pub fn style_at(&self, col: usize) -> Option<TokenStyle> {
        for &(start, end, style) in &self.spans {
            if col >= start && col < end {
                return Some(style);
            }
        }
        None
    }

    /// Returns all spans for this line.
    pub fn spans(&self) -> &[(usize, usize, TokenStyle)] {
        &self.spans
    }
}

impl Default for LineHighlights {
    fn default() -> Self {
        Self::new()
    }
}

/// Syntax highlighter using tree-sitter for incremental parsing.
pub struct SyntaxHighlighter {
    /// Tree-sitter parser.
    parser: Parser,
    /// Current parse tree.
    tree: Option<Tree>,
    /// Current language.
    language: Language,
    /// Syntax theme.
    theme: Theme,
    /// Cached line highlights.
    line_cache: Vec<LineHighlights>,
    /// Whether the cache is valid.
    cache_valid: bool,
}

impl SyntaxHighlighter {
    /// Creates a new syntax highlighter.
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            tree: None,
            language: Language::PlainText,
            theme: Theme::dark(),
            line_cache: Vec::new(),
            cache_valid: false,
        }
    }

    /// Sets the syntax theme.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.cache_valid = false;
    }

    /// Returns a reference to the current theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Sets the language and configures the parser.
    pub fn set_language(&mut self, language: Language) {
        if self.language == language {
            return;
        }

        self.language = language;
        self.tree = None;
        self.cache_valid = false;

        if let Some(ts_lang) = language.tree_sitter_language() {
            if self.parser.set_language(&ts_lang).is_err() {
                // Failed to set language, fall back to plain text
                self.language = Language::PlainText;
            }
        }
    }

    /// Returns the current language.
    pub fn language(&self) -> Language {
        self.language
    }

    /// Parses the source code and updates the syntax tree.
    /// This performs a full parse.
    pub fn parse(&mut self, source: &str) {
        if !self.language.has_highlighting() {
            self.tree = None;
            self.cache_valid = false;
            return;
        }

        self.tree = self.parser.parse(source, None);
        self.cache_valid = false;
    }

    /// Incrementally updates the parse tree after an edit.
    #[allow(clippy::too_many_arguments)]
    pub fn edit(
        &mut self,
        source: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
        start_position: (usize, usize),
        old_end_position: (usize, usize),
        new_end_position: (usize, usize),
    ) {
        if !self.language.has_highlighting() {
            return;
        }

        // Apply the edit to the existing tree
        if let Some(tree) = &mut self.tree {
            let edit = tree_sitter::InputEdit {
                start_byte,
                old_end_byte,
                new_end_byte,
                start_position: tree_sitter::Point {
                    row: start_position.0,
                    column: start_position.1,
                },
                old_end_position: tree_sitter::Point {
                    row: old_end_position.0,
                    column: old_end_position.1,
                },
                new_end_position: tree_sitter::Point {
                    row: new_end_position.0,
                    column: new_end_position.1,
                },
            };
            tree.edit(&edit);

            // Re-parse with the old tree for incremental parsing
            self.tree = self.parser.parse(source, Some(tree));
        } else {
            // No existing tree, do a full parse
            self.tree = self.parser.parse(source, None);
        }

        self.cache_valid = false;
    }

    /// Builds the line cache for efficient rendering.
    pub fn build_line_cache(&mut self, source: &str, line_count: usize) {
        self.line_cache.clear();
        self.line_cache.resize_with(line_count, LineHighlights::new);

        let tree = match &self.tree {
            Some(t) => t,
            None => {
                self.cache_valid = true;
                return;
            }
        };

        // Build byte offset to line/col mapping
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        // Collect all highlights first (avoiding borrow issues)
        let mut highlights: Vec<(usize, usize, usize, TokenStyle)> = Vec::new();

        // Walk the tree and collect highlights
        let mut cursor = tree.walk();
        Self::collect_highlights(
            &mut cursor,
            source,
            &line_starts,
            line_count,
            self.language,
            &mut highlights,
        );

        // Apply collected highlights to line cache
        for (row, start_col, end_col, style) in highlights {
            if row < self.line_cache.len() {
                self.line_cache[row].add_span(start_col, end_col, style);
            }
        }

        self.cache_valid = true;
    }

    /// Recursively collects highlights from the tree.
    fn collect_highlights(
        cursor: &mut TreeCursor,
        source: &str,
        line_starts: &[usize],
        line_count: usize,
        language: Language,
        highlights: &mut Vec<(usize, usize, usize, TokenStyle)>,
    ) {
        loop {
            let node = cursor.node();

            // Determine style based on node type
            if let Some(style) = Self::node_style_static(&node, language) {
                Self::add_node_highlights_static(
                    &node,
                    style,
                    source,
                    line_starts,
                    line_count,
                    highlights,
                );
            }

            // Visit children
            if cursor.goto_first_child() {
                Self::collect_highlights(cursor, source, line_starts, line_count, language, highlights);
                cursor.goto_parent();
            }

            // Move to next sibling
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    /// Determines the token style for a tree-sitter node (static version).
    fn node_style_static(node: &Node, language: Language) -> Option<TokenStyle> {
        let kind = node.kind();

        match language {
            Language::Rust => Self::rust_node_style_static(node, kind),
            Language::Python => Self::python_node_style_static(node, kind),
            Language::JavaScript | Language::TypeScript => Self::js_ts_node_style_static(node, kind),
            Language::C | Language::Cpp => Self::c_cpp_node_style_static(node, kind),
            Language::Json => Self::json_node_style_static(node, kind),
            Language::PlainText => None,
        }
    }

    /// Determines style for Rust nodes (static version).
    fn rust_node_style_static(node: &Node, kind: &str) -> Option<TokenStyle> {
        match kind {
            // Keywords
            "fn" | "let" | "mut" | "const" | "static" | "pub" | "mod" | "use" | "crate"
            | "self" | "super" | "impl" | "trait" | "struct" | "enum" | "type" | "where"
            | "async" | "await" | "dyn" | "extern" | "ref" | "unsafe" | "as" | "in" => {
                Some(TokenStyle::Keyword)
            }

            // Control flow
            "if" | "else" | "match" | "for" | "while" | "loop" | "break" | "continue"
            | "return" | "yield" => Some(TokenStyle::ControlFlow),

            // Literals
            "string_literal" | "raw_string_literal" | "string_content" => Some(TokenStyle::String),
            "char_literal" => Some(TokenStyle::Char),
            "integer_literal" | "float_literal" => Some(TokenStyle::Number),
            "true" | "false" => Some(TokenStyle::Boolean),

            // Comments
            "line_comment" | "block_comment" => Some(TokenStyle::Comment),

            // Types
            "type_identifier" | "primitive_type" => Some(TokenStyle::Type),

            // Identifiers - need context
            "identifier" => {
                if let Some(parent) = node.parent() {
                    match parent.kind() {
                        "function_item" | "function_signature_item" => {
                            if parent.child_by_field_name("name") == Some(*node) {
                                return Some(TokenStyle::Function);
                            }
                        }
                        "call_expression" => {
                            if parent.child_by_field_name("function") == Some(*node) {
                                return Some(TokenStyle::Function);
                            }
                        }
                        "macro_invocation" => {
                            return Some(TokenStyle::Macro);
                        }
                        _ => {}
                    }
                }
                None
            }

            // Macros
            "macro_invocation" | "!" => {
                if let Some(parent) = node.parent() {
                    if parent.kind() == "macro_invocation" {
                        return Some(TokenStyle::Macro);
                    }
                }
                None
            }

            // Attributes
            "attribute_item" | "inner_attribute_item" => Some(TokenStyle::Attribute),
            "#" | "[" | "]" if Self::is_in_attribute_static(node) => Some(TokenStyle::Attribute),

            // Lifetime
            "lifetime" => Some(TokenStyle::Lifetime),

            _ => None,
        }
    }

    /// Determines style for JSON nodes (static version).
    fn json_node_style_static(node: &Node, kind: &str) -> Option<TokenStyle> {
        match kind {
            "string" | "string_content" => {
                if let Some(parent) = node.parent() {
                    if parent.kind() == "pair" {
                        if let Some(first_child) = parent.child(0) {
                            if first_child.id() == node.id() {
                                return Some(TokenStyle::Variable);
                            }
                        }
                    }
                }
                Some(TokenStyle::String)
            }
            "number" => Some(TokenStyle::Number),
            "true" | "false" => Some(TokenStyle::Boolean),
            "null" => Some(TokenStyle::Constant),
            _ => None,
        }
    }

    /// Determines style for Python nodes.
    fn python_node_style_static(node: &Node, kind: &str) -> Option<TokenStyle> {
        match kind {
            // Keywords
            "def" | "class" | "import" | "from" | "as" | "global" | "nonlocal"
            | "lambda" | "with" | "assert" | "yield" | "del" | "pass" | "raise"
            | "except" | "finally" | "try" | "async" | "await" => Some(TokenStyle::Keyword),

            // Control flow
            "if" | "elif" | "else" | "for" | "while" | "break" | "continue"
            | "return" | "in" | "not" | "and" | "or" | "is" => Some(TokenStyle::ControlFlow),

            // Literals
            "string" | "string_start" | "string_content" | "string_end" => Some(TokenStyle::String),
            "integer" | "float" => Some(TokenStyle::Number),
            "true" | "false" => Some(TokenStyle::Boolean),
            "none" => Some(TokenStyle::Constant),

            // Comments
            "comment" => Some(TokenStyle::Comment),

            // Decorators
            "decorator" => Some(TokenStyle::Attribute),

            // Function/class names
            "identifier" => {
                if let Some(parent) = node.parent() {
                    match parent.kind() {
                        "function_definition" | "class_definition" => {
                            if parent.child_by_field_name("name") == Some(*node) {
                                return Some(TokenStyle::Function);
                            }
                        }
                        "call" => {
                            if parent.child_by_field_name("function") == Some(*node) {
                                return Some(TokenStyle::Function);
                            }
                        }
                        _ => {}
                    }
                }
                None
            }

            _ => None,
        }
    }

    /// Determines style for JavaScript/TypeScript nodes.
    fn js_ts_node_style_static(node: &Node, kind: &str) -> Option<TokenStyle> {
        match kind {
            // Keywords
            "function" | "const" | "let" | "var" | "class" | "extends" | "import"
            | "export" | "default" | "from" | "as" | "new" | "this" | "super"
            | "static" | "get" | "set" | "async" | "await" | "typeof" | "instanceof"
            | "void" | "delete" | "in" | "of" => Some(TokenStyle::Keyword),

            // TypeScript specific
            "type" | "interface" | "enum" | "namespace" | "module" | "declare"
            | "readonly" | "abstract" | "implements" | "private" | "protected"
            | "public" => Some(TokenStyle::Keyword),

            // Control flow
            "if" | "else" | "for" | "while" | "do" | "switch" | "case" | "break"
            | "continue" | "return" | "throw" | "try" | "catch" | "finally"
            | "yield" => Some(TokenStyle::ControlFlow),

            // Literals
            "string" | "template_string" | "string_fragment" => Some(TokenStyle::String),
            "number" => Some(TokenStyle::Number),
            "true" | "false" => Some(TokenStyle::Boolean),
            "null" | "undefined" => Some(TokenStyle::Constant),

            // Comments
            "comment" | "line_comment" | "block_comment" => Some(TokenStyle::Comment),

            // Types
            "type_identifier" => Some(TokenStyle::Type),

            // Function names
            "identifier" | "property_identifier" => {
                if let Some(parent) = node.parent() {
                    match parent.kind() {
                        "function_declaration" | "method_definition" | "arrow_function" => {
                            if parent.child_by_field_name("name") == Some(*node) {
                                return Some(TokenStyle::Function);
                            }
                        }
                        "call_expression" => {
                            if parent.child_by_field_name("function") == Some(*node) {
                                return Some(TokenStyle::Function);
                            }
                        }
                        _ => {}
                    }
                }
                None
            }

            _ => None,
        }
    }

    /// Determines style for C/C++ nodes.
    fn c_cpp_node_style_static(node: &Node, kind: &str) -> Option<TokenStyle> {
        match kind {
            // Keywords
            "auto" | "break" | "case" | "const" | "continue" | "default" | "do"
            | "else" | "enum" | "extern" | "for" | "goto" | "if" | "inline"
            | "register" | "restrict" | "return" | "signed" | "sizeof" | "static"
            | "struct" | "switch" | "typedef" | "union" | "unsigned" | "void"
            | "volatile" | "while" => Some(TokenStyle::Keyword),

            // C++ specific
            "class" | "namespace" | "template" | "typename" | "virtual" | "override"
            | "final" | "public" | "private" | "protected" | "friend" | "new"
            | "delete" | "this" | "throw" | "try" | "catch" | "using" | "constexpr"
            | "nullptr" | "noexcept" | "decltype" | "explicit" | "mutable"
            | "operator" => Some(TokenStyle::Keyword),

            // Control flow
            "if" | "else" | "for" | "while" | "do" | "switch" | "case" | "break"
            | "continue" | "return" | "goto" => Some(TokenStyle::ControlFlow),

            // Literals
            "string_literal" | "char_literal" | "raw_string_literal" => Some(TokenStyle::String),
            "number_literal" => Some(TokenStyle::Number),
            "true" | "false" => Some(TokenStyle::Boolean),
            "null" | "nullptr" => Some(TokenStyle::Constant),

            // Comments
            "comment" => Some(TokenStyle::Comment),

            // Types
            "type_identifier" | "primitive_type" | "sized_type_specifier" => Some(TokenStyle::Type),

            // Preprocessor
            "preproc_include" | "preproc_def" | "preproc_ifdef" | "preproc_ifndef"
            | "preproc_if" | "preproc_else" | "preproc_elif" | "preproc_endif"
            | "#include" | "#define" | "#ifdef" | "#ifndef" | "#if" | "#else"
            | "#elif" | "#endif" | "#pragma" => Some(TokenStyle::Attribute),

            // Function names
            "identifier" => {
                if let Some(parent) = node.parent() {
                    match parent.kind() {
                        "function_definition" | "function_declarator" => {
                            if parent.child_by_field_name("declarator") == Some(*node) {
                                return Some(TokenStyle::Function);
                            }
                        }
                        "call_expression" => {
                            if parent.child_by_field_name("function") == Some(*node) {
                                return Some(TokenStyle::Function);
                            }
                        }
                        _ => {}
                    }
                }
                None
            }

            _ => None,
        }
    }

    /// Checks if a node is inside an attribute (static version).
    fn is_in_attribute_static(node: &Node) -> bool {
        let mut current = node.parent();
        while let Some(parent) = current {
            if parent.kind() == "attribute_item" || parent.kind() == "inner_attribute_item" {
                return true;
            }
            current = parent.parent();
        }
        false
    }

    /// Adds highlight spans for a node (static version).
    fn add_node_highlights_static(
        node: &Node,
        style: TokenStyle,
        source: &str,
        line_starts: &[usize],
        line_count: usize,
        highlights: &mut Vec<(usize, usize, usize, TokenStyle)>,
    ) {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_row = node.start_position().row;
        let end_row = node.end_position().row;

        for row in start_row..=end_row {
            if row >= line_count {
                break;
            }

            let line_start = line_starts.get(row).copied().unwrap_or(0);
            let line_end = line_starts
                .get(row + 1)
                .map(|&s| s.saturating_sub(1))
                .unwrap_or(source.len());

            let span_start = start_byte.max(line_start);
            let span_end = end_byte.min(line_end);

            if span_start < span_end {
                let line_text = &source[line_start..line_end.min(source.len())];
                let start_col = line_text[..(span_start - line_start).min(line_text.len())]
                    .chars()
                    .count();
                let end_col = line_text[..(span_end - line_start).min(line_text.len())]
                    .chars()
                    .count();

                highlights.push((row, start_col, end_col, style));
            }
        }
    }

    /// Gets the highlights for a specific line.
    pub fn line_highlights(&self, line: usize) -> Option<&LineHighlights> {
        self.line_cache.get(line)
    }

    /// Gets the color for a specific position.
    pub fn color_at(&self, line: usize, col: usize) -> [f32; 4] {
        if let Some(line_hl) = self.line_cache.get(line) {
            if let Some(style) = line_hl.style_at(col) {
                return self.theme.color(style);
            }
        }
        self.theme.foreground
    }

    /// Returns whether highlighting is available.
    pub fn has_highlighting(&self) -> bool {
        self.language.has_highlighting() && self.tree.is_some()
    }

    /// Returns whether the cache is valid.
    pub fn is_cache_valid(&self) -> bool {
        self.cache_valid
    }

    /// Invalidates the cache.
    pub fn invalidate_cache(&mut self) {
        self.cache_valid = false;
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_highlighting() {
        let mut highlighter = SyntaxHighlighter::new();
        highlighter.set_language(Language::Rust);

        let source = r#"fn main() {
    let x = 42;
    println!("Hello");
}"#;

        highlighter.parse(source);
        highlighter.build_line_cache(source, 4);

        assert!(highlighter.has_highlighting());

        let line0 = highlighter.line_highlights(0).unwrap();
        assert!(!line0.spans().is_empty());
    }

    #[test]
    fn test_json_highlighting() {
        let mut highlighter = SyntaxHighlighter::new();
        highlighter.set_language(Language::Json);

        let source = r#"{"key": "value", "num": 42, "bool": true}"#;

        highlighter.parse(source);
        highlighter.build_line_cache(source, 1);

        assert!(highlighter.has_highlighting());
    }

    #[test]
    fn test_plain_text() {
        let mut highlighter = SyntaxHighlighter::new();
        highlighter.set_language(Language::PlainText);

        let source = "Hello, world!";
        highlighter.parse(source);

        assert!(!highlighter.has_highlighting());
    }

    #[test]
    fn test_incremental_edit() {
        let mut highlighter = SyntaxHighlighter::new();
        highlighter.set_language(Language::Rust);

        let source1 = "fn main() {}";
        highlighter.parse(source1);

        let source2 = "fn main() { let x = 1; }";
        highlighter.edit(
            source2,
            11,
            11,
            23,
            (0, 11),
            (0, 11),
            (0, 23),
        );

        assert!(highlighter.tree.is_some());
    }
}
