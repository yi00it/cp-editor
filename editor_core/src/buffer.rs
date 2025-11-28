//! Text buffer implementation using ropey.

use ropey::Rope;
use std::fs;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;

/// A text buffer backed by a rope data structure.
/// Provides efficient text operations for large files.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    rope: Rope,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBuffer {
    /// Creates a new empty text buffer.
    pub fn new() -> Self {
        Self { rope: Rope::new() }
    }

    /// Creates a text buffer from a string.
    pub fn from_str(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
        }
    }

    /// Loads a text buffer from a file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = fs::File::open(path)?;
        let reader = BufReader::new(file);
        let rope = Rope::from_reader(reader)?;
        Ok(Self { rope })
    }

    /// Saves the buffer to a file.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> io::Result<()> {
        let file = fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        self.rope.write_to(&mut writer)?;
        Ok(())
    }

    /// Returns the total number of characters in the buffer.
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Returns the total number of lines in the buffer.
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// Returns true if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    /// Inserts a character at the given character index.
    pub fn insert_char(&mut self, char_idx: usize, ch: char) {
        let idx = char_idx.min(self.len_chars());
        self.rope.insert_char(idx, ch);
    }

    /// Inserts a string at the given character index.
    pub fn insert(&mut self, char_idx: usize, text: &str) {
        let idx = char_idx.min(self.len_chars());
        self.rope.insert(idx, text);
    }

    /// Removes text in the given character range.
    pub fn remove(&mut self, start: usize, end: usize) {
        let start = start.min(self.len_chars());
        let end = end.min(self.len_chars());
        if start < end {
            self.rope.remove(start..end);
        }
    }

    /// Returns the character at the given index, if it exists.
    pub fn char_at(&self, char_idx: usize) -> Option<char> {
        if char_idx < self.len_chars() {
            Some(self.rope.char(char_idx))
        } else {
            None
        }
    }

    /// Converts a character index to a (line, column) position.
    /// Both line and column are 0-indexed.
    pub fn char_to_line_col(&self, char_idx: usize) -> (usize, usize) {
        let char_idx = char_idx.min(self.len_chars());
        let line = self.rope.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        let col = char_idx - line_start;
        (line, col)
    }

    /// Converts a (line, column) position to a character index.
    /// Both line and column are 0-indexed.
    pub fn line_col_to_char(&self, line: usize, col: usize) -> usize {
        if line >= self.len_lines() {
            return self.len_chars();
        }
        let line_start = self.rope.line_to_char(line);
        let line_len = self.line_len_chars(line);
        line_start + col.min(line_len)
    }

    /// Returns the length of a line in characters (excluding newline).
    pub fn line_len_chars(&self, line: usize) -> usize {
        if line >= self.len_lines() {
            return 0;
        }
        let line_slice = self.rope.line(line);
        let len = line_slice.len_chars();
        // Subtract newline character if present
        if len > 0 {
            let last_char = line_slice.char(len - 1);
            if last_char == '\n' {
                return len - 1;
            }
        }
        len
    }

    /// Returns the character index of the start of a line.
    pub fn line_start(&self, line: usize) -> usize {
        if line >= self.len_lines() {
            self.len_chars()
        } else {
            self.rope.line_to_char(line)
        }
    }

    /// Returns the character index of the end of a line (before newline).
    pub fn line_end(&self, line: usize) -> usize {
        if line >= self.len_lines() {
            self.len_chars()
        } else {
            self.rope.line_to_char(line) + self.line_len_chars(line)
        }
    }

    /// Returns the line at the given index as a string.
    pub fn line(&self, line: usize) -> Option<String> {
        if line >= self.len_lines() {
            None
        } else {
            let line_slice = self.rope.line(line);
            let mut s = line_slice.to_string();
            // Remove trailing newline for consistency
            if s.ends_with('\n') {
                s.pop();
            }
            Some(s)
        }
    }

    /// Returns an iterator over lines in the given range.
    pub fn lines_range(&self, start: usize, end: usize) -> impl Iterator<Item = String> + '_ {
        let start = start.min(self.len_lines());
        let end = end.min(self.len_lines());
        (start..end).filter_map(|i| self.line(i))
    }

    /// Returns the entire buffer as a string.
    pub fn to_string(&self) -> String {
        self.rope.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer() {
        let buf = TextBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.len_chars(), 0);
        assert_eq!(buf.len_lines(), 1); // Empty buffer has 1 line
    }

    #[test]
    fn test_from_str() {
        let buf = TextBuffer::from_str("hello\nworld");
        assert_eq!(buf.len_chars(), 11);
        assert_eq!(buf.len_lines(), 2);
    }

    #[test]
    fn test_insert_char() {
        let mut buf = TextBuffer::new();
        buf.insert_char(0, 'a');
        buf.insert_char(1, 'b');
        buf.insert_char(2, 'c');
        assert_eq!(buf.to_string(), "abc");
    }

    #[test]
    fn test_insert_string() {
        let mut buf = TextBuffer::new();
        buf.insert(0, "hello");
        buf.insert(5, " world");
        assert_eq!(buf.to_string(), "hello world");
    }

    #[test]
    fn test_remove() {
        let mut buf = TextBuffer::from_str("hello world");
        buf.remove(5, 11);
        assert_eq!(buf.to_string(), "hello");
    }

    #[test]
    fn test_line_operations() {
        let buf = TextBuffer::from_str("line1\nline2\nline3");
        assert_eq!(buf.len_lines(), 3);
        assert_eq!(buf.line(0), Some("line1".to_string()));
        assert_eq!(buf.line(1), Some("line2".to_string()));
        assert_eq!(buf.line(2), Some("line3".to_string()));
        assert_eq!(buf.line(3), None);
    }

    #[test]
    fn test_line_len_chars() {
        let buf = TextBuffer::from_str("abc\ndefgh\n");
        assert_eq!(buf.line_len_chars(0), 3);
        assert_eq!(buf.line_len_chars(1), 5);
        assert_eq!(buf.line_len_chars(2), 0);
    }

    #[test]
    fn test_char_to_line_col() {
        let buf = TextBuffer::from_str("abc\ndefgh");
        assert_eq!(buf.char_to_line_col(0), (0, 0));
        assert_eq!(buf.char_to_line_col(2), (0, 2));
        assert_eq!(buf.char_to_line_col(3), (0, 3)); // newline char
        assert_eq!(buf.char_to_line_col(4), (1, 0));
        assert_eq!(buf.char_to_line_col(6), (1, 2));
    }

    #[test]
    fn test_line_col_to_char() {
        let buf = TextBuffer::from_str("abc\ndefgh");
        assert_eq!(buf.line_col_to_char(0, 0), 0);
        assert_eq!(buf.line_col_to_char(0, 2), 2);
        assert_eq!(buf.line_col_to_char(1, 0), 4);
        assert_eq!(buf.line_col_to_char(1, 2), 6);
    }
}
