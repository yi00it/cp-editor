//! Language detection and configuration.
//!
//! Detects programming languages from file extensions and provides
//! tree-sitter language configurations.

use std::path::Path;

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Json,
    PlainText,
}

impl Language {
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
            "rs" => Self::Rust,
            "json" => Self::Json,
            _ => Self::PlainText,
        }
    }

    /// Returns the display name of the language.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
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
            Language::from_path(Path::new("Makefile")),
            Language::PlainText
        );
    }

    #[test]
    fn test_tree_sitter_language() {
        assert!(Language::Rust.tree_sitter_language().is_some());
        assert!(Language::Json.tree_sitter_language().is_some());
        assert!(Language::PlainText.tree_sitter_language().is_none());
    }
}
