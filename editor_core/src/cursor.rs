//! Cursor and selection handling.

use crate::buffer::TextBuffer;

/// Represents a position in the buffer as (line, column).
/// Both are 0-indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// A text selection with an anchor and a cursor position.
/// When anchor == cursor, there is no active selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// The anchor point (where selection started).
    pub anchor: usize,
    /// The cursor position (where selection ends / caret is).
    pub cursor: usize,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            anchor: 0,
            cursor: 0,
        }
    }
}

impl Selection {
    /// Creates a new selection at the given position (no active selection).
    pub fn new(pos: usize) -> Self {
        Self {
            anchor: pos,
            cursor: pos,
        }
    }

    /// Creates a selection from anchor to cursor.
    pub fn with_range(anchor: usize, cursor: usize) -> Self {
        Self { anchor, cursor }
    }

    /// Returns true if there's an active selection (anchor != cursor).
    pub fn has_selection(&self) -> bool {
        self.anchor != self.cursor
    }

    /// Returns the start and end of the selection (ordered).
    pub fn range(&self) -> (usize, usize) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    /// Returns the selected range, or None if no selection.
    pub fn selected_range(&self) -> Option<(usize, usize)> {
        if self.has_selection() {
            Some(self.range())
        } else {
            None
        }
    }

    /// Collapses the selection to the cursor position.
    pub fn collapse(&mut self) {
        self.anchor = self.cursor;
    }

    /// Sets the cursor position, optionally extending the selection.
    pub fn set_cursor(&mut self, pos: usize, extend: bool) {
        self.cursor = pos;
        if !extend {
            self.anchor = pos;
        }
    }
}

/// Cursor manager that handles cursor movement relative to a buffer.
#[derive(Debug, Clone)]
pub struct Cursor {
    /// Current selection (includes cursor position).
    pub selection: Selection,
    /// Preferred column for vertical movement.
    /// This preserves the column when moving through lines of varying length.
    preferred_col: Option<usize>,
}

impl Default for Cursor {
    fn default() -> Self {
        Self::new()
    }
}

impl Cursor {
    /// Creates a new cursor at position 0.
    pub fn new() -> Self {
        Self {
            selection: Selection::default(),
            preferred_col: None,
        }
    }

    /// Returns the current cursor position (character index).
    pub fn position(&self) -> usize {
        self.selection.cursor
    }

    /// Sets the cursor position.
    pub fn set_position(&mut self, pos: usize, extend: bool) {
        self.selection.set_cursor(pos, extend);
        self.preferred_col = None;
    }

    /// Returns true if there's an active selection.
    pub fn has_selection(&self) -> bool {
        self.selection.has_selection()
    }

    /// Returns the selected range if any.
    pub fn selected_range(&self) -> Option<(usize, usize)> {
        self.selection.selected_range()
    }

    /// Collapses the selection to the cursor.
    pub fn collapse_selection(&mut self) {
        self.selection.collapse();
    }

    /// Moves cursor left by one character.
    pub fn move_left(&mut self, _buffer: &TextBuffer, extend: bool) {
        let pos = self.selection.cursor;
        if pos > 0 {
            self.selection.set_cursor(pos - 1, extend);
        } else if !extend && self.has_selection() {
            self.collapse_selection();
        }
        self.preferred_col = None;
    }

    /// Moves cursor right by one character.
    pub fn move_right(&mut self, buffer: &TextBuffer, extend: bool) {
        let pos = self.selection.cursor;
        if pos < buffer.len_chars() {
            self.selection.set_cursor(pos + 1, extend);
        } else if !extend && self.has_selection() {
            self.collapse_selection();
        }
        self.preferred_col = None;
    }

    /// Moves cursor up by one line.
    pub fn move_up(&mut self, buffer: &TextBuffer, extend: bool) {
        let (line, col) = buffer.char_to_line_col(self.selection.cursor);
        
        // Store preferred column on first vertical movement
        if self.preferred_col.is_none() {
            self.preferred_col = Some(col);
        }
        
        if line > 0 {
            let target_col = self.preferred_col.unwrap_or(col);
            let new_pos = buffer.line_col_to_char(line - 1, target_col);
            self.selection.set_cursor(new_pos, extend);
        } else {
            // Already at first line, move to start
            self.selection.set_cursor(0, extend);
            self.preferred_col = None;
        }
    }

    /// Moves cursor down by one line.
    pub fn move_down(&mut self, buffer: &TextBuffer, extend: bool) {
        let (line, col) = buffer.char_to_line_col(self.selection.cursor);
        
        // Store preferred column on first vertical movement
        if self.preferred_col.is_none() {
            self.preferred_col = Some(col);
        }
        
        if line < buffer.len_lines() - 1 {
            let target_col = self.preferred_col.unwrap_or(col);
            let new_pos = buffer.line_col_to_char(line + 1, target_col);
            self.selection.set_cursor(new_pos, extend);
        } else {
            // Already at last line, move to end
            self.selection.set_cursor(buffer.len_chars(), extend);
            self.preferred_col = None;
        }
    }

    /// Moves cursor to the start of the current line.
    pub fn move_to_line_start(&mut self, buffer: &TextBuffer, extend: bool) {
        let (line, _) = buffer.char_to_line_col(self.selection.cursor);
        let new_pos = buffer.line_start(line);
        self.selection.set_cursor(new_pos, extend);
        self.preferred_col = None;
    }

    /// Moves cursor to the end of the current line.
    pub fn move_to_line_end(&mut self, buffer: &TextBuffer, extend: bool) {
        let (line, _) = buffer.char_to_line_col(self.selection.cursor);
        let new_pos = buffer.line_end(line);
        self.selection.set_cursor(new_pos, extend);
        self.preferred_col = None;
    }

    /// Moves cursor to the start of the buffer.
    pub fn move_to_buffer_start(&mut self, extend: bool) {
        self.selection.set_cursor(0, extend);
        self.preferred_col = None;
    }

    /// Moves cursor to the end of the buffer.
    pub fn move_to_buffer_end(&mut self, buffer: &TextBuffer, extend: bool) {
        self.selection.set_cursor(buffer.len_chars(), extend);
        self.preferred_col = None;
    }

    /// Moves cursor up by a page (given number of lines).
    pub fn move_page_up(&mut self, buffer: &TextBuffer, page_lines: usize, extend: bool) {
        let (line, col) = buffer.char_to_line_col(self.selection.cursor);
        
        if self.preferred_col.is_none() {
            self.preferred_col = Some(col);
        }
        
        let target_line = line.saturating_sub(page_lines);
        let target_col = self.preferred_col.unwrap_or(col);
        let new_pos = buffer.line_col_to_char(target_line, target_col);
        self.selection.set_cursor(new_pos, extend);
    }

    /// Moves cursor down by a page (given number of lines).
    pub fn move_page_down(&mut self, buffer: &TextBuffer, page_lines: usize, extend: bool) {
        let (line, col) = buffer.char_to_line_col(self.selection.cursor);
        
        if self.preferred_col.is_none() {
            self.preferred_col = Some(col);
        }
        
        let target_line = (line + page_lines).min(buffer.len_lines().saturating_sub(1));
        let target_col = self.preferred_col.unwrap_or(col);
        let new_pos = buffer.line_col_to_char(target_line, target_col);
        self.selection.set_cursor(new_pos, extend);
    }

    /// Clamps the cursor position to valid buffer bounds.
    pub fn clamp_to_buffer(&mut self, buffer: &TextBuffer) {
        let max = buffer.len_chars();
        if self.selection.cursor > max {
            self.selection.cursor = max;
        }
        if self.selection.anchor > max {
            self.selection.anchor = max;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_range() {
        let sel = Selection::with_range(5, 10);
        assert_eq!(sel.range(), (5, 10));
        
        let sel = Selection::with_range(10, 5);
        assert_eq!(sel.range(), (5, 10));
    }

    #[test]
    fn test_cursor_movement() {
        let buffer = TextBuffer::from_str("hello\nworld");
        let mut cursor = Cursor::new();
        
        // Move right
        cursor.move_right(&buffer, false);
        assert_eq!(cursor.position(), 1);
        
        // Move to end of first line
        for _ in 0..5 {
            cursor.move_right(&buffer, false);
        }
        assert_eq!(cursor.position(), 6); // After newline
        
        // Move down from first line
        cursor.set_position(2, false);
        cursor.move_down(&buffer, false);
        let (line, col) = buffer.char_to_line_col(cursor.position());
        assert_eq!(line, 1);
        assert_eq!(col, 2);
    }

    #[test]
    fn test_preferred_column() {
        let buffer = TextBuffer::from_str("long line here\nshort\nanother long line");
        let mut cursor = Cursor::new();
        
        // Position at column 10 of first line
        cursor.set_position(10, false);
        
        // Move down - should clamp to short line
        cursor.move_down(&buffer, false);
        let (line, _) = buffer.char_to_line_col(cursor.position());
        assert_eq!(line, 1);
        
        // Move down again - should restore to column 10
        cursor.move_down(&buffer, false);
        let (line, col) = buffer.char_to_line_col(cursor.position());
        assert_eq!(line, 2);
        assert_eq!(col, 10);
    }

    #[test]
    fn test_line_navigation() {
        let buffer = TextBuffer::from_str("hello world");
        let mut cursor = Cursor::new();
        cursor.set_position(5, false);
        
        cursor.move_to_line_start(&buffer, false);
        assert_eq!(cursor.position(), 0);
        
        cursor.move_to_line_end(&buffer, false);
        assert_eq!(cursor.position(), 11);
    }

    #[test]
    fn test_selection_extend() {
        let buffer = TextBuffer::from_str("hello");
        let mut cursor = Cursor::new();
        
        // Select "hel"
        cursor.move_right(&buffer, true);
        cursor.move_right(&buffer, true);
        cursor.move_right(&buffer, true);
        
        assert!(cursor.has_selection());
        assert_eq!(cursor.selected_range(), Some((0, 3)));
    }
}
