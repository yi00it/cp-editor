//! Main editor application with GPU rendering.

use crate::gpu_renderer::GpuRenderer;
use crate::input::{EditorCommand, InputHandler};
use crate::lsp::{language_id_from_path, LspEvent, LspManager};
use crate::notifications::NotificationManager;
use cp_editor_core::lsp_types::{CompletionItem, DiagnosticSeverity};
use cp_editor_core::Workspace;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

/// Cursor blink interval in milliseconds.
const CURSOR_BLINK_INTERVAL_MS: u64 = 530;

/// Tab bar height in pixels.
const TAB_BAR_HEIGHT: f32 = 28.0;

/// Search bar height in pixels.
const SEARCH_BAR_HEIGHT: f32 = 32.0;

/// Status bar height in pixels.
const STATUS_BAR_HEIGHT: f32 = 24.0;

/// Input mode for the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal editing mode.
    Normal,
    /// Search mode (Ctrl+F).
    Search,
    /// Replace mode (Ctrl+H).
    Replace,
    /// Go to line mode (Ctrl+G).
    GoToLine,
    /// Rename symbol mode (F2).
    Rename,
}

/// Pending dialog action after unsaved changes confirmation.
#[derive(Debug, Clone)]
pub enum PendingAction {
    /// Close the specified buffer.
    CloseBuffer(cp_editor_core::BufferId),
    /// Quit the application.
    Quit,
    /// Open a file (after closing current unsaved).
    OpenFile,
}

/// The main editor application.
pub struct EditorApp {
    /// The workspace managing multiple buffers.
    pub workspace: Workspace,
    /// Input handler.
    pub input_handler: InputHandler,
    /// Font size.
    pub font_size: f32,
    /// Left margin for line numbers.
    pub line_number_margin: f32,
    /// Whether the cursor is currently visible (for blinking).
    pub cursor_visible: bool,
    /// Last time the cursor blink state changed.
    pub last_cursor_blink: Instant,
    /// Whether the cursor should blink (disabled during typing).
    pub cursor_blink_enabled: bool,
    /// Pending action requiring confirmation.
    pub pending_action: Option<PendingAction>,
    /// Whether a file dialog is currently open.
    pub dialog_open: bool,
    /// Current input mode.
    pub input_mode: InputMode,
    /// Search query text.
    pub search_text: String,
    /// Replace text.
    pub replace_text: String,
    /// Go to line text.
    pub goto_text: String,
    /// Rename symbol text.
    pub rename_text: String,
    /// Which input field is focused (0 = search, 1 = replace).
    pub focused_field: usize,
    /// LSP manager for language server integration.
    pub lsp_manager: LspManager,
    /// Last mouse position for hover (screen coordinates).
    pub hover_mouse_pos: Option<(f32, f32)>,
    /// Last hover request time.
    pub hover_request_time: Option<Instant>,
    /// Whether we're waiting for a hover response.
    pub hover_pending: bool,
    /// Whether the completion popup is visible.
    pub completion_visible: bool,
    /// Selected completion item index.
    pub completion_selected: usize,
    /// Position where completion was triggered (line, col).
    pub completion_trigger_pos: Option<(usize, usize)>,
    /// Notification manager for user feedback.
    pub notifications: NotificationManager,
    /// Whether a document change is waiting to be sent to LSP.
    pub pending_lsp_change: bool,
    /// Timestamp of the last buffered document change.
    pub last_lsp_change: Option<Instant>,
    /// Debounce duration for LSP didChange.
    pub lsp_change_debounce: Duration,
}

impl EditorApp {
    /// Creates a new editor application.
    pub fn new(font_size: f32) -> Self {
        let mut workspace = Workspace::new();
        // Create initial empty buffer
        workspace.new_buffer();

        Self {
            workspace,
            input_handler: InputHandler::new(),
            font_size,
            line_number_margin: 60.0,
            cursor_visible: true,
            last_cursor_blink: Instant::now(),
            cursor_blink_enabled: true,
            pending_action: None,
            dialog_open: false,
            input_mode: InputMode::Normal,
            search_text: String::new(),
            replace_text: String::new(),
            goto_text: String::new(),
            rename_text: String::new(),
            focused_field: 0,
            lsp_manager: LspManager::new(),
            hover_mouse_pos: None,
            hover_request_time: None,
            hover_pending: false,
            completion_visible: false,
            completion_selected: 0,
            completion_trigger_pos: None,
            notifications: NotificationManager::new(),
            pending_lsp_change: false,
            last_lsp_change: None,
            lsp_change_debounce: Duration::from_millis(40),
        }
    }

    /// Polls LSP for events and processes them.
    pub fn poll_lsp(&mut self) {
        let events = self.lsp_manager.poll();
        for event in events {
            self.handle_lsp_event(event);
        }
    }

    /// Handles an LSP event.
    fn handle_lsp_event(&mut self, event: LspEvent) {
        match event {
            LspEvent::Diagnostics { path, diagnostics } => {
                // Find the editor for this path and set diagnostics
                if let Some((_id, editor)) = self.workspace.editors_mut().find(|(_, e)| {
                    e.file_path() == Some(path.as_path())
                }) {
                    editor.set_diagnostics(diagnostics);
                    log::debug!("Updated diagnostics for {:?}", path);
                }
            }
            LspEvent::Hover { path, info } => {
                // Find the editor for this path and set hover info
                self.hover_pending = false;
                if let Some((_, editor)) = self.workspace.editors_mut().find(|(_, e)| {
                    e.file_path() == Some(path.as_path())
                }) {
                    editor.set_hover_info(info);
                }
            }
            LspEvent::Completion { path, items } => {
                // Find the editor for this path and set completions
                if let Some((_, editor)) = self.workspace.editors_mut().find(|(_, e)| {
                    e.file_path() == Some(path.as_path())
                }) {
                    let has_items = !items.is_empty();
                    editor.set_completions(items);
                    // Show completion popup if we have items
                    if has_items {
                        self.completion_visible = true;
                        self.completion_selected = 0;
                    } else {
                        self.completion_visible = false;
                    }
                }
            }
            LspEvent::GotoDefinition { path: _, locations } => {
                // Jump to the first location
                if let Some((def_path, line, col)) = locations.into_iter().next() {
                    // Open the file and go to the location
                    if let Ok(id) = self.workspace.open_file(&def_path) {
                        self.workspace.set_active(id);
                        if let Some(editor) = self.workspace.active_editor_mut() {
                            editor.go_to_line_col(line + 1, col + 1);
                        }
                    }
                }
            }
            LspEvent::Rename { edits } => {
                // Apply workspace edits from rename
                let mut total_edits = 0;
                let mut files_changed = 0;

                // Store original active buffer to restore later
                let original_active = self.workspace.active_buffer_id();

                for (path, file_edits) in edits {
                    // First, find if file is already open (separate scope to release borrow)
                    let existing_id = {
                        self.workspace.editors()
                            .find(|(_, e)| e.file_path() == Some(path.as_path()))
                            .map(|(id, _)| id)
                    };

                    // Open or use existing
                    let editor_id = if let Some(id) = existing_id {
                        Some(id)
                    } else if let Ok(id) = self.workspace.open_file(&path) {
                        Some(id)
                    } else {
                        log::error!("Failed to open file for rename: {:?}", path);
                        None
                    };

                    if let Some(id) = editor_id {
                        // Set this buffer as active to get mutable access
                        self.workspace.set_active(id);
                        if let Some(editor) = self.workspace.active_editor_mut() {
                            // Apply edits in reverse order to preserve positions
                            let mut sorted_edits = file_edits;
                            sorted_edits.sort_by(|a, b| {
                                (b.0, b.1).cmp(&(a.0, a.1))
                            });
                            for (start_line, start_col, end_line, end_col, new_text) in sorted_edits {
                                editor.replace_range(start_line, start_col, end_line, end_col, &new_text);
                                total_edits += 1;
                            }
                            files_changed += 1;
                        }
                    }
                }

                // Restore original active buffer
                if let Some(id) = original_active {
                    self.workspace.set_active(id);
                }

                if total_edits > 0 {
                    self.notifications.success(format!(
                        "Renamed: {} occurrences in {} file(s)",
                        total_edits, files_changed
                    ));
                }
            }
            LspEvent::ServerReady { language } => {
                log::info!("LSP server ready for {}", language);
            }
            LspEvent::Error { message } => {
                log::error!("LSP error: {}", message);
            }
        }
    }

    /// Notifies LSP that the active document changed.
    pub fn notify_lsp_document_change(&mut self) {
        self.pending_lsp_change = true;
        self.last_lsp_change = Some(Instant::now());
    }

    /// Notifies LSP that a file was opened.
    pub fn notify_lsp_file_opened(&mut self) {
        if let Some(editor) = self.workspace.active_editor() {
            if let Some(path) = editor.file_path() {
                // Set workspace root if not already set (use parent directory of opened file)
                if self.lsp_manager.workspace_root().is_none() {
                    if let Some(parent) = path.parent() {
                        // Try to find a project root (Cargo.toml, package.json, .git, etc.)
                        let workspace_root = find_project_root(parent).unwrap_or_else(|| parent.to_path_buf());
                        self.lsp_manager.set_workspace_root(Some(workspace_root));
                    }
                }

                if let Some(lang) = language_id_from_path(path) {
                    let text = editor.buffer().to_string();
                    let path = path.to_path_buf();
                    self.lsp_manager.did_open(&path, lang, &text);
                }
            }
        }
    }

    /// Notifies LSP that a file was saved.
    pub fn notify_lsp_file_saved(&mut self) {
        if let Some(editor) = self.workspace.active_editor() {
            if let Some(path) = editor.file_path() {
                if let Some(lang) = language_id_from_path(path) {
                    let path = path.to_path_buf();
                    self.lsp_manager.did_save(&path, lang);
                }
            }
        }
    }

    /// Notifies LSP that a file was closed.
    pub fn notify_lsp_file_closed(&mut self, path: &PathBuf) {
        if let Some(lang) = language_id_from_path(path) {
            self.flush_pending_lsp_changes(true);
            self.lsp_manager.did_close(path, lang);
        }
    }

    /// Flushes any buffered didChange to LSP (debounced unless forced).
    pub fn flush_pending_lsp_changes(&mut self, force: bool) {
        if !self.pending_lsp_change {
            return;
        }

        if !force {
            if let Some(last) = self.last_lsp_change {
                if last.elapsed() < self.lsp_change_debounce {
                    return;
                }
            } else {
                return;
            }
        }

        if let Some(editor) = self.workspace.active_editor_mut() {
            if let Some(path) = editor.file_path().map(|p| p.to_path_buf()) {
                if let Some(lang) = language_id_from_path(&path) {
                    let text = editor.buffer().to_string();
                    editor.increment_document_version();
                    let version = editor.document_version();
                    self.lsp_manager.did_change(&path, lang, version, &text);
                }
            }
        }

        self.pending_lsp_change = false;
    }

    /// Requests hover info from LSP at the current cursor position.
    pub fn request_hover(&mut self) {
        if let Some(editor) = self.workspace.active_editor() {
            if let Some(path) = editor.file_path() {
                if let Some(lang) = language_id_from_path(path) {
                    let pos = editor.cursor_position();
                    let path = path.to_path_buf();
                    self.lsp_manager.hover(&path, lang, pos.line, pos.col);
                }
            }
        }
    }

    /// Requests completions from LSP at the current cursor position.
    pub fn request_completions(&mut self) {
        if let Some(editor) = self.workspace.active_editor() {
            if let Some(path) = editor.file_path() {
                if let Some(lang) = language_id_from_path(path) {
                    let pos = editor.cursor_position();
                    let path = path.to_path_buf();
                    self.lsp_manager.completion(&path, lang, pos.line, pos.col);
                }
            }
        }
    }

    /// Requests go to definition from LSP at the current cursor position.
    pub fn request_goto_definition(&mut self) {
        if let Some(editor) = self.workspace.active_editor() {
            if let Some(path) = editor.file_path() {
                if let Some(lang) = language_id_from_path(path) {
                    let pos = editor.cursor_position();
                    let path = path.to_path_buf();
                    self.lsp_manager.goto_definition(&path, lang, pos.line, pos.col);
                }
            }
        }
    }

    /// Updates hover state based on mouse position.
    /// Call this when the mouse moves to potentially trigger a hover request.
    pub fn update_hover(&mut self, screen_x: f32, screen_y: f32, char_width: f32, line_height: f32) {
        // Delay before showing hover (500ms)
        const HOVER_DELAY_MS: u64 = 500;

        let (line, col) = self.screen_to_buffer_position(screen_x, screen_y, char_width, line_height);

        // Check if we moved to a different position
        let should_clear = self.hover_mouse_pos.map(|(prev_x, prev_y)| {
            let (prev_line, prev_col) = self.screen_to_buffer_position(prev_x, prev_y, char_width, line_height);
            prev_line != line || prev_col != col
        }).unwrap_or(true);

        if should_clear {
            // Clear existing hover info if we moved
            if let Some(editor) = self.workspace.active_editor_mut() {
                editor.clear_hover_info();
            }
            self.hover_mouse_pos = Some((screen_x, screen_y));
            self.hover_request_time = Some(Instant::now());
            self.hover_pending = false;
        }

        // Check if we should trigger a hover request
        if !self.hover_pending {
            if let Some(request_time) = self.hover_request_time {
                if request_time.elapsed() >= Duration::from_millis(HOVER_DELAY_MS) {
                    // Send hover request at this position
                    if let Some(editor) = self.workspace.active_editor() {
                        if let Some(path) = editor.file_path() {
                            if let Some(lang) = language_id_from_path(path) {
                                let path = path.to_path_buf();
                                self.lsp_manager.hover(&path, lang, line, col);
                                self.hover_pending = true;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Clears the hover state.
    pub fn clear_hover(&mut self) {
        self.hover_mouse_pos = None;
        self.hover_request_time = None;
        self.hover_pending = false;
        if let Some(editor) = self.workspace.active_editor_mut() {
            editor.clear_hover_info();
        }
    }

    /// Triggers auto-completion at the current cursor position.
    pub fn trigger_completion(&mut self) {
        if let Some(editor) = self.workspace.active_editor() {
            if let Some(path) = editor.file_path() {
                if let Some(lang) = language_id_from_path(path) {
                    let pos = editor.cursor_position();
                    let path = path.to_path_buf();
                    self.completion_trigger_pos = Some((pos.line, pos.col));
                    self.lsp_manager.completion(&path, lang, pos.line, pos.col);
                }
            }
        }
    }

    /// Moves to the next completion item.
    pub fn completion_next(&mut self) {
        if let Some(editor) = self.workspace.active_editor() {
            let count = editor.completions().len();
            if count > 0 {
                self.completion_selected = (self.completion_selected + 1) % count;
            }
        }
    }

    /// Moves to the previous completion item.
    pub fn completion_prev(&mut self) {
        if let Some(editor) = self.workspace.active_editor() {
            let count = editor.completions().len();
            if count > 0 {
                if self.completion_selected == 0 {
                    self.completion_selected = count - 1;
                } else {
                    self.completion_selected -= 1;
                }
            }
        }
    }

    /// Accepts the currently selected completion.
    pub fn accept_completion(&mut self) {
        if !self.completion_visible {
            return;
        }

        let insert_text = if let Some(editor) = self.workspace.active_editor() {
            let completions = editor.completions();
            if self.completion_selected < completions.len() {
                let item = &completions[self.completion_selected];
                Some(item.insert_text.clone().unwrap_or_else(|| item.label.clone()))
            } else {
                None
            }
        } else {
            None
        };

        if let Some(text) = insert_text {
            // Delete from trigger position to current position, then insert
            if let Some((trigger_line, trigger_col)) = self.completion_trigger_pos {
                if let Some(editor) = self.workspace.active_editor_mut() {
                    let pos = editor.cursor_position();
                    // Only insert if we're on the same line
                    if pos.line == trigger_line && pos.col >= trigger_col {
                        // Delete the partial text typed so far
                        for _ in trigger_col..pos.col {
                            editor.delete_backward();
                        }
                        // Insert the completion text
                        editor.insert_text(&text);
                    }
                }
            }
        }

        self.hide_completion();
    }

    /// Hides the completion popup.
    pub fn hide_completion(&mut self) {
        self.completion_visible = false;
        self.completion_selected = 0;
        self.completion_trigger_pos = None;
        if let Some(editor) = self.workspace.active_editor_mut() {
            editor.clear_completions();
        }
    }

    /// Opens the search bar.
    pub fn open_search(&mut self) {
        self.input_mode = InputMode::Search;
        self.focused_field = 0;
        // Pre-fill with selection if any
        if let Some(editor) = self.workspace.active_editor() {
            if let Some(selected) = editor.selected_text() {
                if !selected.contains('\n') {
                    self.search_text = selected;
                }
            }
        }
        // Perform search immediately if there's text
        if !self.search_text.is_empty() {
            if let Some(editor) = self.workspace.active_editor_mut() {
                editor.find(&self.search_text);
            }
        }
    }

    /// Opens the replace bar.
    pub fn open_replace(&mut self) {
        self.input_mode = InputMode::Replace;
        self.focused_field = 0;
        // Pre-fill with selection if any
        if let Some(editor) = self.workspace.active_editor() {
            if let Some(selected) = editor.selected_text() {
                if !selected.contains('\n') {
                    self.search_text = selected;
                }
            }
        }
        // Perform search immediately if there's text
        if !self.search_text.is_empty() {
            if let Some(editor) = self.workspace.active_editor_mut() {
                editor.find(&self.search_text);
            }
        }
    }

    /// Opens the go to line dialog.
    pub fn open_goto_line(&mut self) {
        self.input_mode = InputMode::GoToLine;
        self.goto_text.clear();
    }

    /// Opens the rename symbol dialog.
    pub fn open_rename(&mut self) {
        // Get the word under cursor to pre-fill the rename text
        if let Some(editor) = self.workspace.active_editor() {
            if let Some(word) = editor.word_under_cursor() {
                self.rename_text = word;
            } else {
                self.rename_text.clear();
            }
        }
        self.input_mode = InputMode::Rename;
    }

    /// Requests rename from LSP.
    pub fn request_rename(&mut self, new_name: &str) {
        if let Some(editor) = self.workspace.active_editor() {
            if let Some(path) = editor.file_path() {
                if let Some(lang) = language_id_from_path(path) {
                    let pos = editor.cursor_position();
                    let path = path.to_path_buf();
                    self.lsp_manager.rename(&path, lang, pos.line, pos.col, new_name);
                }
            }
        }
        self.input_mode = InputMode::Normal;
    }

    /// Closes the search/replace/goto bar.
    pub fn close_input_bar(&mut self) {
        if self.input_mode != InputMode::Normal {
            self.input_mode = InputMode::Normal;
            // Clear search highlighting
            if let Some(editor) = self.workspace.active_editor_mut() {
                editor.clear_search();
            }
        } else {
            // If already in normal mode, collapse cursors
            if let Some(editor) = self.workspace.active_editor_mut() {
                editor.collapse_cursors();
                editor.exit_block_selection();
            }
        }
    }

    /// Returns true if in any input mode.
    pub fn is_input_mode(&self) -> bool {
        self.input_mode != InputMode::Normal
    }

    /// Returns the current content area Y offset (accounting for tab bar and search bar).
    pub fn content_y_offset(&self) -> f32 {
        let mut offset = TAB_BAR_HEIGHT;
        if self.input_mode != InputMode::Normal {
            offset += SEARCH_BAR_HEIGHT;
        }
        offset
    }

    /// Opens a file, creating a new tab.
    pub fn open_file(&mut self, path: PathBuf) {
        if let Err(e) = self.workspace.open_file(&path) {
            log::error!("Failed to open file {:?}: {}", path, e);
        }
    }

    /// Resets the cursor blink state (makes cursor visible and restarts timer).
    pub fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.last_cursor_blink = Instant::now();
    }

    /// Updates the cursor blink state. Returns true if a redraw is needed.
    pub fn update_cursor_blink(&mut self) -> bool {
        if !self.cursor_blink_enabled {
            return false;
        }

        let elapsed = self.last_cursor_blink.elapsed();
        if elapsed >= Duration::from_millis(CURSOR_BLINK_INTERVAL_MS) {
            self.cursor_visible = !self.cursor_visible;
            self.last_cursor_blink = Instant::now();
            true
        } else {
            false
        }
    }

    /// Converts screen coordinates to buffer position.
    pub fn screen_to_buffer_position(
        &self,
        x: f32,
        y: f32,
        char_width: f32,
        line_height: f32,
    ) -> (usize, usize) {
        // Adjust y for tab bar and search bar
        let y = y - self.content_y_offset();
        if y < 0.0 {
            return (0, 0);
        }

        if let Some(editor) = self.workspace.active_editor() {
            let scroll_offset = editor.scroll_offset();
            let buffer = editor.buffer();

            // Calculate which line was clicked
            let screen_line = (y / line_height).floor() as usize;
            let buffer_line = scroll_offset + screen_line;
            let buffer_line = buffer_line.min(buffer.len_lines().saturating_sub(1));

            // Calculate which column was clicked
            let horizontal_scroll = editor.horizontal_scroll();
            let text_x = (x - self.line_number_margin).max(0.0);
            let col = (text_x / char_width).round() as usize + horizontal_scroll;

            // Clamp column to line length
            let line_len = buffer.line_len_chars(buffer_line);
            let col = col.min(line_len);

            (buffer_line, col)
        } else {
            (0, 0)
        }
    }

    /// Returns whether click is in tab bar area.
    pub fn is_in_tab_bar(&self, y: f32) -> bool {
        y < TAB_BAR_HEIGHT
    }

    /// Returns whether click is in search bar area.
    pub fn is_in_search_bar(&self, y: f32) -> bool {
        self.input_mode != InputMode::Normal && y >= TAB_BAR_HEIGHT && y < TAB_BAR_HEIGHT + SEARCH_BAR_HEIGHT
    }

    /// Handles a click in the tab bar, returns the tab index if clicked on a tab.
    pub fn handle_tab_bar_click(&self, x: f32, char_width: f32) -> Option<usize> {
        let tabs = self.workspace.tabs();
        let mut current_x = 4.0; // Initial padding

        for (index, tab) in tabs.iter().enumerate() {
            // Calculate tab width based on name length + padding + close button
            let tab_width = (tab.name.len() as f32 + 4.0) * char_width + 24.0;

            if x >= current_x && x < current_x + tab_width {
                return Some(index);
            }

            current_x += tab_width + 4.0; // Tab spacing
        }

        None
    }

    /// Renders the editor to the GPU renderer.
    pub fn render(&self, renderer: &mut GpuRenderer) {
        renderer.clear();

        let line_height = renderer.atlas().line_height;
        let char_width = renderer.atlas().char_width;
        let (viewport_width, viewport_height) = renderer.dimensions();
        let content_y = self.content_y_offset();

        // Draw tab bar background
        renderer.draw_rect(
            0.0,
            0.0,
            viewport_width as f32,
            TAB_BAR_HEIGHT,
            renderer.colors.tab_bar_bg,
        );

        // Draw tabs
        let tabs = self.workspace.tabs();
        let active_index = self.workspace.active_tab_index();
        let mut tab_x = 4.0;

        for (index, tab) in tabs.iter().enumerate() {
            let is_active = Some(index) == active_index;
            let tab_width = (tab.name.len() as f32 + 4.0) * char_width + 24.0;

            // Tab background
            let bg_color = if is_active {
                renderer.colors.tab_active_bg
            } else {
                renderer.colors.tab_inactive_bg
            };
            renderer.draw_rect(tab_x, 2.0, tab_width, TAB_BAR_HEIGHT - 4.0, bg_color);

            // Tab text (with modified indicator)
            let display_name = if tab.is_modified {
                format!("● {}", tab.name)
            } else {
                tab.name.clone()
            };
            let text_color = if is_active {
                renderer.colors.text
            } else {
                renderer.colors.line_number
            };
            renderer.draw_text(&display_name, tab_x + 8.0, 6.0, text_color);

            tab_x += tab_width + 4.0;
        }

        // Draw separator line below tab bar
        renderer.draw_rect(
            0.0,
            TAB_BAR_HEIGHT - 1.0,
            viewport_width as f32,
            1.0,
            renderer.colors.line_number,
        );

        // Draw search/replace/goto bar if active
        if self.input_mode != InputMode::Normal {
            self.render_input_bar(renderer, viewport_width as f32, char_width, line_height);
        }

        // Get active editor for rendering
        let Some(editor) = self.workspace.active_editor() else {
            return;
        };

        // Draw line number background (below tab bar and search bar, above status bar)
        renderer.draw_rect(
            0.0,
            content_y,
            self.line_number_margin,
            viewport_height as f32 - content_y - STATUS_BAR_HEIGHT,
            renderer.colors.line_number_bg,
        );

        let smooth_scroll = editor.smooth_scroll();
        let horizontal_scroll = editor.horizontal_scroll();
        let visible_lines = editor.visible_lines();
        let buffer = editor.buffer();
        let total_lines = buffer.len_lines();

        // Calculate smooth scroll offset
        let scroll_frac = smooth_scroll - smooth_scroll.floor();
        let base_scroll_line = smooth_scroll.floor() as usize;

        // Get cursor positions for selection rendering (multi-cursor support)
        let cursor_pos = editor.cursor_position();
        let all_cursor_positions = editor.all_cursor_positions();
        let all_selection_ranges = editor.all_selection_ranges();
        let block_selection = editor.get_block_selection().copied();

        // Get search matches for visible lines
        let search_matches = editor.search_matches_in_range(base_scroll_line, base_scroll_line + visible_lines);
        let current_match = editor.current_search_match();

        // Draw visible lines
        for screen_line in 0..=visible_lines {
            let buffer_line = base_scroll_line + screen_line;
            if buffer_line >= total_lines {
                break;
            }

            // Apply fractional scroll offset, accounting for tab bar and search bar
            let y = content_y + (screen_line as f32 - scroll_frac) * line_height;

            // Draw line number
            let line_num_str = format!("{:>4}", buffer_line + 1);
            renderer.draw_text(&line_num_str, 4.0, y, renderer.colors.line_number);

            // Draw search match highlights for this line
            let line_start = buffer.line_start(buffer_line);
            let line_end = buffer.line_end(buffer_line);
            for m in &search_matches {
                // Check if match overlaps this line
                if m.start < line_end + 1 && m.end > line_start {
                    let match_start_on_line = if m.start > line_start {
                        m.start - line_start
                    } else {
                        0
                    };
                    let match_end_on_line = if m.end < line_end + 1 {
                        m.end - line_start
                    } else {
                        line_end - line_start + 1
                    };

                    // Apply horizontal scroll offset
                    let visible_match_start = match_start_on_line.saturating_sub(horizontal_scroll);
                    let visible_match_end = match_end_on_line.saturating_sub(horizontal_scroll);

                    if visible_match_end > visible_match_start {
                        let match_x = self.line_number_margin + visible_match_start as f32 * char_width;
                        let match_width = (visible_match_end - visible_match_start) as f32 * char_width;

                        // Use brighter color for current match
                        let color = if Some(*m) == current_match {
                            renderer.colors.search_match_current
                        } else {
                            renderer.colors.search_match
                        };

                        renderer.draw_rect(match_x, y, match_width, line_height, color);
                    }
                }
            }

            // Draw selection backgrounds for this line (all cursors)
            let line_start = buffer.line_start(buffer_line);
            let line_end = buffer.line_end(buffer_line);

            for selection_range in &all_selection_ranges {
                if let Some((sel_start, sel_end)) = selection_range {
                    // Check if selection overlaps this line
                    if *sel_start < line_end + 1 && *sel_end > line_start {
                        let sel_start_on_line = if *sel_start > line_start {
                            *sel_start - line_start
                        } else {
                            0
                        };
                        let sel_end_on_line = if *sel_end < line_end + 1 {
                            *sel_end - line_start
                        } else {
                            line_end - line_start + 1
                        };

                        // Apply horizontal scroll offset to selection
                        let visible_sel_start = sel_start_on_line.saturating_sub(horizontal_scroll);
                        let visible_sel_end = sel_end_on_line.saturating_sub(horizontal_scroll);

                        if visible_sel_end > 0 {
                            let sel_x = self.line_number_margin + visible_sel_start as f32 * char_width;
                            let sel_width = (visible_sel_end - visible_sel_start) as f32 * char_width;

                            renderer.draw_rect(
                                sel_x,
                                y,
                                sel_width.max(char_width * 0.5),
                                line_height,
                                renderer.colors.selection,
                            );
                        }
                    }
                }
            }

            // Draw block selection for this line (if active)
            if let Some(ref block) = block_selection {
                let (top, bottom) = block.bounds();
                if buffer_line >= top.line && buffer_line <= bottom.line {
                    if let Some((start_col, end_col)) = block.col_range(buffer, buffer_line) {
                        // Apply horizontal scroll offset
                        let visible_start = start_col.saturating_sub(horizontal_scroll);
                        let visible_end = end_col.saturating_sub(horizontal_scroll);

                        if visible_end > visible_start {
                            let block_x = self.line_number_margin + visible_start as f32 * char_width;
                            let block_width = (visible_end - visible_start) as f32 * char_width;

                            renderer.draw_rect(
                                block_x,
                                y,
                                block_width.max(char_width * 0.5),
                                line_height,
                                renderer.colors.selection,
                            );
                        }
                    }
                }
            }

            // Draw line text with syntax highlighting
            if let Some(line_text) = buffer.line(buffer_line) {
                let x = self.line_number_margin;
                let char_width = renderer.atlas().char_width;

                // Check if syntax highlighting is available
                if editor.has_syntax_highlighting() {
                    // Draw each character with its highlight color
                    for (i, ch) in line_text.chars().skip(horizontal_scroll).enumerate() {
                        let col = horizontal_scroll + i;
                        let color = editor.highlight_color_at(buffer_line, col);
                        let char_x = x + i as f32 * char_width;
                        renderer.draw_char(ch, char_x, y, color);
                    }
                } else {
                    // No highlighting, draw with default color
                    let visible_text: String = line_text.chars().skip(horizontal_scroll).collect();
                    renderer.draw_text(&visible_text, x, y, renderer.colors.text);
                }
            }

            // Draw diagnostic underlines for this line
            for diagnostic in editor.diagnostics_on_line(buffer_line) {
                // Determine color based on severity
                let color = match diagnostic.severity {
                    DiagnosticSeverity::Error => renderer.colors.diagnostic_error,
                    DiagnosticSeverity::Warning => renderer.colors.diagnostic_warning,
                    DiagnosticSeverity::Information => renderer.colors.diagnostic_info,
                    DiagnosticSeverity::Hint => renderer.colors.diagnostic_hint,
                };

                // Calculate the start and end columns on this line
                let diag_start_col = if diagnostic.start_line == buffer_line {
                    diagnostic.start_col
                } else {
                    0
                };
                let diag_end_col = if diagnostic.end_line == buffer_line {
                    diagnostic.end_col
                } else {
                    buffer.line_len_chars(buffer_line)
                };

                // Adjust for horizontal scroll
                let visible_start = diag_start_col.saturating_sub(horizontal_scroll);
                let visible_end = diag_end_col.saturating_sub(horizontal_scroll);

                if visible_end > visible_start {
                    let underline_x = self.line_number_margin + visible_start as f32 * char_width;
                    let underline_width = (visible_end - visible_start) as f32 * char_width;

                    // Use squiggly underline for errors/warnings, simple underline for info/hints
                    match diagnostic.severity {
                        DiagnosticSeverity::Error | DiagnosticSeverity::Warning => {
                            renderer.draw_squiggle(underline_x, y, underline_width, line_height, color);
                        }
                        _ => {
                            renderer.draw_underline(underline_x, y, underline_width, line_height, color);
                        }
                    }
                }
            }
        }

        // Draw bracket match highlighting
        if let Some((bracket_pos, match_pos)) = editor.matching_bracket_at_cursor() {
            // Helper to draw bracket highlight at a position
            let draw_bracket_highlight = |renderer: &mut GpuRenderer, char_pos: usize| {
                let (line, col) = buffer.char_to_line_col(char_pos);
                if line >= base_scroll_line
                    && line <= base_scroll_line + visible_lines
                    && col >= horizontal_scroll
                {
                    let screen_line = line as f32 - smooth_scroll;
                    let screen_col = col - horizontal_scroll;
                    let x = self.line_number_margin + screen_col as f32 * char_width;
                    let y = content_y + screen_line * line_height;

                    if y >= content_y && y < viewport_height as f32 {
                        renderer.draw_rect(x, y, char_width, line_height, renderer.colors.bracket_match);
                    }
                }
            };

            draw_bracket_highlight(renderer, bracket_pos);
            draw_bracket_highlight(renderer, match_pos);
        }

        // Draw all cursors (multi-cursor support)
        if self.cursor_visible {
            for (cursor_line, cursor_col) in &all_cursor_positions {
                if *cursor_line >= base_scroll_line
                    && *cursor_line <= base_scroll_line + visible_lines
                    && *cursor_col >= horizontal_scroll
                {
                    let cursor_screen_line = *cursor_line as f32 - smooth_scroll;
                    let cursor_screen_col = *cursor_col - horizontal_scroll;
                    let cursor_x = self.line_number_margin + cursor_screen_col as f32 * char_width;
                    let cursor_y = content_y + cursor_screen_line * line_height;

                    // Only draw if cursor is within visible area
                    if cursor_y >= content_y && cursor_y < viewport_height as f32 {
                        renderer.draw_rect(cursor_x, cursor_y, 2.0, line_height, renderer.colors.cursor);
                    }
                }
            }
        }

        // Draw hover popup if we have hover info
        if let Some(hover_info) = editor.hover_info() {
            if let Some((mouse_x, mouse_y)) = self.hover_mouse_pos {
                self.render_hover_popup(renderer, &hover_info.contents, mouse_x, mouse_y, viewport_width as f32, viewport_height as f32, char_width, line_height);
            }
        }

        // Draw completion popup if visible
        if self.completion_visible {
            let completions = editor.completions();
            if !completions.is_empty() {
                // Calculate popup position near the cursor
                let popup_x = self.line_number_margin + (cursor_pos.col - horizontal_scroll) as f32 * char_width;
                let popup_y = content_y + ((cursor_pos.line as f32 - smooth_scroll) + 1.0) * line_height;

                self.render_completion_popup(
                    renderer,
                    completions,
                    self.completion_selected,
                    popup_x,
                    popup_y,
                    viewport_width as f32,
                    viewport_height as f32,
                    char_width,
                    line_height,
                );
            }
        }

        // Draw status bar at the bottom
        self.render_status_bar(renderer, viewport_width as f32, viewport_height as f32, char_width, line_height);

        // Draw notifications in top-right corner
        self.render_notifications(renderer, viewport_width as f32, char_width, line_height);
    }

    /// Renders the hover information popup.
    fn render_hover_popup(
        &self,
        renderer: &mut GpuRenderer,
        content: &str,
        mouse_x: f32,
        mouse_y: f32,
        viewport_width: f32,
        viewport_height: f32,
        char_width: f32,
        line_height: f32,
    ) {
        const PADDING: f32 = 8.0;
        const MAX_WIDTH: f32 = 500.0;
        const MAX_HEIGHT: f32 = 300.0;

        // Calculate popup dimensions based on content
        let lines: Vec<&str> = content.lines().collect();
        let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
        let content_width = (max_line_len as f32 * char_width).min(MAX_WIDTH - 2.0 * PADDING);
        let content_height = (lines.len() as f32 * line_height).min(MAX_HEIGHT - 2.0 * PADDING);

        let popup_width = content_width + 2.0 * PADDING;
        let popup_height = content_height + 2.0 * PADDING;

        // Position popup near the mouse, but keep it on screen
        let mut popup_x = mouse_x + 16.0;
        let mut popup_y = mouse_y + 16.0;

        // Adjust if popup would go off the right edge
        if popup_x + popup_width > viewport_width {
            popup_x = mouse_x - popup_width - 8.0;
        }

        // Adjust if popup would go off the bottom edge
        if popup_y + popup_height > viewport_height {
            popup_y = mouse_y - popup_height - 8.0;
        }

        // Ensure popup stays on screen
        popup_x = popup_x.max(4.0);
        popup_y = popup_y.max(self.content_y_offset() + 4.0);

        // Draw popup background
        renderer.draw_rect(popup_x, popup_y, popup_width, popup_height, renderer.colors.hover_bg);

        // Draw border
        let border_width = 1.0;
        // Top border
        renderer.draw_rect(popup_x, popup_y, popup_width, border_width, renderer.colors.hover_border);
        // Bottom border
        renderer.draw_rect(popup_x, popup_y + popup_height - border_width, popup_width, border_width, renderer.colors.hover_border);
        // Left border
        renderer.draw_rect(popup_x, popup_y, border_width, popup_height, renderer.colors.hover_border);
        // Right border
        renderer.draw_rect(popup_x + popup_width - border_width, popup_y, border_width, popup_height, renderer.colors.hover_border);

        // Draw text content (limited to visible lines)
        let max_visible_lines = ((MAX_HEIGHT - 2.0 * PADDING) / line_height) as usize;
        let text_x = popup_x + PADDING;
        let mut text_y = popup_y + PADDING;

        for line in lines.iter().take(max_visible_lines) {
            // Truncate long lines
            let max_chars = ((MAX_WIDTH - 2.0 * PADDING) / char_width) as usize;
            let display_line: String = line.chars().take(max_chars).collect();
            renderer.draw_text(&display_line, text_x, text_y, renderer.colors.text);
            text_y += line_height;
        }

        // Show "..." if content is truncated
        if lines.len() > max_visible_lines {
            renderer.draw_text("...", text_x, text_y, renderer.colors.line_number);
        }
    }

    /// Renders the completion popup.
    fn render_completion_popup(
        &self,
        renderer: &mut GpuRenderer,
        items: &[CompletionItem],
        selected: usize,
        x: f32,
        y: f32,
        viewport_width: f32,
        viewport_height: f32,
        char_width: f32,
        line_height: f32,
    ) {
        const PADDING: f32 = 4.0;
        const MAX_VISIBLE_ITEMS: usize = 10;
        const ITEM_HEIGHT: f32 = 20.0;

        if items.is_empty() {
            return;
        }

        // Calculate popup dimensions
        let visible_items = items.len().min(MAX_VISIBLE_ITEMS);
        let max_label_len = items.iter().map(|i| i.label.len()).max().unwrap_or(10).max(20);
        let popup_width = (max_label_len as f32 * char_width) + 2.0 * PADDING + 24.0; // Extra space for icon
        let popup_height = visible_items as f32 * ITEM_HEIGHT + 2.0 * PADDING;

        // Position popup - try below cursor first
        let mut popup_x = x;
        let mut popup_y = y;

        // Adjust if popup would go off the right edge
        if popup_x + popup_width > viewport_width {
            popup_x = viewport_width - popup_width - 4.0;
        }

        // Adjust if popup would go off the bottom edge - show above cursor
        if popup_y + popup_height > viewport_height {
            popup_y = y - popup_height - line_height;
        }

        // Ensure popup stays on screen
        popup_x = popup_x.max(4.0);
        popup_y = popup_y.max(self.content_y_offset() + 4.0);

        // Draw popup background
        renderer.draw_rect(popup_x, popup_y, popup_width, popup_height, renderer.colors.completion_bg);

        // Draw border
        let border_width = 1.0;
        renderer.draw_rect(popup_x, popup_y, popup_width, border_width, renderer.colors.completion_border);
        renderer.draw_rect(popup_x, popup_y + popup_height - border_width, popup_width, border_width, renderer.colors.completion_border);
        renderer.draw_rect(popup_x, popup_y, border_width, popup_height, renderer.colors.completion_border);
        renderer.draw_rect(popup_x + popup_width - border_width, popup_y, border_width, popup_height, renderer.colors.completion_border);

        // Calculate scroll offset to keep selected item visible
        let scroll_offset = if selected >= MAX_VISIBLE_ITEMS {
            selected - MAX_VISIBLE_ITEMS + 1
        } else {
            0
        };

        // Draw items
        let text_x = popup_x + PADDING + 20.0; // Leave space for icon
        let mut item_y = popup_y + PADDING;

        for (i, item) in items.iter().skip(scroll_offset).take(visible_items).enumerate() {
            let actual_index = scroll_offset + i;
            let is_selected = actual_index == selected;

            // Draw selection highlight
            if is_selected {
                renderer.draw_rect(
                    popup_x + border_width,
                    item_y,
                    popup_width - 2.0 * border_width,
                    ITEM_HEIGHT,
                    renderer.colors.completion_selected_bg,
                );
            }

            // Draw kind icon (simplified - just first letter of kind)
            let kind_char = item.kind.map(|k| {
                use cp_editor_core::lsp_types::CompletionKind;
                match k {
                    CompletionKind::Method | CompletionKind::Function => 'f',
                    CompletionKind::Variable => 'v',
                    CompletionKind::Field | CompletionKind::Property => 'p',
                    CompletionKind::Class | CompletionKind::Struct => 'S',
                    CompletionKind::Interface => 'I',
                    CompletionKind::Module => 'M',
                    CompletionKind::Keyword => 'k',
                    CompletionKind::Snippet => 's',
                    CompletionKind::Constant => 'c',
                    CompletionKind::Enum | CompletionKind::EnumMember => 'E',
                    CompletionKind::TypeParameter => 'T',
                    _ => '?',
                }
            }).unwrap_or('?');

            let kind_color = renderer.colors.line_number;
            renderer.draw_char(kind_char, popup_x + PADDING + 4.0, item_y + 2.0, kind_color);

            // Draw label
            let label_color = if is_selected {
                renderer.colors.text
            } else {
                [0.8, 0.8, 0.8, 1.0]
            };
            let max_label_chars = ((popup_width - 2.0 * PADDING - 24.0) / char_width) as usize;
            let display_label: String = item.label.chars().take(max_label_chars).collect();
            renderer.draw_text(&display_label, text_x, item_y + 2.0, label_color);

            item_y += ITEM_HEIGHT;
        }

        // Draw scroll indicator if needed
        if items.len() > MAX_VISIBLE_ITEMS {
            let indicator = format!("{}/{}", selected + 1, items.len());
            let indicator_x = popup_x + popup_width - (indicator.len() as f32 * char_width) - PADDING;
            renderer.draw_text(&indicator, indicator_x, popup_y + popup_height - line_height - PADDING, renderer.colors.line_number);
        }
    }

    /// Renders the search/replace/goto input bar.
    fn render_input_bar(&self, renderer: &mut GpuRenderer, viewport_width: f32, char_width: f32, line_height: f32) {
        let bar_y = TAB_BAR_HEIGHT;

        // Draw bar background
        renderer.draw_rect(0.0, bar_y, viewport_width, SEARCH_BAR_HEIGHT, renderer.colors.search_bar_bg);

        // Draw separator line
        renderer.draw_rect(0.0, bar_y + SEARCH_BAR_HEIGHT - 1.0, viewport_width, 1.0, renderer.colors.line_number);

        let padding = 8.0;
        let field_height = 22.0;
        let field_y = bar_y + (SEARCH_BAR_HEIGHT - field_height) / 2.0;
        let text_y = field_y + (field_height - line_height) / 2.0;

        match self.input_mode {
            InputMode::Search => {
                // Draw "Find:" label
                renderer.draw_text("Find:", padding, text_y, renderer.colors.text);
                let label_width = 5.0 * char_width + padding;

                // Draw search input field
                let field_x = label_width + padding;
                let field_width = 200.0;
                self.draw_input_field(renderer, field_x, field_y, field_width, field_height, &self.search_text, self.focused_field == 0, char_width, line_height);

                // Draw status
                if let Some(editor) = self.workspace.active_editor() {
                    if let Some(status) = editor.search_status() {
                        let status_x = field_x + field_width + padding;
                        renderer.draw_text(&status, status_x, text_y, renderer.colors.line_number);
                    }
                }
            }
            InputMode::Replace => {
                // Draw "Find:" label and field
                renderer.draw_text("Find:", padding, text_y, renderer.colors.text);
                let label_width = 5.0 * char_width + padding;
                let field_x = label_width + padding;
                let field_width = 150.0;
                self.draw_input_field(renderer, field_x, field_y, field_width, field_height, &self.search_text, self.focused_field == 0, char_width, line_height);

                // Draw "Replace:" label and field
                let replace_label_x = field_x + field_width + padding * 2.0;
                renderer.draw_text("Replace:", replace_label_x, text_y, renderer.colors.text);
                let replace_field_x = replace_label_x + 8.0 * char_width + padding;
                self.draw_input_field(renderer, replace_field_x, field_y, field_width, field_height, &self.replace_text, self.focused_field == 1, char_width, line_height);

                // Draw status
                if let Some(editor) = self.workspace.active_editor() {
                    if let Some(status) = editor.search_status() {
                        let status_x = replace_field_x + field_width + padding;
                        renderer.draw_text(&status, status_x, text_y, renderer.colors.line_number);
                    }
                }
            }
            InputMode::GoToLine => {
                // Draw "Go to line:" label
                renderer.draw_text("Go to line:", padding, text_y, renderer.colors.text);
                let label_width = 11.0 * char_width + padding;

                // Draw input field
                let field_x = label_width + padding;
                let field_width = 80.0;
                self.draw_input_field(renderer, field_x, field_y, field_width, field_height, &self.goto_text, true, char_width, line_height);

                // Draw line count info
                if let Some(editor) = self.workspace.active_editor() {
                    let total_lines = editor.buffer().len_lines();
                    let info = format!("of {}", total_lines);
                    let info_x = field_x + field_width + padding;
                    renderer.draw_text(&info, info_x, text_y, renderer.colors.line_number);
                }
            }
            InputMode::Rename => {
                // Draw "Rename:" label
                renderer.draw_text("Rename to:", padding, text_y, renderer.colors.text);
                let label_width = 10.0 * char_width + padding;

                // Draw input field
                let field_x = label_width + padding;
                let field_width = 200.0;
                self.draw_input_field(renderer, field_x, field_y, field_width, field_height, &self.rename_text, true, char_width, line_height);

                // Draw hint
                let hint = "(Enter to confirm, Esc to cancel)";
                let hint_x = field_x + field_width + padding;
                renderer.draw_text(hint, hint_x, text_y, renderer.colors.line_number);
            }
            InputMode::Normal => {}
        }
    }

    /// Draws an input field.
    fn draw_input_field(
        &self,
        renderer: &mut GpuRenderer,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        text: &str,
        focused: bool,
        char_width: f32,
        line_height: f32,
    ) {
        // Draw field background
        renderer.draw_rect(x, y, width, height, renderer.colors.input_field_bg);

        // Draw border (brighter if focused)
        let border_color = if focused {
            renderer.colors.text
        } else {
            renderer.colors.input_field_border
        };
        // Top border
        renderer.draw_rect(x, y, width, 1.0, border_color);
        // Bottom border
        renderer.draw_rect(x, y + height - 1.0, width, 1.0, border_color);
        // Left border
        renderer.draw_rect(x, y, 1.0, height, border_color);
        // Right border
        renderer.draw_rect(x + width - 1.0, y, 1.0, height, border_color);

        // Draw text
        let text_x = x + 4.0;
        let text_y = y + (height - line_height) / 2.0;
        let max_chars = ((width - 8.0) / char_width) as usize;
        let display_text: String = text.chars().take(max_chars).collect();
        renderer.draw_text(&display_text, text_x, text_y, renderer.colors.text);

        // Draw cursor if focused
        if focused && self.cursor_visible {
            let cursor_x = text_x + display_text.len() as f32 * char_width;
            renderer.draw_rect(cursor_x, text_y, 2.0, line_height, renderer.colors.cursor);
        }
    }

    /// Renders the status bar at the bottom of the window.
    fn render_status_bar(
        &self,
        renderer: &mut GpuRenderer,
        viewport_width: f32,
        viewport_height: f32,
        char_width: f32,
        line_height: f32,
    ) {
        let bar_y = viewport_height - STATUS_BAR_HEIGHT;
        let padding = 8.0;
        let text_y = bar_y + (STATUS_BAR_HEIGHT - line_height) / 2.0;

        // Draw status bar background
        renderer.draw_rect(0.0, bar_y, viewport_width, STATUS_BAR_HEIGHT, renderer.colors.tab_bar_bg);

        // Draw separator line above status bar
        renderer.draw_rect(0.0, bar_y, viewport_width, 1.0, renderer.colors.line_number);

        // Get editor info
        if let Some(editor) = self.workspace.active_editor() {
            // Left side: File info and language
            let mut left_x = padding;

            // Language indicator
            let lang_name = editor.language().name();
            renderer.draw_text(lang_name, left_x, text_y, renderer.colors.line_number);
            left_x += (lang_name.len() as f32 + 2.0) * char_width;

            // Encoding (always UTF-8 for now)
            renderer.draw_text("UTF-8", left_x, text_y, renderer.colors.line_number);

            // Right side: Cursor position
            let cursor = editor.cursor_position();
            let pos_text = format!("Ln {}, Col {}", cursor.line + 1, cursor.col + 1);
            let pos_x = viewport_width - padding - pos_text.len() as f32 * char_width;
            renderer.draw_text(&pos_text, pos_x, text_y, renderer.colors.text);

            // Modified indicator (if modified)
            if editor.is_modified() {
                let mod_text = "Modified";
                let mod_x = pos_x - (mod_text.len() as f32 + 3.0) * char_width;
                renderer.draw_text(mod_text, mod_x, text_y, [0.9, 0.7, 0.3, 1.0]);
            }
        }
    }

    /// Renders notifications in the top-right corner.
    fn render_notifications(&self, renderer: &mut GpuRenderer, viewport_width: f32, char_width: f32, line_height: f32) {
        const NOTIFICATION_WIDTH: f32 = 300.0;
        const NOTIFICATION_HEIGHT: f32 = 40.0;
        const NOTIFICATION_MARGIN: f32 = 8.0;
        const NOTIFICATION_PADDING: f32 = 12.0;

        let start_y = TAB_BAR_HEIGHT + NOTIFICATION_MARGIN;
        let mut y = start_y;

        for notification in self.notifications.visible() {
            let visibility = notification.visibility();
            if visibility <= 0.0 {
                continue;
            }

            let x = viewport_width - NOTIFICATION_WIDTH - NOTIFICATION_MARGIN;

            // Get colors with alpha based on visibility
            let mut bg_color = notification.notification_type.color();
            bg_color[3] *= visibility;
            let mut text_color = notification.notification_type.text_color();
            text_color[3] *= visibility;

            // Draw background
            renderer.draw_rect(x, y, NOTIFICATION_WIDTH, NOTIFICATION_HEIGHT, bg_color);

            // Draw border
            let border_color = [0.0, 0.0, 0.0, 0.3 * visibility];
            renderer.draw_rect(x, y, NOTIFICATION_WIDTH, 1.0, border_color);
            renderer.draw_rect(x, y + NOTIFICATION_HEIGHT - 1.0, NOTIFICATION_WIDTH, 1.0, border_color);
            renderer.draw_rect(x, y, 1.0, NOTIFICATION_HEIGHT, border_color);
            renderer.draw_rect(x + NOTIFICATION_WIDTH - 1.0, y, 1.0, NOTIFICATION_HEIGHT, border_color);

            // Draw text (truncate if too long)
            let text_x = x + NOTIFICATION_PADDING;
            let text_y = y + (NOTIFICATION_HEIGHT - line_height) / 2.0;
            let max_chars = ((NOTIFICATION_WIDTH - 2.0 * NOTIFICATION_PADDING) / char_width) as usize;
            let display_text: String = notification.message.chars().take(max_chars).collect();
            renderer.draw_text(&display_text, text_x, text_y, text_color);

            y += NOTIFICATION_HEIGHT + NOTIFICATION_MARGIN;
        }
    }

    /// Updates the window title based on current buffer.
    pub fn window_title(&self) -> String {
        if let Some(editor) = self.workspace.active_editor() {
            let name = editor
                .file_path()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("Untitled");
            let modified = if editor.is_modified() { " ●" } else { "" };
            format!("{}{} - CP Editor", name, modified)
        } else {
            "CP Editor".to_string()
        }
    }
}

/// GPU state for rendering.
struct GpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    renderer: GpuRenderer,
}

impl GpuState {
    fn new(window: Arc<Window>, font_size: f32) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window).unwrap();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("Failed to find an appropriate adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            },
            None,
        ))
        .expect("Failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let renderer = GpuRenderer::new(
            &device,
            &queue,
            surface_format,
            size.width.max(1),
            size.height.max(1),
            font_size,
        );

        Self {
            surface,
            device,
            queue,
            config,
            size,
            renderer,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.renderer
                .resize(&self.queue, new_size.width, new_size.height);
        }
    }

    fn render(&mut self, app: &EditorApp) {
        // Build draw commands
        app.render(&mut self.renderer);

        // Get surface texture
        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(e) => {
                log::error!("Surface error: {:?}", e);
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Render to GPU
        self.renderer.render(&self.device, &self.queue, &view);

        output.present();
    }

    fn line_height(&self) -> f32 {
        self.renderer.atlas().line_height
    }

    fn char_width(&self) -> f32 {
        self.renderer.atlas().char_width
    }
}

/// Application state wrapper for winit 0.30.
struct AppState {
    app: EditorApp,
    gpu: Option<GpuState>,
    window: Option<Arc<Window>>,
    modifiers: ModifiersState,
    /// Current mouse position.
    mouse_position: PhysicalPosition<f64>,
    /// Whether the left mouse button is pressed (for drag selection).
    mouse_dragging: bool,
}

impl AppState {
    fn new(app: EditorApp) -> Self {
        Self {
            app,
            gpu: None,
            window: None,
            modifiers: ModifiersState::empty(),
            mouse_position: PhysicalPosition::new(0.0, 0.0),
            mouse_dragging: false,
        }
    }

    fn handle_mouse_click(&mut self, extend_selection: bool) {
        if let Some(gpu) = &self.gpu {
            // Check if click is in tab bar
            if self.app.is_in_tab_bar(self.mouse_position.y as f32) {
                if let Some(tab_index) = self
                    .app
                    .handle_tab_bar_click(self.mouse_position.x as f32, gpu.char_width())
                {
                    self.app.flush_pending_lsp_changes(true);
                    self.app.workspace.switch_to_tab(tab_index);
                    self.update_window_title();
                }
                return;
            }

            let (line, col) = self.app.screen_to_buffer_position(
                self.mouse_position.x as f32,
                self.mouse_position.y as f32,
                gpu.char_width(),
                gpu.line_height(),
            );
            if let Some(editor) = self.app.workspace.active_editor_mut() {
                editor.set_cursor_position(line, col, extend_selection);
            }
            self.app.reset_cursor_blink();
        }
    }

    fn handle_mouse_drag(&mut self) {
        // Don't drag in tab bar or search bar
        if self.app.is_in_tab_bar(self.mouse_position.y as f32)
            || self.app.is_in_search_bar(self.mouse_position.y as f32) {
            return;
        }

        if let Some(gpu) = &self.gpu {
            let (line, col) = self.app.screen_to_buffer_position(
                self.mouse_position.x as f32,
                self.mouse_position.y as f32,
                gpu.char_width(),
                gpu.line_height(),
            );
            if let Some(editor) = self.app.workspace.active_editor_mut() {
                editor.set_cursor_position(line, col, true);
            }
        }
    }

    /// Handles keyboard input when in input mode (search/replace/goto).
    /// Returns true if the key was handled.
    fn handle_input_mode_key(&mut self, key: &Key, _event_loop: &ActiveEventLoop) -> bool {
        match key {
            Key::Named(NamedKey::Backspace) => {
                match self.app.input_mode {
                    InputMode::Search | InputMode::Replace if self.app.focused_field == 0 => {
                        self.app.search_text.pop();
                        // Update search incrementally
                        if let Some(editor) = self.app.workspace.active_editor_mut() {
                            editor.find(&self.app.search_text);
                        }
                    }
                    InputMode::Replace if self.app.focused_field == 1 => {
                        self.app.replace_text.pop();
                    }
                    InputMode::GoToLine => {
                        self.app.goto_text.pop();
                    }
                    InputMode::Rename => {
                        self.app.rename_text.pop();
                    }
                    _ => {}
                }
                true
            }
            Key::Named(NamedKey::Enter) => {
                match self.app.input_mode {
                    InputMode::Search => {
                        // Find next on Enter
                        if let Some(editor) = self.app.workspace.active_editor_mut() {
                            editor.find_next();
                        }
                    }
                    InputMode::Replace => {
                        if self.app.focused_field == 0 {
                            // Move to replace field
                            self.app.focused_field = 1;
                        } else {
                            // Perform replacement
                            if self.modifiers.shift_key() {
                                // Replace all with Shift+Enter
                                if let Some(editor) = self.app.workspace.active_editor_mut() {
                                    let count = editor.replace_all(&self.app.replace_text);
                                    log::info!("Replaced {} occurrences", count);
                                    if count > 0 {
                                        self.app.notifications.success(format!("Replaced {} occurrence{}", count, if count == 1 { "" } else { "s" }));
                                    } else {
                                        self.app.notifications.info("No matches to replace");
                                    }
                                }
                                self.app.notify_lsp_document_change();
                                self.update_window_title();
                            } else {
                                // Replace current
                                if let Some(editor) = self.app.workspace.active_editor_mut() {
                                    if editor.replace_current(&self.app.replace_text) {
                                        self.app.notifications.info("Replaced match");
                                    }
                                }
                                self.app.notify_lsp_document_change();
                                self.update_window_title();
                            }
                        }
                    }
                    InputMode::GoToLine => {
                        // Go to the specified line
                        if let Ok(line_num) = self.app.goto_text.parse::<usize>() {
                            if let Some(editor) = self.app.workspace.active_editor_mut() {
                                editor.go_to_line(line_num);
                            }
                            self.app.close_input_bar();
                        }
                    }
                    InputMode::Rename => {
                        // Request rename with the new name
                        if !self.app.rename_text.is_empty() {
                            let new_name = self.app.rename_text.clone();
                            self.app.request_rename(&new_name);
                        } else {
                            self.app.close_input_bar();
                        }
                    }
                    _ => {}
                }
                true
            }
            Key::Named(NamedKey::Tab) => {
                // Switch between search and replace fields
                if self.app.input_mode == InputMode::Replace {
                    self.app.focused_field = if self.app.focused_field == 0 { 1 } else { 0 };
                }
                true
            }
            Key::Character(ch) => {
                if !self.modifiers.control_key() && !self.modifiers.alt_key() {
                    if let Some(c) = ch.chars().next() {
                        match self.app.input_mode {
                            InputMode::Search | InputMode::Replace if self.app.focused_field == 0 => {
                                self.app.search_text.push(c);
                                // Update search incrementally
                                if let Some(editor) = self.app.workspace.active_editor_mut() {
                                    editor.find(&self.app.search_text);
                                }
                            }
                            InputMode::Replace if self.app.focused_field == 1 => {
                                self.app.replace_text.push(c);
                            }
                            InputMode::GoToLine => {
                                // Only allow digits
                                if c.is_ascii_digit() {
                                    self.app.goto_text.push(c);
                                }
                            }
                            InputMode::Rename => {
                                // Allow valid identifier characters
                                if c.is_alphanumeric() || c == '_' {
                                    self.app.rename_text.push(c);
                                }
                            }
                            _ => {}
                        }
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn execute_command(&mut self, command: EditorCommand, _event_loop: &ActiveEventLoop) -> bool {
        match command {
            EditorCommand::Save => {
                self.app.flush_pending_lsp_changes(true);
                if let Err(e) = self.app.workspace.save_active() {
                    if e.kind() == std::io::ErrorKind::Other {
                        // No file path - trigger Save As
                        self.show_save_as_dialog();
                    } else {
                        log::error!("Failed to save: {}", e);
                        self.app.notifications.error(format!("Failed to save: {}", e));
                    }
                } else {
                    // Notify LSP about the saved file
                    self.app.notify_lsp_file_saved();
                    // Get file name for notification
                    let filename = self.app.workspace.active_editor()
                        .and_then(|e| e.file_path())
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("File");
                    self.app.notifications.success(format!("Saved: {}", filename));
                }
                self.update_window_title();
                false
            }
            EditorCommand::SaveAs => {
                self.app.flush_pending_lsp_changes(true);
                self.show_save_as_dialog();
                false
            }
            EditorCommand::OpenFile => {
                self.show_open_file_dialog();
                false
            }
            EditorCommand::NewFile => {
                self.app.workspace.new_buffer();
                self.update_window_title();
                false
            }
            EditorCommand::CloseTab => {
                self.close_active_tab();
                false
            }
            EditorCommand::Quit => {
                if self.app.workspace.has_unsaved_changes() {
                    // Show confirmation dialog
                    let result = rfd::MessageDialog::new()
                        .set_title("Unsaved Changes")
                        .set_description("You have unsaved changes. Are you sure you want to quit?")
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show();

                    if result != rfd::MessageDialogResult::Yes {
                        return false; // User cancelled, don't quit
                    }
                }
                self.shutdown_lsp();
                true
            }
            EditorCommand::NextTab => {
                self.app.flush_pending_lsp_changes(true);
                self.app.workspace.next_tab();
                self.update_window_title();
                false
            }
            EditorCommand::PrevTab => {
                self.app.flush_pending_lsp_changes(true);
                self.app.workspace.prev_tab();
                self.update_window_title();
                false
            }
            EditorCommand::SwitchToTab(index) => {
                self.app.flush_pending_lsp_changes(true);
                self.app.workspace.switch_to_tab(index);
                self.update_window_title();
                false
            }
            EditorCommand::InsertChar(ch) => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    // Use auto-bracket for opening brackets
                    if matches!(ch, '(' | '[' | '{') {
                        editor.insert_char_with_auto_bracket(ch);
                    } else {
                        editor.insert_char(ch);
                    }
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::InsertNewline => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.insert_newline();
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::DeleteBackward => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.delete_backward();
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::DeleteForward => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.delete_forward();
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::MoveLeft => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_left(false);
                }
                false
            }
            EditorCommand::MoveRight => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_right(false);
                }
                false
            }
            EditorCommand::MoveUp => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_up(false);
                }
                false
            }
            EditorCommand::MoveDown => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_down(false);
                }
                false
            }
            EditorCommand::MoveWordLeft => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_word_left(false);
                }
                false
            }
            EditorCommand::MoveWordRight => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_word_right(false);
                }
                false
            }
            EditorCommand::MoveToLineStart => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_to_line_start(false);
                }
                false
            }
            EditorCommand::MoveToLineStartSmart => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_to_line_start_smart(false);
                }
                false
            }
            EditorCommand::MoveToLineEnd => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_to_line_end(false);
                }
                false
            }
            EditorCommand::MovePageUp => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_page_up(false);
                }
                false
            }
            EditorCommand::MovePageDown => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_page_down(false);
                }
                false
            }
            EditorCommand::MoveToBufferStart => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_to_buffer_start(false);
                }
                false
            }
            EditorCommand::MoveToBufferEnd => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_to_buffer_end(false);
                }
                false
            }
            EditorCommand::SelectLeft => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_left(true);
                }
                false
            }
            EditorCommand::SelectRight => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_right(true);
                }
                false
            }
            EditorCommand::SelectUp => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_up(true);
                }
                false
            }
            EditorCommand::SelectDown => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_down(true);
                }
                false
            }
            EditorCommand::SelectWordLeft => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_word_left(true);
                }
                false
            }
            EditorCommand::SelectWordRight => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_word_right(true);
                }
                false
            }
            EditorCommand::SelectToLineStart => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_to_line_start(true);
                }
                false
            }
            EditorCommand::SelectToLineStartSmart => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_to_line_start_smart(true);
                }
                false
            }
            EditorCommand::SelectToLineEnd => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_to_line_end(true);
                }
                false
            }
            EditorCommand::SelectPageUp => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_page_up(true);
                }
                false
            }
            EditorCommand::SelectPageDown => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_page_down(true);
                }
                false
            }
            EditorCommand::SelectToBufferStart => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_to_buffer_start(true);
                }
                false
            }
            EditorCommand::SelectToBufferEnd => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_to_buffer_end(true);
                }
                false
            }
            EditorCommand::SelectAll => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.select_all();
                }
                false
            }
            EditorCommand::DuplicateLine => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.duplicate_line();
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::MoveLineUp => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_line_up();
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::MoveLineDown => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_line_down();
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::ToggleBlockSelection => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.toggle_block_selection();
                }
                false
            }
            EditorCommand::AddCursorAbove => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.add_cursor_above();
                }
                false
            }
            EditorCommand::AddCursorBelow => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.add_cursor_below();
                }
                false
            }
            EditorCommand::CollapseCursors => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.collapse_cursors();
                    // Also exit block selection mode
                    editor.exit_block_selection();
                }
                false
            }
            EditorCommand::Undo => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.undo();
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::Redo => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.redo();
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::Copy => {
                if let Some(editor) = self.app.workspace.active_editor() {
                    if let Some(text) = editor.get_selected_text() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            if clipboard.set_text(&text).is_err() {
                                self.app.notifications.error("Failed to copy to clipboard");
                            }
                        }
                    }
                }
                false
            }
            EditorCommand::Cut => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    if let Some(text) = editor.cut_selection() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            if clipboard.set_text(&text).is_err() {
                                self.app.notifications.error("Failed to copy to clipboard");
                            }
                        }
                    }
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::Paste => {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        if let Some(editor) = self.app.workspace.active_editor_mut() {
                            editor.paste(&text);
                        }
                        self.app.notify_lsp_document_change();
                        self.update_window_title();
                    }
                }
                false
            }
            EditorCommand::ToggleComment => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.toggle_comment();
                }
                self.app.notify_lsp_document_change();
                self.update_window_title();
                false
            }
            EditorCommand::ToggleWordWrap => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.toggle_word_wrap();
                    let state = if editor.word_wrap() { "enabled" } else { "disabled" };
                    self.app.notifications.info(format!("Word wrap {}", state));
                }
                false
            }
            EditorCommand::ToggleFold => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    // Detect folds if not already done
                    if editor.fold_manager().regions().is_empty() {
                        editor.detect_folds();
                    }
                    if editor.toggle_fold_at_cursor() {
                        let (line, _) = editor.buffer().char_to_line_col(editor.cursor_char_index());
                        let state = if editor.is_line_folded(line) { "folded" } else { "unfolded" };
                        self.app.notifications.info(format!("Code region {}", state));
                    }
                }
                false
            }
            EditorCommand::FoldAll => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.detect_folds();
                    editor.fold_all();
                    self.app.notifications.info("All regions folded");
                }
                false
            }
            EditorCommand::UnfoldAll => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.unfold_all();
                    self.app.notifications.info("All regions unfolded");
                }
                false
            }
            EditorCommand::ScrollUp(lines) => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    let current = editor.scroll_offset();
                    editor.set_scroll_offset(current.saturating_sub(lines as usize));
                }
                false
            }
            EditorCommand::ScrollDown(lines) => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    let current = editor.scroll_offset();
                    editor.set_scroll_offset(current + lines as usize);
                }
                false
            }
            EditorCommand::OpenSearch => {
                self.app.open_search();
                false
            }
            EditorCommand::OpenReplace => {
                self.app.open_replace();
                false
            }
            EditorCommand::FindNext => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.find_next();
                }
                false
            }
            EditorCommand::FindPrev => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.find_prev();
                }
                false
            }
            EditorCommand::CloseSearch => {
                self.app.close_input_bar();
                false
            }
            EditorCommand::GoToLine => {
                self.app.open_goto_line();
                false
            }
            EditorCommand::GotoDefinition => {
                self.app.request_goto_definition();
                false
            }
            EditorCommand::TriggerCompletion => {
                self.app.trigger_completion();
                false
            }
            EditorCommand::RenameSymbol => {
                self.app.open_rename();
                false
            }
        }
    }

    fn show_open_file_dialog(&mut self) {
        if self.app.dialog_open {
            return;
        }
        self.app.dialog_open = true;

        let dialog = rfd::FileDialog::new()
            .set_title("Open File")
            .pick_file();

        self.app.dialog_open = false;

        match dialog {
            Some(path) => {
                if let Err(e) = self.app.workspace.open_file(&path) {
                    log::error!("Failed to open file: {}", e);
                } else {
                    // Notify LSP about the newly opened file
                    self.app.notify_lsp_file_opened();
                }
                self.update_window_title();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            None => {
                log::info!("Open file dialog cancelled or unavailable (try: apt install zenity)");
            }
        }
    }

    fn show_save_as_dialog(&mut self) {
        if self.app.dialog_open {
            return;
        }
        self.app.dialog_open = true;

        let dialog = rfd::FileDialog::new()
            .set_title("Save As")
            .save_file();

        self.app.dialog_open = false;

        match dialog {
            Some(path) => {
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("File")
                    .to_string();
                if let Err(e) = self.app.workspace.save_active_as(&path) {
                    log::error!("Failed to save file: {}", e);
                    self.app.notifications.error(format!("Failed to save: {}", e));
                } else {
                    // Notify LSP about the saved file (and open it if new)
                    self.app.notify_lsp_file_opened();
                    self.app.notify_lsp_file_saved();
                    self.app.notifications.success(format!("Saved: {}", filename));
                }
                self.update_window_title();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            None => {
                log::info!("Save dialog cancelled or unavailable (try: apt install zenity)");
            }
        }
    }

    fn close_active_tab(&mut self) {
        if let Some(editor) = self.app.workspace.active_editor() {
            if editor.is_modified() {
                let file_name = editor
                    .file_path()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("Untitled");

                // Show confirmation dialog with Save/Don't Save/Cancel options
                let result = rfd::MessageDialog::new()
                    .set_title("Unsaved Changes")
                    .set_description(&format!(
                        "Do you want to save the changes to \"{}\"?",
                        file_name
                    ))
                    .set_buttons(rfd::MessageButtons::YesNoCancel)
                    .show();

                match result {
                    rfd::MessageDialogResult::Yes => {
                        // Save before closing
                        if let Err(e) = self.app.workspace.save_active() {
                            if e.kind() == std::io::ErrorKind::Other {
                                // No file path - trigger Save As
                                self.show_save_as_dialog();
                                return; // Don't close yet - SaveAs will handle it
                            } else {
                                log::error!("Failed to save: {}", e);
                                return; // Save failed, don't close
                            }
                        }
                        self.app.notify_lsp_file_saved();
                    }
                    rfd::MessageDialogResult::No => {
                        // Don't save, proceed with closing
                    }
                    _ => {
                        // Cancel - don't close
                        return;
                    }
                }
            }
        }

        // Notify LSP about the document being closed before dropping it
        if let Some(path) = self
            .app
            .workspace
            .active_editor()
            .and_then(|e| e.file_path().map(|p| p.to_path_buf()))
        {
            self.app.flush_pending_lsp_changes(true);
            self.app.notify_lsp_file_closed(&path);
        }

        self.app.workspace.close_active_buffer();

        // If no buffers left, create a new one
        if self.app.workspace.tab_count() == 0 {
            self.app.workspace.new_buffer();
        }

        self.update_window_title();
    }

    /// Flushes pending LSP changes and closes all open LSP documents.
    fn shutdown_lsp(&mut self) {
        self.app.flush_pending_lsp_changes(true);
        let open_paths: Vec<PathBuf> = self
            .app
            .workspace
            .editors()
            .filter_map(|(_, editor)| editor.file_path().map(|p| p.to_path_buf()))
            .collect();

        for path in open_paths {
            self.app.notify_lsp_file_closed(&path);
        }
        self.app.lsp_manager.shutdown_all();
    }

    fn update_window_title(&self) {
        if let Some(window) = &self.window {
            window.set_title(&self.app.window_title());
        }
    }

    fn update_visible_dimensions(&mut self) {
        if let Some(gpu) = &self.gpu {
            if let Some(window) = &self.window {
                let size = window.inner_size();
                // Account for tab bar, search bar (if active), and status bar
                let mut content_height = size.height as f32 - TAB_BAR_HEIGHT - STATUS_BAR_HEIGHT;
                if self.app.input_mode != InputMode::Normal {
                    content_height -= SEARCH_BAR_HEIGHT;
                }
                let visible_lines = (content_height / gpu.line_height()) as usize;
                let visible_cols =
                    ((size.width as f32 - self.app.line_number_margin) / gpu.char_width()) as usize;

                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.set_visible_lines(visible_lines.max(1));
                    editor.set_visible_cols(visible_cols.max(1));
                    // Update wrap width to match visible columns
                    editor.set_wrap_width(visible_cols.max(10));
                }
            }
        }
    }
}

impl ApplicationHandler for AppState {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title(&self.app.window_title())
                .with_inner_size(PhysicalSize::new(1280u32, 720u32));

            let window = Arc::new(
                event_loop
                    .create_window(window_attributes)
                    .expect("Failed to create window"),
            );

            let gpu = GpuState::new(window.clone(), self.app.font_size);

            self.window = Some(window.clone());
            self.gpu = Some(gpu);

            self.update_visible_dimensions();

            // Set up continuous redraw for cursor blinking
            event_loop.set_control_flow(ControlFlow::Poll);

            // Request initial redraw
            window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if self.app.workspace.has_unsaved_changes() {
                    // Show confirmation dialog
                    let result = rfd::MessageDialog::new()
                        .set_title("Unsaved Changes")
                        .set_description("You have unsaved changes. Are you sure you want to quit?")
                        .set_buttons(rfd::MessageButtons::YesNo)
                        .show();

                    if result != rfd::MessageDialogResult::Yes {
                        return; // User cancelled, don't quit
                    }
                }
                self.shutdown_lsp();
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    if let Some(gpu) = &mut self.gpu {
                        gpu.resize(new_size);
                    }
                    self.update_visible_dimensions();
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
                self.app
                    .input_handler
                    .update_modifiers_state(self.modifiers);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state,
                        logical_key,
                        repeat,
                        ..
                    },
                ..
            } => {
                if state == ElementState::Pressed {
                    // Handle completion navigation first
                    if self.app.completion_visible {
                        match &logical_key {
                            Key::Named(NamedKey::ArrowDown) => {
                                self.app.completion_next();
                                self.app.reset_cursor_blink();
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                                return;
                            }
                            Key::Named(NamedKey::ArrowUp) => {
                                self.app.completion_prev();
                                self.app.reset_cursor_blink();
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                                return;
                            }
                            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Tab) => {
                                self.app.accept_completion();
                                self.app.notify_lsp_document_change();
                                self.update_window_title();
                                self.app.reset_cursor_blink();
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                                return;
                            }
                            Key::Named(NamedKey::Escape) => {
                                self.app.hide_completion();
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                                return;
                            }
                            _ => {
                                // Any other key hides completion
                                self.app.hide_completion();
                            }
                        }
                    }

                    // Handle input mode (search/replace/goto) first
                    if self.app.is_input_mode() {
                        let handled = self.handle_input_mode_key(&logical_key, event_loop);
                        if handled {
                            self.app.reset_cursor_blink();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        } else {
                            // Check for commands that should work in input mode (Escape, F3)
                            if let Some(command) = self
                                .app
                                .input_handler
                                .handle_key_event_new(&logical_key, state)
                            {
                                match command {
                                    EditorCommand::CloseSearch
                                    | EditorCommand::FindNext
                                    | EditorCommand::FindPrev => {
                                        if self.execute_command(command, event_loop) {
                                            event_loop.exit();
                                        }
                                        self.app.reset_cursor_blink();
                                        if let Some(window) = &self.window {
                                            window.request_redraw();
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    } else {
                        // Normal mode - regular command handling
                        if let Some(command) = self
                            .app
                            .input_handler
                            .handle_key_event_new(&logical_key, state)
                        {
                            if self.execute_command(command, event_loop) {
                                event_loop.exit();
                            }
                            self.app.reset_cursor_blink();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }

                        // Handle character input for text
                        if let Key::Character(ch) = &logical_key {
                            if !self.modifiers.control_key() && !self.modifiers.alt_key() {
                                if let Some(c) = ch.chars().next() {
                                    if let Some(command) = self.app.input_handler.handle_char_input(c) {
                                        self.execute_command(command, event_loop);
                                        self.app.reset_cursor_blink();
                                        if let Some(window) = &self.window {
                                            window.request_redraw();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if repeat {
                    log::trace!("Key repeat: {:?}", logical_key);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(command) = self.app.input_handler.handle_scroll(delta) {
                    self.execute_command(command, event_loop);
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::Ime(ime_event) => {
                use winit::event::Ime;
                match ime_event {
                    Ime::Enabled => {
                        log::debug!("IME enabled");
                    }
                    Ime::Preedit(text, cursor) => {
                        if text.is_empty() {
                            self.app.input_handler.ime.cancel_composition();
                        } else {
                            if !self.app.input_handler.ime.composing {
                                self.app.input_handler.ime.start_composition();
                            }
                            let cursor_pos = cursor.map(|(start, _)| start).unwrap_or(text.len());
                            self.app
                                .input_handler
                                .ime
                                .update_composition(&text, cursor_pos);
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    Ime::Commit(text) => {
                        self.app.input_handler.ime.end_composition();
                        if let Some(editor) = self.app.workspace.active_editor_mut() {
                            editor.insert_text(&text);
                        }
                        self.app.notify_lsp_document_change();
                        self.app.reset_cursor_blink();
                        self.update_window_title();
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    Ime::Disabled => {
                        self.app.input_handler.ime.cancel_composition();
                        log::debug!("IME disabled");
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_position = position;
                if self.mouse_dragging {
                    self.handle_mouse_drag();
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                } else if let Some(gpu) = &self.gpu {
                    // Update hover state when not dragging
                    self.app.update_hover(
                        position.x as f32,
                        position.y as f32,
                        gpu.char_width(),
                        gpu.line_height(),
                    );
                    // Request redraw to check for hover timeout
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    match state {
                        ElementState::Pressed => {
                            self.mouse_dragging = true;
                            // Clear hover on click
                            self.app.clear_hover();
                            let extend = self.modifiers.shift_key();
                            self.handle_mouse_click(extend);
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                        ElementState::Released => {
                            self.mouse_dragging = false;
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Poll LSP for events (non-blocking)
                self.app.poll_lsp();

                // Send debounced document changes
                self.app.flush_pending_lsp_changes(false);

                // Update cursor blink
                let blink_needs_redraw = self.app.update_cursor_blink();

                // Update notifications (expire old ones)
                let notifications_need_redraw = self.app.notifications.update();

                // Update smooth scroll animation and syntax highlighting cache
                let scroll_needs_redraw = self
                    .app
                    .workspace
                    .active_editor_mut()
                    .map(|e| {
                        // Ensure syntax highlighting cache is up to date
                        if !e.highlighter().is_cache_valid() {
                            e.reparse_syntax();
                        }
                        e.update_smooth_scroll()
                    })
                    .unwrap_or(false);

                if let Some(gpu) = &mut self.gpu {
                    gpu.render(&self.app);
                }

                // Request next frame for continuous animations
                if let Some(window) = &self.window {
                    if blink_needs_redraw || scroll_needs_redraw || notifications_need_redraw || self.app.cursor_blink_enabled {
                        window.request_redraw();
                    }
                }
            }
            _ => {}
        }
    }
}

/// Runs the editor application.
pub fn run(app: EditorApp) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut state = AppState::new(app);
    event_loop.run_app(&mut state).expect("Event loop error");
}

/// Finds the project root directory by looking for common project markers.
/// Walks up the directory tree looking for files like Cargo.toml, package.json, .git, etc.
fn find_project_root(start_dir: &std::path::Path) -> Option<PathBuf> {
    let markers = [
        "Cargo.toml",       // Rust
        "package.json",     // Node.js
        "pyproject.toml",   // Python
        "setup.py",         // Python
        "go.mod",           // Go
        "CMakeLists.txt",   // C/C++
        "Makefile",         // General
        ".git",             // Git repo root
    ];

    let mut current = start_dir;
    loop {
        for marker in &markers {
            if current.join(marker).exists() {
                return Some(current.to_path_buf());
            }
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}
