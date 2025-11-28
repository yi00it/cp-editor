//! Search and replace functionality.

use crate::buffer::TextBuffer;

/// A search match in the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    /// Start character position (inclusive).
    pub start: usize,
    /// End character position (exclusive).
    pub end: usize,
}

impl SearchMatch {
    /// Creates a new search match.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Returns the length of the match in characters.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Returns true if the match is empty.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Search state for incremental search.
#[derive(Debug, Clone)]
pub struct Search {
    /// The current search query.
    query: String,
    /// All matches in the buffer.
    matches: Vec<SearchMatch>,
    /// Index of the current (highlighted) match.
    current_match: Option<usize>,
    /// Whether search is case sensitive.
    case_sensitive: bool,
    /// Whether to use regex search.
    use_regex: bool,
}

impl Default for Search {
    fn default() -> Self {
        Self::new()
    }
}

impl Search {
    /// Creates a new empty search state.
    pub fn new() -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            current_match: None,
            case_sensitive: false,
            use_regex: false,
        }
    }

    /// Returns the current search query.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Sets the search query and performs a search on the buffer.
    /// Returns the number of matches found.
    pub fn set_query(&mut self, query: &str, buffer: &TextBuffer) -> usize {
        self.query = query.to_string();
        self.find_all(buffer)
    }

    /// Returns whether search is case sensitive.
    pub fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    /// Sets whether search is case sensitive.
    pub fn set_case_sensitive(&mut self, sensitive: bool, buffer: &TextBuffer) {
        if self.case_sensitive != sensitive {
            self.case_sensitive = sensitive;
            self.find_all(buffer);
        }
    }

    /// Toggles case sensitivity and re-searches.
    pub fn toggle_case_sensitive(&mut self, buffer: &TextBuffer) {
        self.case_sensitive = !self.case_sensitive;
        self.find_all(buffer);
    }

    /// Returns all matches.
    pub fn matches(&self) -> &[SearchMatch] {
        &self.matches
    }

    /// Returns the number of matches.
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Returns the current match index (1-based for display).
    pub fn current_match_index(&self) -> Option<usize> {
        self.current_match.map(|i| i + 1)
    }

    /// Returns the current match, if any.
    pub fn current_match(&self) -> Option<SearchMatch> {
        self.current_match.map(|i| self.matches[i])
    }

    /// Clears the search state.
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current_match = None;
    }

    /// Returns true if there are any matches.
    pub fn has_matches(&self) -> bool {
        !self.matches.is_empty()
    }

    /// Returns true if the search is active (has a non-empty query).
    pub fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    /// Finds all matches in the buffer.
    fn find_all(&mut self, buffer: &TextBuffer) -> usize {
        self.matches.clear();
        self.current_match = None;

        if self.query.is_empty() {
            return 0;
        }

        let text = buffer.to_string();
        let query = if self.case_sensitive {
            self.query.clone()
        } else {
            self.query.to_lowercase()
        };

        let search_text = if self.case_sensitive {
            text.clone()
        } else {
            text.to_lowercase()
        };

        // Find all occurrences
        let mut start = 0;
        while let Some(pos) = search_text[start..].find(&query) {
            let match_start = start + pos;
            let match_end = match_start + self.query.len();
            self.matches.push(SearchMatch::new(match_start, match_end));
            start = match_start + 1; // Allow overlapping matches
        }

        if !self.matches.is_empty() {
            self.current_match = Some(0);
        }

        self.matches.len()
    }

    /// Moves to the next match, wrapping around.
    /// Returns the new current match position if any.
    pub fn next_match(&mut self) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }

        let next = match self.current_match {
            Some(i) => (i + 1) % self.matches.len(),
            None => 0,
        };
        self.current_match = Some(next);
        Some(self.matches[next])
    }

    /// Moves to the previous match, wrapping around.
    /// Returns the new current match position if any.
    pub fn prev_match(&mut self) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }

        let prev = match self.current_match {
            Some(i) if i > 0 => i - 1,
            _ => self.matches.len() - 1,
        };
        self.current_match = Some(prev);
        Some(self.matches[prev])
    }

    /// Finds the match closest to the given cursor position.
    /// Returns the match and its index.
    pub fn find_nearest(&mut self, cursor_pos: usize) -> Option<SearchMatch> {
        if self.matches.is_empty() {
            return None;
        }

        // Find the first match at or after cursor position
        let idx = self.matches.iter()
            .position(|m| m.start >= cursor_pos)
            .unwrap_or(0); // Wrap to first match if none found after cursor

        self.current_match = Some(idx);
        Some(self.matches[idx])
    }

    /// Updates the search after the buffer has changed.
    /// Should be called after any text modification.
    pub fn refresh(&mut self, buffer: &TextBuffer) {
        let old_current = self.current_match.and_then(|i| self.matches.get(i).copied());
        self.find_all(buffer);

        // Try to restore position near the old match
        if let Some(old) = old_current {
            self.find_nearest(old.start);
        }
    }

    /// Returns matches that overlap with the given line range.
    /// Useful for rendering only visible matches.
    pub fn matches_in_range(&self, buffer: &TextBuffer, start_line: usize, end_line: usize) -> Vec<SearchMatch> {
        if self.matches.is_empty() {
            return Vec::new();
        }

        let range_start = buffer.line_start(start_line);
        let range_end = if end_line >= buffer.len_lines() {
            buffer.len_chars()
        } else {
            buffer.line_start(end_line + 1)
        };

        self.matches.iter()
            .filter(|m| m.end > range_start && m.start < range_end)
            .copied()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_basic() {
        let buffer = TextBuffer::from_str("hello world hello");
        let mut search = Search::new();

        let count = search.set_query("hello", &buffer);
        assert_eq!(count, 2);
        assert_eq!(search.match_count(), 2);

        let first = search.current_match().unwrap();
        assert_eq!(first.start, 0);
        assert_eq!(first.end, 5);

        let second = search.next_match().unwrap();
        assert_eq!(second.start, 12);
        assert_eq!(second.end, 17);

        // Wrap around
        let wrapped = search.next_match().unwrap();
        assert_eq!(wrapped.start, 0);
    }

    #[test]
    fn test_search_case_insensitive() {
        let buffer = TextBuffer::from_str("Hello HELLO hello");
        let mut search = Search::new();

        let count = search.set_query("hello", &buffer);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_search_case_sensitive() {
        let buffer = TextBuffer::from_str("Hello HELLO hello");
        let mut search = Search::new();
        search.case_sensitive = true;

        let count = search.set_query("hello", &buffer);
        assert_eq!(count, 1);
    }

    #[test]
    fn test_search_prev() {
        let buffer = TextBuffer::from_str("a b a c a");
        let mut search = Search::new();

        search.set_query("a", &buffer);
        assert_eq!(search.match_count(), 3);

        // Start at first match
        assert_eq!(search.current_match_index(), Some(1));

        // Go to last match (wrap)
        let prev = search.prev_match().unwrap();
        assert_eq!(prev.start, 8);
        assert_eq!(search.current_match_index(), Some(3));

        // Go to second match
        let prev = search.prev_match().unwrap();
        assert_eq!(prev.start, 4);
    }

    #[test]
    fn test_search_empty_query() {
        let buffer = TextBuffer::from_str("hello world");
        let mut search = Search::new();

        let count = search.set_query("", &buffer);
        assert_eq!(count, 0);
        assert!(!search.is_active());
    }

    #[test]
    fn test_search_no_matches() {
        let buffer = TextBuffer::from_str("hello world");
        let mut search = Search::new();

        let count = search.set_query("xyz", &buffer);
        assert_eq!(count, 0);
        assert!(search.is_active());
        assert!(!search.has_matches());
    }

    #[test]
    fn test_find_nearest() {
        let buffer = TextBuffer::from_str("a  a  a  a");
        let mut search = Search::new();
        search.set_query("a", &buffer);

        // Find nearest to position 5 (should find the "a" at position 6)
        let nearest = search.find_nearest(5).unwrap();
        assert_eq!(nearest.start, 6);
    }
}
