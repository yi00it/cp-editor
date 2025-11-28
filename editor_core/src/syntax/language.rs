//! Language detection and configuration.
//!
//! Detects programming languages from file extensions and provides
//! tree-sitter language configurations.

use std::path::Path;

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    C,
    Cpp,
    Json,
    PlainText,
}

impl Language {
    /// Returns all available languages (for UI selection).
    pub fn all() -> &'static [Language] {
        &[
            Language::Rust,
            Language::Python,
            Language::JavaScript,
            Language::TypeScript,
            Language::C,
            Language::Cpp,
            Language::Json,
            Language::PlainText,
        ]
    }

    /// Detects language from a file path based on extension.
    pub fn from_path(path: &Path) -> Self {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| Self::from_extension(ext))
            .unwrap_or(Self::PlainText)
    }

    /// Detects language from a file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            // Rust
            "rs" => Self::Rust,
            // Python
            "py" | "pyw" | "pyi" => Self::Python,
            // JavaScript
            "js" | "jsx" | "mjs" | "cjs" => Self::JavaScript,
            // TypeScript
            "ts" | "tsx" | "mts" | "cts" => Self::TypeScript,
            // C
            "c" | "h" => Self::C,
            // C++
            "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" => Self::Cpp,
            // JSON
            "json" | "jsonc" | "json5" => Self::Json,
            // Default
            _ => Self::PlainText,
        }
    }

    /// Returns the display name of the language.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Json => "JSON",
            Self::PlainText => "Plain Text",
        }
    }

    /// Returns whether this language supports syntax highlighting.
    pub fn has_highlighting(&self) -> bool {
        !matches!(self, Self::PlainText)
    }

    /// Returns the tree-sitter language for this language, if available.
    pub fn tree_sitter_language(&self) -> Option<tree_sitter::Language> {
        match self {
            Self::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
            Self::Python => Some(tree_sitter_python::LANGUAGE.into()),
            Self::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),
            Self::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            Self::C => Some(tree_sitter_c::LANGUAGE.into()),
            Self::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
            Self::Json => Some(tree_sitter_json::LANGUAGE.into()),
            Self::PlainText => None,
        }
    }
}

impl Default for Language {
    fn default() -> Self {
        Self::PlainText
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_extension() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(Language::from_extension("RS"), Language::Rust);
        assert_eq!(Language::from_extension("py"), Language::Python);
        assert_eq!(Language::from_extension("js"), Language::JavaScript);
        assert_eq!(Language::from_extension("ts"), Language::TypeScript);
        assert_eq!(Language::from_extension("c"), Language::C);
        assert_eq!(Language::from_extension("cpp"), Language::Cpp);
        assert_eq!(Language::from_extension("json"), Language::Json);
        assert_eq!(Language::from_extension("txt"), Language::PlainText);
        assert_eq!(Language::from_extension("unknown"), Language::PlainText);
    }

    #[test]
    fn test_from_path() {
        assert_eq!(
            Language::from_path(Path::new("main.rs")),
            Language::Rust
        );
        assert_eq!(
            Language::from_path(Path::new("/path/to/config.json")),
            Language::Json
        );
        assert_eq!(
            Language::from_path(Path::new("README.md")),
            Language::PlainText
        );
        assert_eq!(
            Language::from_path(Path::new("main.py")),
            Language::Python
        );
    }

    #[test]
    fn test_tree_sitter_language() {
        assert!(Language::Rust.tree_sitter_language().is_some());
        assert!(Language::Python.tree_sitter_language().is_some());
        assert!(Language::JavaScript.tree_sitter_language().is_some());
        assert!(Language::TypeScript.tree_sitter_language().is_some());
        assert!(Language::Json.tree_sitter_language().is_some());
        assert!(Language::PlainText.tree_sitter_language().is_none());
    }
}
