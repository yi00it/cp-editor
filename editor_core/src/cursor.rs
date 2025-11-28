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

/// Represents a block (rectangular/column) selection.
/// This is separate from regular selection and allows selecting
/// a rectangular region of text across multiple lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSelection {
    /// The anchor position (where block selection started).
    pub anchor: Position,
    /// The cursor position (current corner of the block).
    pub cursor: Position,
}

impl BlockSelection {
    /// Creates a new block selection at the given position.
    pub fn new(pos: Position) -> Self {
        Self {
            anchor: pos,
            cursor: pos,
        }
    }

    /// Returns the top-left and bottom-right corners of the block.
    pub fn bounds(&self) -> (Position, Position) {
        let top_line = self.anchor.line.min(self.cursor.line);
        let bottom_line = self.anchor.line.max(self.cursor.line);
        let left_col = self.anchor.col.min(self.cursor.col);
        let right_col = self.anchor.col.max(self.cursor.col);

        (
            Position::new(top_line, left_col),
            Position::new(bottom_line, right_col),
        )
    }

    /// Returns the range of lines covered by this block selection.
    pub fn line_range(&self) -> std::ops::RangeInclusive<usize> {
        let (top, bottom) = self.bounds();
        top.line..=bottom.line
    }

    /// Returns the column range for a given line.
    /// Takes into account that lines may be shorter than the selection.
    pub fn col_range(&self, buffer: &TextBuffer, line: usize) -> Option<(usize, usize)> {
        let (top, bottom) = self.bounds();
        if line < top.line || line > bottom.line {
            return None;
        }

        let line_len = buffer.line_len_chars(line);
        let start_col = top.col.min(line_len);
        let end_col = bottom.col.min(line_len);

        if start_col < end_col {
            Some((start_col, end_col))
        } else {
            // Empty selection on this line (cursor column past line end)
            Some((start_col, start_col))
        }
    }

    /// Returns true if this block selection covers multiple lines or columns.
    pub fn is_non_empty(&self) -> bool {
        self.anchor.line != self.cursor.line || self.anchor.col != self.cursor.col
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

/// Selection mode - determines how selections are interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    /// Normal character-based selection.
    #[default]
    Normal,
    /// Block (rectangular/column) selection mode.
    Block,
}

/// Cursor manager that handles cursor movement relative to a buffer.
#[derive(Debug, Clone)]
pub struct Cursor {
    /// Current selection (includes cursor position).
    pub selection: Selection,
    /// Block selection (when in block mode).
    pub block_selection: Option<BlockSelection>,
    /// Current selection mode.
    pub selection_mode: SelectionMode,
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
            block_selection: None,
            selection_mode: SelectionMode::Normal,
            preferred_col: None,
        }
    }

    /// Returns true if in block selection mode.
    pub fn is_block_mode(&self) -> bool {
        self.selection_mode == SelectionMode::Block
    }

    /// Enables block selection mode at the current position.
    pub fn start_block_selection(&mut self, buffer: &TextBuffer) {
        let (line, col) = buffer.char_to_line_col(self.selection.cursor);
        self.block_selection = Some(BlockSelection::new(Position::new(line, col)));
        self.selection_mode = SelectionMode::Block;
    }

    /// Exits block selection mode.
    pub fn exit_block_mode(&mut self) {
        self.block_selection = None;
        self.selection_mode = SelectionMode::Normal;
    }

    /// Updates the block selection cursor position.
    pub fn update_block_selection(&mut self, line: usize, col: usize) {
        if let Some(block) = &mut self.block_selection {
            block.cursor = Position::new(line, col);
        }
    }

    /// Returns the block selection if active.
    pub fn get_block_selection(&self) -> Option<&BlockSelection> {
        if self.selection_mode == SelectionMode::Block {
            self.block_selection.as_ref()
        } else {
            None
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

    /// Moves cursor left by one word.
    pub fn move_word_left(&mut self, buffer: &TextBuffer, extend: bool) {
        let new_pos = buffer.find_word_boundary_left(self.selection.cursor);
        self.selection.set_cursor(new_pos, extend);
        self.preferred_col = None;
    }

    /// Moves cursor right by one word.
    pub fn move_word_right(&mut self, buffer: &TextBuffer, extend: bool) {
        let new_pos = buffer.find_word_boundary_right(self.selection.cursor);
        self.selection.set_cursor(new_pos, extend);
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

    /// Smart Home: toggles between first non-whitespace character and line start.
    /// If cursor is at first non-whitespace, moves to line start.
    /// Otherwise, moves to first non-whitespace character.
    pub fn move_to_line_start_smart(&mut self, buffer: &TextBuffer, extend: bool) {
        let (line, col) = buffer.char_to_line_col(self.selection.cursor);
        let line_start = buffer.line_start(line);
        let first_non_ws = buffer.first_non_whitespace_col(line);
        let line_len = buffer.line_len_chars(line);

        // Check if line has any non-whitespace content
        // first_non_whitespace_col returns 0 for all-whitespace lines, but also for
        // lines starting with non-whitespace. We need to distinguish these cases.
        let has_non_ws = first_non_ws < line_len
            && buffer
                .char_at(line_start + first_non_ws)
                .map(|c| !c.is_whitespace())
                .unwrap_or(false);

        if !has_non_ws {
            // All whitespace line - just go to line start
            self.selection.set_cursor(line_start, extend);
            self.preferred_col = None;
            return;
        }

        let new_pos = if col == 0 {
            // At line start, go to first non-whitespace
            line_start + first_non_ws
        } else if col <= first_non_ws {
            // At or before first non-whitespace, go to line start
            line_start
        } else {
            // After first non-whitespace, go to first non-whitespace
            line_start + first_non_ws
        };

        // If we're already at that position, toggle to the other
        if new_pos == self.selection.cursor {
            let alt_pos = if new_pos == line_start {
                line_start + first_non_ws
            } else {
                line_start
            };
            self.selection.set_cursor(alt_pos, extend);
        } else {
            self.selection.set_cursor(new_pos, extend);
        }
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

/// Multi-cursor manager that handles multiple independent cursors.
#[derive(Debug, Clone)]
pub struct MultiCursor {
    /// All cursors (sorted by position).
    cursors: Vec<Cursor>,
    /// Index of the "primary" cursor (the main one that was created first or most recently used).
    primary_index: usize,
}

impl Default for MultiCursor {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiCursor {
    /// Creates a new multi-cursor with a single cursor at position 0.
    pub fn new() -> Self {
        Self {
            cursors: vec![Cursor::new()],
            primary_index: 0,
        }
    }

    /// Returns the number of cursors.
    pub fn len(&self) -> usize {
        self.cursors.len()
    }

    /// Returns true if there's only one cursor.
    pub fn is_single(&self) -> bool {
        self.cursors.len() == 1
    }

    /// Returns a reference to the primary cursor.
    pub fn primary(&self) -> &Cursor {
        &self.cursors[self.primary_index]
    }

    /// Returns a mutable reference to the primary cursor.
    pub fn primary_mut(&mut self) -> &mut Cursor {
        &mut self.cursors[self.primary_index]
    }

    /// Returns an iterator over all cursors.
    pub fn iter(&self) -> impl Iterator<Item = &Cursor> {
        self.cursors.iter()
    }

    /// Returns a mutable iterator over all cursors.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Cursor> {
        self.cursors.iter_mut()
    }

    /// Adds a new cursor at the given character position.
    /// Returns true if the cursor was added (not a duplicate).
    pub fn add_cursor(&mut self, pos: usize) -> bool {
        // Check if cursor already exists at this position
        for cursor in &self.cursors {
            if cursor.position() == pos {
                return false;
            }
        }

        let mut new_cursor = Cursor::new();
        new_cursor.set_position(pos, false);
        self.cursors.push(new_cursor);
        self.normalize();
        true
    }

    /// Adds a new cursor at the given line and column.
    pub fn add_cursor_at(&mut self, buffer: &TextBuffer, line: usize, col: usize) -> bool {
        let pos = buffer.line_col_to_char(line, col);
        self.add_cursor(pos)
    }

    /// Removes all secondary cursors, keeping only the primary.
    pub fn collapse_to_primary(&mut self) {
        let primary = self.cursors[self.primary_index].clone();
        self.cursors = vec![primary];
        self.primary_index = 0;
    }

    /// Removes the cursor at the given index.
    pub fn remove_cursor(&mut self, index: usize) {
        if self.cursors.len() <= 1 {
            return; // Can't remove the last cursor
        }

        self.cursors.remove(index);

        // Adjust primary index if needed
        if self.primary_index >= self.cursors.len() {
            self.primary_index = self.cursors.len() - 1;
        } else if self.primary_index > index {
            self.primary_index -= 1;
        }
    }

    /// Normalizes cursors: sorts by position and merges overlapping selections.
    pub fn normalize(&mut self) {
        if self.cursors.len() <= 1 {
            return;
        }

        // Remember primary cursor position before sort
        let primary_pos = self.cursors[self.primary_index].position();

        // Sort cursors by position
        self.cursors.sort_by_key(|c| c.position());

        // Find new primary index after sort
        self.primary_index = self
            .cursors
            .iter()
            .position(|c| c.position() == primary_pos)
            .unwrap_or(0);

        // Merge overlapping selections
        let mut i = 0;
        while i + 1 < self.cursors.len() {
            let (left, right) = self.cursors.split_at_mut(i + 1);
            let a = &left[i];
            let b = &right[0];

            // Check if selections overlap
            let a_range = a.selection.range();
            let b_range = b.selection.range();

            if a_range.1 >= b_range.0 {
                // Merge: extend the first cursor to cover both ranges
                let merged_start = a_range.0.min(b_range.0);
                let merged_end = a_range.1.max(b_range.1);

                self.cursors[i].selection.anchor = merged_start;
                self.cursors[i].selection.cursor = merged_end;
                self.cursors.remove(i + 1);

                // Adjust primary if we removed it
                if self.primary_index > i + 1 {
                    self.primary_index -= 1;
                } else if self.primary_index == i + 1 {
                    self.primary_index = i;
                }
            } else {
                i += 1;
            }
        }
    }

    /// Moves all cursors, applying an offset to account for text changes.
    /// Used after insert/delete operations.
    pub fn adjust_positions(&mut self, from: usize, delta: isize) {
        for cursor in &mut self.cursors {
            if cursor.selection.cursor >= from {
                cursor.selection.cursor =
                    (cursor.selection.cursor as isize + delta).max(0) as usize;
            }
            if cursor.selection.anchor >= from {
                cursor.selection.anchor =
                    (cursor.selection.anchor as isize + delta).max(0) as usize;
            }
        }
    }

    /// Clamps all cursors to valid buffer bounds.
    pub fn clamp_to_buffer(&mut self, buffer: &TextBuffer) {
        for cursor in &mut self.cursors {
            cursor.clamp_to_buffer(buffer);
        }
    }

    /// Sets the position of the primary cursor (and collapses to single cursor mode).
    pub fn set_position(&mut self, pos: usize, extend: bool) {
        // When setting position normally, collapse to single cursor
        if !extend && self.cursors.len() > 1 {
            self.collapse_to_primary();
        }
        self.primary_mut().set_position(pos, extend);
    }

    /// Returns the primary cursor's selection.
    pub fn selection(&self) -> Selection {
        self.primary().selection
    }

    /// Returns all cursor positions (for rendering).
    pub fn positions(&self) -> Vec<usize> {
        self.cursors.iter().map(|c| c.position()).collect()
    }

    /// Returns all selection ranges (for rendering).
    pub fn selection_ranges(&self) -> Vec<Option<(usize, usize)>> {
        self.cursors.iter().map(|c| c.selected_range()).collect()
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

    #[test]
    fn test_multi_cursor_add() {
        let mut mc = MultiCursor::new();
        assert_eq!(mc.len(), 1);

        mc.add_cursor(5);
        assert_eq!(mc.len(), 2);

        mc.add_cursor(10);
        assert_eq!(mc.len(), 3);

        // Adding duplicate should not increase count
        mc.add_cursor(5);
        assert_eq!(mc.len(), 3);
    }

    #[test]
    fn test_multi_cursor_normalize() {
        let mut mc = MultiCursor::new();

        // Add cursors in random order
        mc.add_cursor(10);
        mc.add_cursor(5);
        mc.add_cursor(15);

        // After normalize (called by add_cursor), should be sorted
        let positions = mc.positions();
        assert_eq!(positions, vec![0, 5, 10, 15]);
    }

    #[test]
    fn test_multi_cursor_collapse() {
        let mut mc = MultiCursor::new();
        mc.add_cursor(5);
        mc.add_cursor(10);
        assert_eq!(mc.len(), 3);

        mc.collapse_to_primary();
        assert_eq!(mc.len(), 1);
    }

    #[test]
    fn test_multi_cursor_adjust_positions() {
        let mut mc = MultiCursor::new();
        mc.primary_mut().set_position(5, false);
        mc.add_cursor(10);
        mc.add_cursor(15);

        // Simulate inserting 3 chars at position 8
        mc.adjust_positions(8, 3);

        let positions = mc.positions();
        // Position 5 is before 8, unchanged
        // Positions 10 and 15 are after 8, increased by 3
        assert_eq!(positions, vec![5, 13, 18]);
    }
}
