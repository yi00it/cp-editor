//! Main editor application with GPU rendering.

use crate::gpu_renderer::GpuRenderer;
use crate::input::{EditorCommand, InputHandler};
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
    /// Which input field is focused (0 = search, 1 = replace).
    pub focused_field: usize,
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
            focused_field: 0,
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

        // Draw line number background (below tab bar and search bar)
        renderer.draw_rect(
            0.0,
            content_y,
            self.line_number_margin,
            viewport_height as f32 - content_y,
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

        // Get cursor position for selection rendering
        let cursor_pos = editor.cursor_position();
        let selection_range = editor.selected_range();

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

            // Draw selection background for this line
            if let Some((sel_start, sel_end)) = selection_range {
                let line_start = buffer.line_start(buffer_line);
                let line_end = buffer.line_end(buffer_line);

                // Check if selection overlaps this line
                if sel_start < line_end + 1 && sel_end > line_start {
                    let sel_start_on_line = if sel_start > line_start {
                        sel_start - line_start
                    } else {
                        0
                    };
                    let sel_end_on_line = if sel_end < line_end + 1 {
                        sel_end - line_start
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
        }

        // Draw cursor
        if self.cursor_visible
            && cursor_pos.line >= base_scroll_line
            && cursor_pos.line <= base_scroll_line + visible_lines
            && cursor_pos.col >= horizontal_scroll
        {
            let cursor_screen_line = cursor_pos.line as f32 - smooth_scroll;
            let cursor_screen_col = cursor_pos.col - horizontal_scroll;
            let cursor_x = self.line_number_margin + cursor_screen_col as f32 * char_width;
            let cursor_y = content_y + cursor_screen_line * line_height;

            // Only draw if cursor is within visible area
            if cursor_y >= content_y && cursor_y < viewport_height as f32 {
                renderer.draw_rect(cursor_x, cursor_y, 2.0, line_height, renderer.colors.cursor);
            }
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
                                }
                                self.update_window_title();
                            } else {
                                // Replace current
                                if let Some(editor) = self.app.workspace.active_editor_mut() {
                                    editor.replace_current(&self.app.replace_text);
                                }
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
                if let Err(e) = self.app.workspace.save_active() {
                    if e.kind() == std::io::ErrorKind::Other {
                        // No file path - trigger Save As
                        self.show_save_as_dialog();
                    } else {
                        log::error!("Failed to save: {}", e);
                    }
                }
                self.update_window_title();
                false
            }
            EditorCommand::SaveAs => {
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
                    // TODO: Show confirmation dialog
                    log::warn!("Unsaved changes, quitting anyway for now");
                }
                true
            }
            EditorCommand::NextTab => {
                self.app.workspace.next_tab();
                self.update_window_title();
                false
            }
            EditorCommand::PrevTab => {
                self.app.workspace.prev_tab();
                self.update_window_title();
                false
            }
            EditorCommand::SwitchToTab(index) => {
                self.app.workspace.switch_to_tab(index);
                self.update_window_title();
                false
            }
            EditorCommand::InsertChar(ch) => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.insert_char(ch);
                }
                self.update_window_title();
                false
            }
            EditorCommand::InsertNewline => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.insert_newline();
                }
                self.update_window_title();
                false
            }
            EditorCommand::DeleteBackward => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.delete_backward();
                }
                self.update_window_title();
                false
            }
            EditorCommand::DeleteForward => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.delete_forward();
                }
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
                self.update_window_title();
                false
            }
            EditorCommand::MoveLineUp => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_line_up();
                }
                self.update_window_title();
                false
            }
            EditorCommand::MoveLineDown => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.move_line_down();
                }
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
                self.update_window_title();
                false
            }
            EditorCommand::Redo => {
                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.redo();
                }
                self.update_window_title();
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
                if let Err(e) = self.app.workspace.save_active_as(&path) {
                    log::error!("Failed to save file: {}", e);
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
                // TODO: Show confirmation dialog
                log::warn!("Closing modified buffer without saving");
            }
        }

        self.app.workspace.close_active_buffer();

        // If no buffers left, create a new one
        if self.app.workspace.tab_count() == 0 {
            self.app.workspace.new_buffer();
        }

        self.update_window_title();
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
                // Account for tab bar height
                let content_height = size.height as f32 - TAB_BAR_HEIGHT;
                let visible_lines = (content_height / gpu.line_height()) as usize;
                let visible_cols =
                    ((size.width as f32 - self.app.line_number_margin) / gpu.char_width()) as usize;

                if let Some(editor) = self.app.workspace.active_editor_mut() {
                    editor.set_visible_lines(visible_lines.max(1));
                    editor.set_visible_cols(visible_cols.max(1));
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
                    // TODO: Show confirmation dialog
                    log::warn!("Closing with unsaved changes");
                }
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
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    match state {
                        ElementState::Pressed => {
                            self.mouse_dragging = true;
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
                // Update cursor blink
                let blink_needs_redraw = self.app.update_cursor_blink();

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
                    if blink_needs_redraw || scroll_needs_redraw || self.app.cursor_blink_enabled {
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
