//! Main editor logic.

use crate::buffer::TextBuffer;
use crate::cursor::{Cursor, MultiCursor, Position};
use crate::history::{EditOperation, History};
use crate::syntax::{Language, SyntaxHighlighter};
use std::io;
use std::path::{Path, PathBuf};

/// The main editor state.
///
/// Note: Does not derive Debug because SyntaxHighlighter contains Parser
/// which doesn't implement Debug.
pub struct Editor {
    /// The text buffer.
    buffer: TextBuffer,
    /// The cursor.
    cursor: Cursor,
    /// Multi-cursor support (additional cursors beyond the main one).
    multi_cursors: MultiCursor,
    /// Undo/redo history.
    history: History,
    /// Current file path, if any.
    file_path: Option<PathBuf>,
    /// Whether the buffer has unsaved changes.
    modified: bool,
    /// Number of visible lines (for page up/down).
    visible_lines: usize,
    /// Number of visible columns.
    visible_cols: usize,
    /// Target vertical scroll offset (first visible line).
    scroll_offset: usize,
    /// Smooth scroll position (can be fractional for animation).
    smooth_scroll: f32,
    /// Horizontal scroll offset (first visible column).
    horizontal_scroll: usize,
    /// Syntax highlighter.
    highlighter: SyntaxHighlighter,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    /// Creates a new empty editor.
    pub fn new() -> Self {
        Self {
            buffer: TextBuffer::new(),
            cursor: Cursor::new(),
            multi_cursors: MultiCursor::new(),
            history: History::default(),
            file_path: None,
            modified: false,
            visible_lines: 40,
            visible_cols: 80,
            scroll_offset: 0,
            smooth_scroll: 0.0,
            horizontal_scroll: 0,
            highlighter: SyntaxHighlighter::new(),
        }
    }

    /// Opens a file in the editor.
    pub fn open_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        self.buffer = TextBuffer::from_file(path)?;
        self.cursor = Cursor::new();
        self.multi_cursors = MultiCursor::new();
        self.history.clear();
        self.file_path = Some(path.to_path_buf());
        self.modified = false;
        self.scroll_offset = 0;
        self.smooth_scroll = 0.0;
        self.horizontal_scroll = 0;

        // Set up syntax highlighting based on file extension
        let language = Language::from_path(path);
        self.highlighter.set_language(language);
        self.reparse_syntax();

        Ok(())
    }

    /// Saves the buffer to the current file path.
    pub fn save(&mut self) -> io::Result<()> {
        if let Some(path) = &self.file_path {
            self.buffer.save_to_file(path)?;
            self.modified = false;
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "No file path set",
            ))
        }
    }

    /// Saves the buffer to a new file path.
    pub fn save_as<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        self.buffer.save_to_file(path)?;
        self.file_path = Some(path.to_path_buf());
        self.modified = false;
        Ok(())
    }

    /// Returns the current file path.
    pub fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    /// Returns whether the buffer has unsaved changes.
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Returns a reference to the buffer.
    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    /// Returns the cursor position as (line, column).
    pub fn cursor_position(&self) -> Position {
        let (line, col) = self.buffer.char_to_line_col(self.cursor.position());
        Position::new(line, col)
    }

    /// Returns the cursor character index.
    pub fn cursor_char_index(&self) -> usize {
        self.cursor.position()
    }

    /// Returns the selected range if any, as character indices.
    pub fn selected_range(&self) -> Option<(usize, usize)> {
        self.cursor.selected_range()
    }

    /// Returns true if there is an active selection.
    pub fn has_selection(&self) -> bool {
        self.cursor.has_selection()
    }

    /// Returns the scroll offset (first visible line).
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Sets the number of visible lines.
    pub fn set_visible_lines(&mut self, lines: usize) {
        self.visible_lines = lines.max(1);
    }

    /// Returns the number of visible lines.
    pub fn visible_lines(&self) -> usize {
        self.visible_lines
    }

    /// Sets the number of visible columns.
    pub fn set_visible_cols(&mut self, cols: usize) {
        self.visible_cols = cols.max(1);
    }

    /// Returns the number of visible columns.
    pub fn visible_cols(&self) -> usize {
        self.visible_cols
    }

    /// Returns the horizontal scroll offset (first visible column).
    pub fn horizontal_scroll(&self) -> usize {
        self.horizontal_scroll
    }

    /// Sets the horizontal scroll offset directly.
    pub fn set_horizontal_scroll(&mut self, offset: usize) {
        self.horizontal_scroll = offset;
    }

    /// Scrolls to ensure the cursor is visible.
    pub fn scroll_to_cursor(&mut self) {
        let (line, col) = self.buffer.char_to_line_col(self.cursor.position());
        
        // Vertical scrolling
        if line < self.scroll_offset {
            self.scroll_offset = line;
        } else if line >= self.scroll_offset + self.visible_lines {
            self.scroll_offset = line - self.visible_lines + 1;
        }

        // Horizontal scrolling with some margin (keep 4 chars visible on each side)
        let margin = 4;
        if col < self.horizontal_scroll + margin {
            self.horizontal_scroll = col.saturating_sub(margin);
        } else if col >= self.horizontal_scroll + self.visible_cols.saturating_sub(margin) {
            self.horizontal_scroll = col.saturating_sub(self.visible_cols.saturating_sub(margin + 1));
        }
    }

    /// Sets the scroll offset directly.
    pub fn set_scroll_offset(&mut self, offset: usize) {
        let max_offset = self.buffer.len_lines().saturating_sub(1);
        self.scroll_offset = offset.min(max_offset);
    }

    /// Returns the smooth scroll position (fractional line offset).
    pub fn smooth_scroll(&self) -> f32 {
        self.smooth_scroll
    }

    /// Updates the smooth scroll animation. Returns true if still animating.
    pub fn update_smooth_scroll(&mut self) -> bool {
        let target = self.scroll_offset as f32;
        let diff = target - self.smooth_scroll;
        
        // If close enough, snap to target
        if diff.abs() < 0.01 {
            self.smooth_scroll = target;
            return false;
        }
        
        // Smooth interpolation (ease-out)
        let speed = 0.15; // Adjust for faster/slower scrolling
        self.smooth_scroll += diff * speed;
        true
    }

    /// Jumps smooth scroll to match the target immediately (no animation).
    pub fn snap_scroll(&mut self) {
        self.smooth_scroll = self.scroll_offset as f32;
    }

    /// Sets the cursor position by line and column.
    pub fn set_cursor_position(&mut self, line: usize, col: usize, extend_selection: bool) {
        let char_pos = self.buffer.line_col_to_char(line, col);
        self.cursor.set_position(char_pos, extend_selection);
        self.scroll_to_cursor();
    }

    // ==================== Text Editing ====================

    /// Inserts a character at the cursor position.
    pub fn insert_char(&mut self, ch: char) {
        self.begin_edit();
        
        // Delete selection first if any
        self.delete_selection_internal();
        
        let pos = self.cursor.position();
        self.buffer.insert_char(pos, ch);
        self.history.record(EditOperation::Insert {
            position: pos,
            text: ch.to_string(),
        });
        
        self.cursor.set_position(pos + 1, false);
        self.finish_edit();
        self.scroll_to_cursor();
    }

    /// Inserts a string at the cursor position.
    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        
        self.begin_edit();
        
        // Delete selection first if any
        self.delete_selection_internal();
        
        let pos = self.cursor.position();
        self.buffer.insert(pos, text);
        self.history.record(EditOperation::Insert {
            position: pos,
            text: text.to_string(),
        });
        
        self.cursor.set_position(pos + text.chars().count(), false);
        self.finish_edit();
        self.scroll_to_cursor();
    }

    /// Inserts a newline at the cursor position.
    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    /// Deletes the character before the cursor (backspace).
    pub fn delete_backward(&mut self) {
        self.begin_edit();
        
        if self.delete_selection_internal() {
            self.finish_edit();
            self.scroll_to_cursor();
            return;
        }
        
        let pos = self.cursor.position();
        if pos > 0 {
            let ch = self.buffer.char_at(pos - 1).unwrap();
            self.buffer.remove(pos - 1, pos);
            self.history.record(EditOperation::Delete {
                position: pos - 1,
                text: ch.to_string(),
            });
            self.cursor.set_position(pos - 1, false);
        }
        
        self.finish_edit();
        self.scroll_to_cursor();
    }

    /// Deletes the character after the cursor (delete key).
    pub fn delete_forward(&mut self) {
        self.begin_edit();
        
        if self.delete_selection_internal() {
            self.finish_edit();
            self.scroll_to_cursor();
            return;
        }
        
        let pos = self.cursor.position();
        if pos < self.buffer.len_chars() {
            let ch = self.buffer.char_at(pos).unwrap();
            self.buffer.remove(pos, pos + 1);
            self.history.record(EditOperation::Delete {
                position: pos,
                text: ch.to_string(),
            });
        }

        self.finish_edit();
        self.scroll_to_cursor();
    }

    /// Deletes the current selection.
    /// Returns true if there was a selection to delete.
    fn delete_selection_internal(&mut self) -> bool {
        if let Some((start, end)) = self.cursor.selected_range() {
            // Get the text being deleted for undo
            let mut deleted = String::new();
            for i in start..end {
                if let Some(ch) = self.buffer.char_at(i) {
                    deleted.push(ch);
                }
            }
            
            self.buffer.remove(start, end);
            self.history.record(EditOperation::Delete {
                position: start,
                text: deleted,
            });
            self.cursor.set_position(start, false);
            true
        } else {
            false
        }
    }

    // ==================== Cursor Movement ====================

    /// Moves cursor left.
    pub fn move_left(&mut self, extend_selection: bool) {
        if !extend_selection && self.has_selection() {
            // Move to start of selection
            let (start, _) = self.cursor.selected_range().unwrap();
            self.cursor.set_position(start, false);
        } else {
            self.cursor.move_left(&self.buffer, extend_selection);
        }
        self.scroll_to_cursor();
    }

    /// Moves cursor right.
    pub fn move_right(&mut self, extend_selection: bool) {
        if !extend_selection && self.has_selection() {
            // Move to end of selection
            let (_, end) = self.cursor.selected_range().unwrap();
            self.cursor.set_position(end, false);
        } else {
            self.cursor.move_right(&self.buffer, extend_selection);
        }
        self.scroll_to_cursor();
    }

    /// Moves cursor up.
    pub fn move_up(&mut self, extend_selection: bool) {
        self.cursor.move_up(&self.buffer, extend_selection);
        self.scroll_to_cursor();
    }

    /// Moves cursor down.
    pub fn move_down(&mut self, extend_selection: bool) {
        self.cursor.move_down(&self.buffer, extend_selection);
        self.scroll_to_cursor();
    }

    /// Moves cursor left by one word.
    pub fn move_word_left(&mut self, extend_selection: bool) {
        self.cursor.move_word_left(&self.buffer, extend_selection);
        self.scroll_to_cursor();
    }

    /// Moves cursor right by one word.
    pub fn move_word_right(&mut self, extend_selection: bool) {
        self.cursor.move_word_right(&self.buffer, extend_selection);
        self.scroll_to_cursor();
    }

    /// Moves cursor to the start of the line.
    pub fn move_to_line_start(&mut self, extend_selection: bool) {
        self.cursor.move_to_line_start(&self.buffer, extend_selection);
        self.scroll_to_cursor();
    }

    /// Smart Home: toggles between first non-whitespace and line start.
    pub fn move_to_line_start_smart(&mut self, extend_selection: bool) {
        self.cursor.move_to_line_start_smart(&self.buffer, extend_selection);
        self.scroll_to_cursor();
    }

    /// Moves cursor to the end of the line.
    pub fn move_to_line_end(&mut self, extend_selection: bool) {
        self.cursor.move_to_line_end(&self.buffer, extend_selection);
        self.scroll_to_cursor();
    }

    /// Moves cursor up by a page.
    pub fn move_page_up(&mut self, extend_selection: bool) {
        self.cursor.move_page_up(&self.buffer, self.visible_lines, extend_selection);
        self.scroll_to_cursor();
    }

    /// Moves cursor down by a page.
    pub fn move_page_down(&mut self, extend_selection: bool) {
        self.cursor.move_page_down(&self.buffer, self.visible_lines, extend_selection);
        self.scroll_to_cursor();
    }

    /// Moves cursor to the start of the buffer.
    pub fn move_to_buffer_start(&mut self, extend_selection: bool) {
        self.cursor.move_to_buffer_start(extend_selection);
        self.scroll_to_cursor();
    }

    /// Moves cursor to the end of the buffer.
    pub fn move_to_buffer_end(&mut self, extend_selection: bool) {
        self.cursor.move_to_buffer_end(&self.buffer, extend_selection);
        self.scroll_to_cursor();
    }

    // ==================== Undo/Redo ====================

    /// Begins a new edit operation.
    fn begin_edit(&mut self) {
        self.history.begin_edit(self.cursor.selection);
    }

    /// Finishes the current edit operation.
    fn finish_edit(&mut self) {
        self.history.set_selection_after(self.cursor.selection);
        self.history.commit_edit();
        self.modified = true;
        // Invalidate syntax cache - will be rebuilt on next render
        self.highlighter.invalidate_cache();
    }

    /// Undoes the last edit.
    pub fn undo(&mut self) {
        if let Some((ops, selection)) = self.history.undo() {
            for op in ops {
                self.apply_operation(&op);
            }
            self.cursor.selection = selection;
            self.cursor.clamp_to_buffer(&self.buffer);
            self.scroll_to_cursor();
            self.highlighter.invalidate_cache();
        }
    }

    /// Redoes the last undone edit.
    pub fn redo(&mut self) {
        if let Some((ops, selection)) = self.history.redo() {
            for op in ops {
                self.apply_operation(&op);
            }
            self.cursor.selection = selection;
            self.cursor.clamp_to_buffer(&self.buffer);
            self.scroll_to_cursor();
            self.highlighter.invalidate_cache();
        }
    }

    /// Applies an edit operation to the buffer.
    fn apply_operation(&mut self, op: &EditOperation) {
        match op {
            EditOperation::Insert { position, text } => {
                self.buffer.insert(*position, text);
            }
            EditOperation::Delete { position, text } => {
                self.buffer.remove(*position, *position + text.chars().count());
            }
        }
    }

    /// Returns true if undo is available.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Returns true if redo is available.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    // ==================== Line Operations ====================

    /// Duplicates the current line (or selected lines).
    pub fn duplicate_line(&mut self) {
        self.begin_edit();

        let (line, _) = self.buffer.char_to_line_col(self.cursor.position());

        // Get the line content with newline
        let line_text = self.buffer.line_with_newline(line).unwrap_or_default();
        let line_start = self.buffer.line_start(line);
        let has_newline = line_text.ends_with('\n');

        // For lines without newline at end (last line), insert newline + content after
        let (actual_insert_pos, actual_text) = if has_newline {
            (line_start, line_text)
        } else {
            let text = format!("\n{}", line_text);
            (self.buffer.len_chars(), text)
        };

        self.buffer.insert(actual_insert_pos, &actual_text);
        self.history.record(EditOperation::Insert {
            position: actual_insert_pos,
            text: actual_text.clone(),
        });

        // Move cursor to duplicated line
        let new_line = line + 1;
        let new_pos = self.buffer.line_start(new_line);
        self.cursor.set_position(new_pos, false);

        self.finish_edit();
        self.scroll_to_cursor();
    }

    /// Moves the current line up.
    pub fn move_line_up(&mut self) {
        let (line, col) = self.buffer.char_to_line_col(self.cursor.position());

        if line == 0 {
            return; // Can't move first line up
        }

        self.begin_edit();

        let line_start = self.buffer.line_start(line);
        let line_end = if line + 1 < self.buffer.len_lines() {
            self.buffer.line_start(line + 1)
        } else {
            self.buffer.len_chars()
        };

        // Get line content
        let mut line_text = String::new();
        for i in line_start..line_end {
            if let Some(ch) = self.buffer.char_at(i) {
                line_text.push(ch);
            }
        }

        // Handle last line (no trailing newline)
        let is_last_line = line + 1 >= self.buffer.len_lines();

        // Delete the current line
        self.buffer.remove(line_start, line_end);
        self.history.record(EditOperation::Delete {
            position: line_start,
            text: line_text.clone(),
        });

        // Insert at the previous line position
        let prev_line_start = self.buffer.line_start(line - 1);

        // Ensure we have a newline if needed
        let insert_text = if is_last_line && !line_text.ends_with('\n') {
            format!("{}\n", line_text)
        } else {
            line_text
        };

        self.buffer.insert(prev_line_start, &insert_text);
        self.history.record(EditOperation::Insert {
            position: prev_line_start,
            text: insert_text.clone(),
        });

        // Restore cursor position on the moved line
        let new_line = line - 1;
        let new_line_len = self.buffer.line_len_chars(new_line);
        let new_col = col.min(new_line_len);
        let new_pos = self.buffer.line_col_to_char(new_line, new_col);
        self.cursor.set_position(new_pos, false);

        self.finish_edit();
        self.scroll_to_cursor();
    }

    /// Moves the current line down.
    pub fn move_line_down(&mut self) {
        let (line, col) = self.buffer.char_to_line_col(self.cursor.position());

        if line + 1 >= self.buffer.len_lines() {
            return; // Can't move last line down
        }

        self.begin_edit();

        let line_start = self.buffer.line_start(line);
        let line_end = self.buffer.line_start(line + 1);

        // Get line content
        let mut line_text = String::new();
        for i in line_start..line_end {
            if let Some(ch) = self.buffer.char_at(i) {
                line_text.push(ch);
            }
        }

        // Delete the current line
        self.buffer.remove(line_start, line_end);
        self.history.record(EditOperation::Delete {
            position: line_start,
            text: line_text.clone(),
        });

        // Insert after what is now the current line (was next line)
        let new_next_line_end = if line + 1 < self.buffer.len_lines() {
            self.buffer.line_start(line + 1)
        } else {
            self.buffer.len_chars()
        };

        // Handle inserting at end of file
        let insert_text = if new_next_line_end == self.buffer.len_chars()
            && !self.buffer.to_string().ends_with('\n') {
            format!("\n{}", line_text.trim_end_matches('\n'))
        } else {
            line_text
        };

        self.buffer.insert(new_next_line_end, &insert_text);
        self.history.record(EditOperation::Insert {
            position: new_next_line_end,
            text: insert_text.clone(),
        });

        // Restore cursor position on the moved line
        let new_line = line + 1;
        let new_line_len = self.buffer.line_len_chars(new_line);
        let new_col = col.min(new_line_len);
        let new_pos = self.buffer.line_col_to_char(new_line, new_col);
        self.cursor.set_position(new_pos, false);

        self.finish_edit();
        self.scroll_to_cursor();
    }

    // ==================== Selection ====================

    /// Selects all text.
    pub fn select_all(&mut self) {
        self.cursor.set_position(0, false);
        self.cursor.set_position(self.buffer.len_chars(), true);
    }

    /// Clears the selection.
    pub fn clear_selection(&mut self) {
        self.cursor.collapse_selection();
    }

    /// Returns the selected text, if any.
    pub fn selected_text(&self) -> Option<String> {
        self.cursor.selected_range().map(|(start, end)| {
            let mut text = String::new();
            for i in start..end {
                if let Some(ch) = self.buffer.char_at(i) {
                    text.push(ch);
                }
            }
            text
        })
    }

    // ==================== Block Selection ====================

    /// Returns true if currently in block selection mode.
    pub fn is_block_selection_mode(&self) -> bool {
        self.cursor.is_block_mode()
    }

    /// Starts block selection at the current cursor position.
    pub fn start_block_selection(&mut self) {
        self.cursor.start_block_selection(&self.buffer);
    }

    /// Exits block selection mode.
    pub fn exit_block_selection(&mut self) {
        self.cursor.exit_block_mode();
    }

    /// Toggles block selection mode.
    pub fn toggle_block_selection(&mut self) {
        if self.cursor.is_block_mode() {
            self.cursor.exit_block_mode();
        } else {
            self.cursor.start_block_selection(&self.buffer);
        }
    }

    /// Extends block selection to the given line and column.
    pub fn extend_block_selection(&mut self, line: usize, col: usize) {
        if self.cursor.is_block_mode() {
            self.cursor.update_block_selection(line, col);
            // Also update the regular cursor position
            let new_pos = self.buffer.line_col_to_char(line, col);
            self.cursor.set_position(new_pos, false);
        }
    }

    /// Returns the block selection if active.
    pub fn get_block_selection(&self) -> Option<&crate::cursor::BlockSelection> {
        self.cursor.get_block_selection()
    }

    /// Returns the selected text in block mode as a vector of strings (one per line).
    pub fn block_selected_text(&self) -> Option<Vec<String>> {
        let block = self.cursor.get_block_selection()?;
        let (top, bottom) = block.bounds();

        let mut lines = Vec::new();
        for line_num in top.line..=bottom.line {
            if let Some((start_col, end_col)) = block.col_range(&self.buffer, line_num) {
                let line_start = self.buffer.line_start(line_num);
                let mut text = String::new();
                for col in start_col..end_col {
                    if let Some(ch) = self.buffer.char_at(line_start + col) {
                        text.push(ch);
                    }
                }
                lines.push(text);
            }
        }
        Some(lines)
    }

    /// Deletes the block selection.
    pub fn delete_block_selection(&mut self) {
        let block = match self.cursor.get_block_selection() {
            Some(b) => *b,
            None => return,
        };

        self.begin_edit();

        let (top, bottom) = block.bounds();

        // Delete from bottom to top to preserve line indices
        for line_num in (top.line..=bottom.line).rev() {
            if let Some((start_col, end_col)) = block.col_range(&self.buffer, line_num) {
                if start_col < end_col {
                    let line_start = self.buffer.line_start(line_num);
                    let start_pos = line_start + start_col;
                    let end_pos = line_start + end_col;

                    // Get text for undo
                    let mut deleted = String::new();
                    for i in start_pos..end_pos {
                        if let Some(ch) = self.buffer.char_at(i) {
                            deleted.push(ch);
                        }
                    }

                    self.buffer.remove(start_pos, end_pos);
                    self.history.record(EditOperation::Delete {
                        position: start_pos,
                        text: deleted,
                    });
                }
            }
        }

        // Move cursor to top-left of selection
        let new_pos = self.buffer.line_col_to_char(top.line, top.col);
        self.cursor.set_position(new_pos, false);
        self.cursor.exit_block_mode();

        self.finish_edit();
        self.scroll_to_cursor();
    }

    /// Inserts text at each line of the block selection.
    pub fn insert_text_at_block(&mut self, text: &str) {
        let block = match self.cursor.get_block_selection() {
            Some(b) => *b,
            None => {
                // Not in block mode, just insert normally
                self.insert_text(text);
                return;
            }
        };

        self.begin_edit();

        let (top, bottom) = block.bounds();
        let insert_col = top.col;

        // Insert from bottom to top to preserve positions
        for line_num in (top.line..=bottom.line).rev() {
            let line_len = self.buffer.line_len_chars(line_num);
            let actual_col = insert_col.min(line_len);
            let line_start = self.buffer.line_start(line_num);
            let insert_pos = line_start + actual_col;

            self.buffer.insert(insert_pos, text);
            self.history.record(EditOperation::Insert {
                position: insert_pos,
                text: text.to_string(),
            });
        }

        // Exit block mode and move cursor
        self.cursor.exit_block_mode();
        let new_pos = self.buffer.line_col_to_char(top.line, insert_col + text.chars().count());
        self.cursor.set_position(new_pos, false);

        self.finish_edit();
        self.scroll_to_cursor();
    }

    // ==================== Multi-Cursor ====================

    /// Returns the number of active cursors.
    pub fn cursor_count(&self) -> usize {
        self.multi_cursors.len()
    }

    /// Returns true if there are multiple cursors active.
    pub fn has_multiple_cursors(&self) -> bool {
        self.multi_cursors.len() > 1
    }

    /// Adds a cursor above the current cursor position.
    pub fn add_cursor_above(&mut self) {
        let (line, col) = self.buffer.char_to_line_col(self.cursor.position());
        if line > 0 {
            let new_line = line - 1;
            let new_col = col.min(self.buffer.line_len_chars(new_line));
            self.multi_cursors.add_cursor_at(&self.buffer, new_line, new_col);
            self.scroll_to_cursor();
        }
    }

    /// Adds a cursor below the current cursor position.
    pub fn add_cursor_below(&mut self) {
        let (line, col) = self.buffer.char_to_line_col(self.cursor.position());
        if line + 1 < self.buffer.len_lines() {
            let new_line = line + 1;
            let new_col = col.min(self.buffer.line_len_chars(new_line));
            self.multi_cursors.add_cursor_at(&self.buffer, new_line, new_col);
            self.scroll_to_cursor();
        }
    }

    /// Adds a cursor at the specified line and column.
    pub fn add_cursor_at(&mut self, line: usize, col: usize) {
        self.multi_cursors.add_cursor_at(&self.buffer, line, col);
    }

    /// Collapses all cursors to the primary cursor.
    pub fn collapse_cursors(&mut self) {
        self.multi_cursors.collapse_to_primary();
    }

    /// Returns all cursor positions for rendering.
    pub fn all_cursor_positions(&self) -> Vec<(usize, usize)> {
        self.multi_cursors
            .positions()
            .iter()
            .map(|&pos| self.buffer.char_to_line_col(pos))
            .collect()
    }

    /// Returns all selection ranges for rendering.
    pub fn all_selection_ranges(&self) -> Vec<Option<(usize, usize)>> {
        self.multi_cursors.selection_ranges()
    }

    // ==================== Syntax Highlighting ====================

    /// Returns a reference to the syntax highlighter.
    pub fn highlighter(&self) -> &SyntaxHighlighter {
        &self.highlighter
    }

    /// Returns a mutable reference to the syntax highlighter.
    pub fn highlighter_mut(&mut self) -> &mut SyntaxHighlighter {
        &mut self.highlighter
    }

    /// Sets the syntax highlighting language.
    pub fn set_language(&mut self, language: Language) {
        self.highlighter.set_language(language);
        self.reparse_syntax();
    }

    /// Returns the current language.
    pub fn language(&self) -> Language {
        self.highlighter.language()
    }

    /// Re-parses the entire buffer for syntax highlighting.
    /// Call this when the buffer content changes significantly.
    pub fn reparse_syntax(&mut self) {
        let source = self.buffer.to_string();
        self.highlighter.parse(&source);
        self.highlighter.build_line_cache(&source, self.buffer.len_lines());
    }

    /// Updates the syntax highlighting cache if needed.
    /// Returns true if the cache was rebuilt.
    pub fn update_syntax_cache(&mut self) -> bool {
        if self.highlighter.is_cache_valid() {
            return false;
        }
        let source = self.buffer.to_string();
        self.highlighter.build_line_cache(&source, self.buffer.len_lines());
        true
    }

    /// Invalidates the syntax cache, forcing a rebuild on next render.
    pub fn invalidate_syntax_cache(&mut self) {
        self.highlighter.invalidate_cache();
    }

    /// Gets the highlight color for a specific position.
    pub fn highlight_color_at(&self, line: usize, col: usize) -> [f32; 4] {
        self.highlighter.color_at(line, col)
    }

    /// Returns true if syntax highlighting is available.
    pub fn has_syntax_highlighting(&self) -> bool {
        self.highlighter.has_highlighting()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_delete() {
        let mut editor = Editor::new();
        
        editor.insert_text("hello");
        assert_eq!(editor.buffer().to_string(), "hello");
        assert_eq!(editor.cursor_char_index(), 5);
        
        editor.delete_backward();
        assert_eq!(editor.buffer().to_string(), "hell");
        
        editor.move_left(false);
        editor.delete_forward();
        assert_eq!(editor.buffer().to_string(), "hel");
    }

    #[test]
    fn test_newline() {
        let mut editor = Editor::new();
        editor.insert_text("hello");
        editor.insert_newline();
        editor.insert_text("world");
        
        assert_eq!(editor.buffer().to_string(), "hello\nworld");
        assert_eq!(editor.buffer().len_lines(), 2);
    }

    #[test]
    fn test_cursor_movement() {
        let mut editor = Editor::new();
        editor.insert_text("hello\nworld");
        
        // Cursor is at end
        assert_eq!(editor.cursor_char_index(), 11);
        
        editor.move_to_line_start(false);
        assert_eq!(editor.cursor_char_index(), 6);
        
        editor.move_up(false);
        assert_eq!(editor.cursor_position().line, 0);
        
        editor.move_to_line_end(false);
        assert_eq!(editor.cursor_char_index(), 5);
    }

    #[test]
    fn test_selection() {
        let mut editor = Editor::new();
        editor.insert_text("hello world");
        
        editor.move_to_buffer_start(false);
        editor.move_right(true);
        editor.move_right(true);
        editor.move_right(true);
        editor.move_right(true);
        editor.move_right(true);
        
        assert!(editor.has_selection());
        assert_eq!(editor.selected_text(), Some("hello".to_string()));
    }

    #[test]
    fn test_undo_redo() {
        let mut editor = Editor::new();
        
        editor.insert_text("hello");
        assert_eq!(editor.buffer().to_string(), "hello");
        
        editor.undo();
        assert_eq!(editor.buffer().to_string(), "");
        
        editor.redo();
        assert_eq!(editor.buffer().to_string(), "hello");
    }

    #[test]
    fn test_delete_selection() {
        let mut editor = Editor::new();
        editor.insert_text("hello world");
        
        // Select "world"
        editor.move_to_buffer_start(false);
        for _ in 0..6 {
            editor.move_right(false);
        }
        for _ in 0..5 {
            editor.move_right(true);
        }
        
        assert_eq!(editor.selected_text(), Some("world".to_string()));
        
        editor.delete_backward();
        assert_eq!(editor.buffer().to_string(), "hello ");
    }

    #[test]
    fn test_modified_flag() {
        let mut editor = Editor::new();
        assert!(!editor.is_modified());

        editor.insert_char('a');
        assert!(editor.is_modified());
    }

    #[test]
    fn test_block_selection() {
        let mut editor = Editor::new();
        editor.insert_text("line1\nline2\nline3");

        // Start block selection at (0, 0)
        editor.move_to_buffer_start(false);
        editor.start_block_selection();
        assert!(editor.is_block_selection_mode());

        // Extend to (2, 3) - should select "lin" on each line
        editor.extend_block_selection(2, 3);

        let selected = editor.block_selected_text().unwrap();
        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0], "lin");
        assert_eq!(selected[1], "lin");
        assert_eq!(selected[2], "lin");

        // Exit block mode
        editor.exit_block_selection();
        assert!(!editor.is_block_selection_mode());
    }

    #[test]
    fn test_block_selection_delete() {
        let mut editor = Editor::new();
        editor.insert_text("abcd\nefgh\nijkl");

        // Select "bc" from each line (columns 1-3)
        editor.move_to_buffer_start(false);
        editor.move_right(false); // Move to column 1
        editor.start_block_selection();
        editor.extend_block_selection(2, 3);

        // Delete the block
        editor.delete_block_selection();

        assert_eq!(editor.buffer().to_string(), "ad\neh\nil");
        assert!(!editor.is_block_selection_mode());
    }
}
