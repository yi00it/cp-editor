//! Main editor logic.

use crate::buffer::TextBuffer;
use crate::cursor::{Cursor, MultiCursor, Position};
use crate::fold::FoldManager;
use crate::history::{EditOperation, History};
use crate::lsp_types::{CompletionItem, Diagnostic, HoverInfo};
use crate::search::{Search, SearchMatch};
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
    /// Search state.
    search: Search,
    /// LSP diagnostics for this buffer.
    diagnostics: Vec<Diagnostic>,
    /// Current hover information (if any).
    hover_info: Option<HoverInfo>,
    /// Current completion items (if any).
    completions: Vec<CompletionItem>,
    /// Document version for LSP (increments on each change).
    document_version: i32,
    /// Whether word wrap is enabled.
    word_wrap: bool,
    /// Wrap width in characters (used when word_wrap is true).
    wrap_width: usize,
    /// Code folding manager.
    fold_manager: FoldManager,
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
            search: Search::new(),
            diagnostics: Vec::new(),
            hover_info: None,
            completions: Vec::new(),
            document_version: 0,
            word_wrap: false,
            wrap_width: 80,
            fold_manager: FoldManager::new(),
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
        self.diagnostics.clear();
        self.hover_info = None;
        self.completions.clear();
        self.document_version = 0;

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

        // Update syntax highlighting based on new file extension
        let language = Language::from_path(path);
        self.highlighter.set_language(language);
        self.reparse_syntax();

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

    // ==================== Word Wrap ====================

    /// Returns whether word wrap is enabled.
    pub fn word_wrap(&self) -> bool {
        self.word_wrap
    }

    /// Enables or disables word wrap.
    pub fn set_word_wrap(&mut self, enabled: bool) {
        self.word_wrap = enabled;
    }

    /// Toggles word wrap.
    pub fn toggle_word_wrap(&mut self) {
        self.word_wrap = !self.word_wrap;
    }

    /// Returns the wrap width in characters.
    pub fn wrap_width(&self) -> usize {
        self.wrap_width
    }

    /// Sets the wrap width in characters.
    pub fn set_wrap_width(&mut self, width: usize) {
        self.wrap_width = width.max(10);
    }

    /// Returns wrapped line segments for rendering.
    /// Each segment is (start_col, end_col) within the line.
    /// If word wrap is disabled, returns a single segment covering the whole line.
    pub fn get_wrapped_line_segments(&self, line: usize) -> Vec<(usize, usize)> {
        if !self.word_wrap {
            // No wrapping - return entire line as one segment
            let line_len = self.buffer.line_len_chars(line);
            return vec![(0, line_len)];
        }

        let line_text = match self.buffer.line(line) {
            Some(text) => text,
            None => return vec![],
        };

        let line_len = line_text.chars().count();
        if line_len == 0 {
            return vec![(0, 0)];
        }

        let mut segments = Vec::new();
        let mut start = 0;

        while start < line_len {
            let end = (start + self.wrap_width).min(line_len);

            // Try to find a word boundary if we're not at the end
            let actual_end = if end < line_len {
                // Look for last space or punctuation within the wrap width
                let search_start = start;
                let text_slice: String = line_text.chars().skip(search_start).take(end - search_start).collect();

                // Find last word boundary (space, tab)
                if let Some(last_space) = text_slice.rfind(|c: char| c == ' ' || c == '\t') {
                    let boundary = search_start + last_space + 1;
                    if boundary > start {
                        boundary
                    } else {
                        end
                    }
                } else {
                    end
                }
            } else {
                end
            };

            segments.push((start, actual_end));
            start = actual_end;
        }

        if segments.is_empty() {
            segments.push((0, 0));
        }

        segments
    }

    /// Returns the total number of visual lines (accounting for word wrap).
    pub fn visual_line_count(&self) -> usize {
        if !self.word_wrap {
            return self.buffer.len_lines();
        }

        let mut count = 0;
        for line in 0..self.buffer.len_lines() {
            count += self.get_wrapped_line_segments(line).len();
        }
        count.max(1)
    }

    // ==================== Code Folding ====================

    /// Returns a reference to the fold manager.
    pub fn fold_manager(&self) -> &FoldManager {
        &self.fold_manager
    }

    /// Returns a mutable reference to the fold manager.
    pub fn fold_manager_mut(&mut self) -> &mut FoldManager {
        &mut self.fold_manager
    }

    /// Detects fold regions in the buffer.
    /// Uses brace matching for brace-based languages, indent-based for others.
    pub fn detect_folds(&mut self) {
        let language = self.highlighter.language();
        match language {
            Language::Python => {
                self.fold_manager.detect_indent_folds(&self.buffer);
            }
            _ => {
                self.fold_manager.detect_brace_folds(&self.buffer);
            }
        }
    }

    /// Toggles the fold at the current cursor line.
    pub fn toggle_fold_at_cursor(&mut self) -> bool {
        let (line, _) = self.buffer.char_to_line_col(self.cursor.position());
        self.fold_manager.toggle_fold_at_line(line)
    }

    /// Toggles the fold at the given line.
    pub fn toggle_fold_at_line(&mut self, line: usize) -> bool {
        self.fold_manager.toggle_fold_at_line(line)
    }

    /// Folds all regions.
    pub fn fold_all(&mut self) {
        self.fold_manager.fold_all();
    }

    /// Unfolds all regions.
    pub fn unfold_all(&mut self) {
        self.fold_manager.unfold_all();
    }

    /// Returns whether the given line is hidden (inside a folded region).
    pub fn is_line_hidden(&self, line: usize) -> bool {
        self.fold_manager.is_line_hidden(line)
    }

    /// Returns whether the given line is the start of a fold region.
    pub fn is_fold_start(&self, line: usize) -> bool {
        self.fold_manager.is_fold_start(line)
    }

    /// Returns whether the fold at the given line is collapsed.
    pub fn is_line_folded(&self, line: usize) -> bool {
        self.fold_manager.is_line_folded(line)
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

    /// Inserts a newline at the cursor position with auto-indentation.
    pub fn insert_newline(&mut self) {
        self.begin_edit();

        // Delete selection first if any
        self.delete_selection_internal();

        let pos = self.cursor.position();
        let (line, _col) = self.buffer.char_to_line_col(pos);

        // Get the indentation of the current line
        let indent = self.get_line_indentation(line);

        // Check if we should add extra indentation (after { or :)
        let extra_indent = self.should_increase_indent(line, pos);

        // Insert newline
        self.buffer.insert_char(pos, '\n');
        self.history.record(EditOperation::Insert {
            position: pos,
            text: "\n".to_string(),
        });

        // Build indentation string
        let mut indent_str = indent.clone();
        if extra_indent {
            // Add one level of indentation (use same style as current line or default to 4 spaces)
            if indent.contains('\t') {
                indent_str.push('\t');
            } else {
                indent_str.push_str("    ");
            }
        }

        // Insert indentation
        if !indent_str.is_empty() {
            self.buffer.insert(pos + 1, &indent_str);
            self.history.record(EditOperation::Insert {
                position: pos + 1,
                text: indent_str.clone(),
            });
        }

        self.cursor.set_position(pos + 1 + indent_str.len(), false);
        self.finish_edit();
        self.scroll_to_cursor();
    }

    /// Gets the indentation (leading whitespace) of a line.
    fn get_line_indentation(&self, line: usize) -> String {
        if let Some(line_text) = self.buffer.line(line) {
            let mut indent = String::new();
            for ch in line_text.chars() {
                if ch == ' ' || ch == '\t' {
                    indent.push(ch);
                } else {
                    break;
                }
            }
            indent
        } else {
            String::new()
        }
    }

    /// Checks if we should increase indentation after this line.
    /// Returns true after opening braces, colons (Python), etc.
    fn should_increase_indent(&self, line: usize, cursor_pos: usize) -> bool {
        let line_start = self.buffer.line_start(line);
        let cursor_col = cursor_pos - line_start;

        if let Some(line_text) = self.buffer.line(line) {
            // Only look at text before cursor
            let text_before_cursor: String = line_text.chars().take(cursor_col).collect();
            let trimmed = text_before_cursor.trim_end();

            // Check for characters that should trigger indent
            if let Some(last_char) = trimmed.chars().last() {
                matches!(last_char, '{' | '[' | '(' | ':')
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Gets the selected text as a string.
    /// Returns None if there's no selection.
    pub fn get_selected_text(&self) -> Option<String> {
        if let Some((start, end)) = self.cursor.selected_range() {
            let mut text = String::new();
            for i in start..end {
                if let Some(ch) = self.buffer.char_at(i) {
                    text.push(ch);
                }
            }
            Some(text)
        } else {
            None
        }
    }

    /// Cuts the selected text (returns it and deletes from buffer).
    /// Returns None if there's no selection.
    pub fn cut_selection(&mut self) -> Option<String> {
        let text = self.get_selected_text();
        if text.is_some() {
            self.begin_edit();
            self.delete_selection_internal();
            self.finish_edit();
            self.scroll_to_cursor();
        }
        text
    }

    /// Pastes text at the cursor position.
    pub fn paste(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.insert_text(text);
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

    /// Toggles line comment on the current line or selected lines.
    pub fn toggle_comment(&mut self) {
        let comment_prefix = match self.highlighter.language().line_comment() {
            Some(prefix) => prefix,
            None => return, // Language doesn't support line comments
        };

        self.begin_edit();

        let cursor_pos = self.cursor.position();
        let (start_line, end_line) = if let Some((sel_start, sel_end)) = self.cursor.selected_range() {
            let (start_line, _) = self.buffer.char_to_line_col(sel_start);
            let (end_line, end_col) = self.buffer.char_to_line_col(sel_end);
            // If selection ends at beginning of line, don't include that line
            let end_line = if end_col == 0 && end_line > start_line {
                end_line - 1
            } else {
                end_line
            };
            (start_line, end_line)
        } else {
            let (line, _) = self.buffer.char_to_line_col(cursor_pos);
            (line, line)
        };

        // Check if all lines are commented (to decide whether to uncomment or comment)
        let all_commented = (start_line..=end_line).all(|line| {
            if let Some(line_text) = self.buffer.line(line) {
                let trimmed = line_text.trim_start();
                trimmed.starts_with(comment_prefix)
            } else {
                true
            }
        });

        // Calculate position adjustments
        let comment_len = comment_prefix.len() + 1; // prefix + space
        let mut total_offset: isize = 0;

        for line in start_line..=end_line {
            let line_start = self.buffer.line_start(line);

            if all_commented {
                // Uncomment: remove the comment prefix
                if let Some(line_text) = self.buffer.line(line) {
                    let first_non_ws = line_text
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .count();

                    let content_start = line_start + first_non_ws;
                    let rest = &line_text[first_non_ws..];

                    if rest.starts_with(comment_prefix) {
                        // Check if there's a space after the prefix
                        let has_space = rest.len() > comment_prefix.len()
                            && rest.chars().nth(comment_prefix.len()) == Some(' ');
                        let remove_len = if has_space { comment_len } else { comment_prefix.len() };

                        let remove_end = (content_start + remove_len).min(self.buffer.len_chars());
                        let removed_text: String = (content_start..remove_end)
                            .filter_map(|i| self.buffer.char_at(i))
                            .collect();

                        self.buffer.remove(content_start, remove_end);
                        self.history.record(EditOperation::Delete {
                            position: content_start,
                            text: removed_text,
                        });

                        total_offset -= remove_len as isize;
                    }
                }
            } else {
                // Comment: add the comment prefix at the start of non-whitespace
                if let Some(line_text) = self.buffer.line(line) {
                    let first_non_ws = line_text
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .count();

                    let insert_pos = line_start + first_non_ws;
                    let insert_text = format!("{} ", comment_prefix);

                    self.buffer.insert(insert_pos, &insert_text);
                    self.history.record(EditOperation::Insert {
                        position: insert_pos,
                        text: insert_text,
                    });

                    total_offset += comment_len as isize;
                }
            }
        }

        // Adjust cursor position
        let new_pos = (cursor_pos as isize + total_offset).max(0) as usize;
        self.cursor.set_position(new_pos.min(self.buffer.len_chars()), false);

        self.finish_edit();
        self.scroll_to_cursor();
    }

    // ==================== Bracket Matching ====================

    /// Finds the matching bracket for the bracket at the given position.
    /// Returns the position of the matching bracket, or None if not found.
    pub fn find_matching_bracket(&self, pos: usize) -> Option<usize> {
        let ch = self.buffer.char_at(pos)?;
        let bracket_pairs = self.highlighter.language().bracket_pairs();

        // Check if character is an opening bracket
        for &(open, close) in bracket_pairs {
            if ch == open {
                return self.find_closing_bracket(pos, open, close);
            } else if ch == close {
                return self.find_opening_bracket(pos, open, close);
            }
        }
        None
    }

    /// Finds the closing bracket starting from pos.
    fn find_closing_bracket(&self, start: usize, open: char, close: char) -> Option<usize> {
        let mut depth = 1;
        let mut pos = start + 1;

        while pos < self.buffer.len_chars() && depth > 0 {
            if let Some(ch) = self.buffer.char_at(pos) {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(pos);
                    }
                }
            }
            pos += 1;
        }
        None
    }

    /// Finds the opening bracket starting from pos.
    fn find_opening_bracket(&self, start: usize, open: char, close: char) -> Option<usize> {
        let mut depth = 1;
        let mut pos = start;

        while pos > 0 && depth > 0 {
            pos -= 1;
            if let Some(ch) = self.buffer.char_at(pos) {
                if ch == close {
                    depth += 1;
                } else if ch == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some(pos);
                    }
                }
            }
        }
        None
    }

    /// Returns the matching bracket position for the current cursor position.
    /// Checks both the character at cursor and the character before cursor.
    pub fn matching_bracket_at_cursor(&self) -> Option<(usize, usize)> {
        let pos = self.cursor.position();

        // Check character at cursor position
        if let Some(match_pos) = self.find_matching_bracket(pos) {
            return Some((pos, match_pos));
        }

        // Check character before cursor
        if pos > 0 {
            if let Some(match_pos) = self.find_matching_bracket(pos - 1) {
                return Some((pos - 1, match_pos));
            }
        }

        None
    }

    /// Inserts a character with auto-close bracket support.
    pub fn insert_char_with_auto_bracket(&mut self, ch: char) {
        let bracket_pairs = self.highlighter.language().bracket_pairs();

        // Check if this is an opening bracket
        for &(open, close) in bracket_pairs {
            if ch == open {
                // Insert both opening and closing bracket
                self.begin_edit();
                self.delete_selection_internal();

                let pos = self.cursor.position();
                let pair = format!("{}{}", open, close);
                self.buffer.insert(pos, &pair);
                self.history.record(EditOperation::Insert {
                    position: pos,
                    text: pair,
                });

                // Position cursor between brackets
                self.cursor.set_position(pos + 1, false);
                self.finish_edit();
                self.scroll_to_cursor();
                return;
            }

            // If typing a closing bracket and the next char is the same closing bracket, just skip
            if ch == close {
                let pos = self.cursor.position();
                if let Some(next_ch) = self.buffer.char_at(pos) {
                    if next_ch == close {
                        self.cursor.set_position(pos + 1, false);
                        self.scroll_to_cursor();
                        return;
                    }
                }
            }
        }

        // Default: insert character normally
        self.insert_char(ch);
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

    /// Syncs the primary cursor position to multi_cursors.
    /// Call this before transitioning to multi-cursor mode.
    fn sync_cursor_to_multi(&mut self) {
        // Set the multi_cursors primary position to match the main cursor
        self.multi_cursors.set_position(self.cursor.position(), false);
    }

    /// Adds a cursor above the current cursor position.
    pub fn add_cursor_above(&mut self) {
        // Sync primary cursor before adding new cursor
        if self.multi_cursors.is_single() {
            self.sync_cursor_to_multi();
        }

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
        // Sync primary cursor before adding new cursor
        if self.multi_cursors.is_single() {
            self.sync_cursor_to_multi();
        }

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
        // Sync primary cursor before adding new cursor
        if self.multi_cursors.is_single() {
            self.sync_cursor_to_multi();
        }
        self.multi_cursors.add_cursor_at(&self.buffer, line, col);
    }

    /// Collapses all cursors to the primary cursor.
    pub fn collapse_cursors(&mut self) {
        self.multi_cursors.collapse_to_primary();
    }

    /// Returns all cursor positions for rendering.
    pub fn all_cursor_positions(&self) -> Vec<(usize, usize)> {
        // When there's only one cursor, use the primary cursor (self.cursor)
        // which is kept in sync with editing operations
        if self.multi_cursors.is_single() {
            vec![self.buffer.char_to_line_col(self.cursor.position())]
        } else {
            // Multi-cursor mode: use positions from multi_cursors
            self.multi_cursors
                .positions()
                .iter()
                .map(|&pos| self.buffer.char_to_line_col(pos))
                .collect()
        }
    }

    /// Returns all selection ranges for rendering.
    pub fn all_selection_ranges(&self) -> Vec<Option<(usize, usize)>> {
        // When there's only one cursor, use the primary cursor's selection
        if self.multi_cursors.is_single() {
            vec![self.cursor.selected_range()]
        } else {
            self.multi_cursors.selection_ranges()
        }
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

    // ==================== Search & Replace ====================

    /// Returns a reference to the search state.
    pub fn search(&self) -> &Search {
        &self.search
    }

    /// Performs a search with the given query.
    /// Returns the number of matches found.
    pub fn find(&mut self, query: &str) -> usize {
        let count = self.search.set_query(query, &self.buffer);
        // Jump to the first match near cursor
        if let Some(match_) = self.search.find_nearest(self.cursor.position()) {
            self.jump_to_match(match_);
        }
        count
    }

    /// Clears the current search.
    pub fn clear_search(&mut self) {
        self.search.clear();
    }

    /// Returns true if there is an active search.
    pub fn has_search(&self) -> bool {
        self.search.is_active()
    }

    /// Moves to the next search match.
    /// Returns true if a match was found.
    pub fn find_next(&mut self) -> bool {
        if let Some(match_) = self.search.next_match() {
            self.jump_to_match(match_);
            true
        } else {
            false
        }
    }

    /// Moves to the previous search match.
    /// Returns true if a match was found.
    pub fn find_prev(&mut self) -> bool {
        if let Some(match_) = self.search.prev_match() {
            self.jump_to_match(match_);
            true
        } else {
            false
        }
    }

    /// Jumps to the given search match position.
    fn jump_to_match(&mut self, match_: SearchMatch) {
        // Set cursor to the start of the match
        self.cursor.set_position(match_.start, false);
        // Select the match
        self.cursor.set_position(match_.end, true);
        self.scroll_to_cursor();
    }

    /// Returns the current search matches visible in the given line range.
    pub fn search_matches_in_range(&self, start_line: usize, end_line: usize) -> Vec<SearchMatch> {
        self.search.matches_in_range(&self.buffer, start_line, end_line)
    }

    /// Returns all search matches.
    pub fn search_matches(&self) -> &[SearchMatch] {
        self.search.matches()
    }

    /// Returns the current (highlighted) search match.
    pub fn current_search_match(&self) -> Option<SearchMatch> {
        self.search.current_match()
    }

    /// Returns search status string like "1 of 5".
    pub fn search_status(&self) -> Option<String> {
        if !self.search.is_active() {
            return None;
        }
        let count = self.search.match_count();
        if count == 0 {
            return Some("No results".to_string());
        }
        if let Some(current) = self.search.current_match_index() {
            Some(format!("{} of {}", current, count))
        } else {
            Some(format!("{} results", count))
        }
    }

    /// Toggles case sensitivity for search.
    pub fn toggle_search_case_sensitive(&mut self) {
        self.search.toggle_case_sensitive(&self.buffer);
        // Re-jump to nearest match if any
        if let Some(match_) = self.search.find_nearest(self.cursor.position()) {
            self.jump_to_match(match_);
        }
    }

    /// Replaces the current search match with the given replacement text.
    /// Returns true if a replacement was made.
    pub fn replace_current(&mut self, replacement: &str) -> bool {
        let Some(match_) = self.search.current_match() else {
            return false;
        };

        self.begin_edit();

        // Delete the match text
        let mut deleted = String::new();
        for i in match_.start..match_.end {
            if let Some(ch) = self.buffer.char_at(i) {
                deleted.push(ch);
            }
        }
        self.buffer.remove(match_.start, match_.end);
        self.history.record(EditOperation::Delete {
            position: match_.start,
            text: deleted,
        });

        // Insert the replacement
        self.buffer.insert(match_.start, replacement);
        self.history.record(EditOperation::Insert {
            position: match_.start,
            text: replacement.to_string(),
        });

        // Move cursor after the replacement
        self.cursor.set_position(match_.start + replacement.chars().count(), false);

        self.finish_edit();

        // Refresh search to update matches
        self.search.refresh(&self.buffer);

        // Jump to next match if available
        if let Some(next) = self.search.current_match() {
            self.jump_to_match(next);
        }

        true
    }

    /// Replaces all search matches with the given replacement text.
    /// Returns the number of replacements made.
    pub fn replace_all(&mut self, replacement: &str) -> usize {
        let matches: Vec<_> = self.search.matches().to_vec();
        if matches.is_empty() {
            return 0;
        }

        self.begin_edit();

        let replacement_char_count = replacement.chars().count();
        let mut offset: isize = 0;

        for m in &matches {
            // Adjust position based on previous replacements
            let adjusted_start = (m.start as isize + offset) as usize;
            let adjusted_end = (m.end as isize + offset) as usize;

            // Delete the match text
            let mut deleted = String::new();
            for i in adjusted_start..adjusted_end {
                if let Some(ch) = self.buffer.char_at(i) {
                    deleted.push(ch);
                }
            }
            self.buffer.remove(adjusted_start, adjusted_end);
            self.history.record(EditOperation::Delete {
                position: adjusted_start,
                text: deleted,
            });

            // Insert the replacement
            self.buffer.insert(adjusted_start, replacement);
            self.history.record(EditOperation::Insert {
                position: adjusted_start,
                text: replacement.to_string(),
            });

            // Update offset for subsequent replacements
            offset += replacement_char_count as isize - m.len() as isize;
        }

        let count = matches.len();

        self.finish_edit();

        // Clear search after replace all
        self.search.clear();

        count
    }

    // ==================== Go to Line ====================

    /// Moves the cursor to the specified line number (1-based).
    /// Returns true if the line exists.
    pub fn go_to_line(&mut self, line_number: usize) -> bool {
        if line_number == 0 {
            return false;
        }

        let line_idx = line_number - 1; // Convert to 0-based
        if line_idx >= self.buffer.len_lines() {
            return false;
        }

        let line_start = self.buffer.line_start(line_idx);
        self.cursor.set_position(line_start, false);
        self.scroll_to_cursor();
        true
    }

    /// Moves the cursor to the specified line and column (both 1-based).
    pub fn go_to_line_col(&mut self, line_number: usize, col_number: usize) -> bool {
        if line_number == 0 {
            return false;
        }

        let line_idx = line_number - 1;
        if line_idx >= self.buffer.len_lines() {
            return false;
        }

        let col_idx = col_number.saturating_sub(1);
        let char_pos = self.buffer.line_col_to_char(line_idx, col_idx);
        self.cursor.set_position(char_pos, false);
        self.scroll_to_cursor();
        true
    }

    // ==================== LSP Integration ====================

    /// Returns the document version (increments on each change).
    pub fn document_version(&self) -> i32 {
        self.document_version
    }

    /// Increments the document version. Call after each text change.
    pub fn increment_document_version(&mut self) {
        self.document_version += 1;
    }

    /// Sets the diagnostics for this buffer.
    pub fn set_diagnostics(&mut self, diagnostics: Vec<Diagnostic>) {
        self.diagnostics = diagnostics;
    }

    /// Returns the diagnostics for this buffer.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns diagnostics for a specific line.
    pub fn diagnostics_on_line(&self, line: usize) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| d.on_line(line)).collect()
    }

    /// Returns the diagnostic at the given position, if any.
    pub fn diagnostic_at(&self, line: usize, col: usize) -> Option<&Diagnostic> {
        self.diagnostics.iter().find(|d| d.contains(line, col))
    }

    /// Clears all diagnostics.
    pub fn clear_diagnostics(&mut self) {
        self.diagnostics.clear();
    }

    /// Sets the hover information.
    pub fn set_hover_info(&mut self, info: Option<HoverInfo>) {
        self.hover_info = info;
    }

    /// Returns the current hover information.
    pub fn hover_info(&self) -> Option<&HoverInfo> {
        self.hover_info.as_ref()
    }

    /// Clears the hover information.
    pub fn clear_hover_info(&mut self) {
        self.hover_info = None;
    }

    /// Sets the completion items.
    pub fn set_completions(&mut self, items: Vec<CompletionItem>) {
        self.completions = items;
    }

    /// Returns the current completion items.
    pub fn completions(&self) -> &[CompletionItem] {
        &self.completions
    }

    /// Clears the completion items.
    pub fn clear_completions(&mut self) {
        self.completions.clear();
    }

    /// Returns true if there are active completions.
    pub fn has_completions(&self) -> bool {
        !self.completions.is_empty()
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
