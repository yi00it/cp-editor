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
use winit::keyboard::{Key, ModifiersState};
use winit::window::{Window, WindowId};

/// Cursor blink interval in milliseconds.
const CURSOR_BLINK_INTERVAL_MS: u64 = 530;

/// Tab bar height in pixels.
const TAB_BAR_HEIGHT: f32 = 28.0;

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
        }
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
        // Adjust y for tab bar
        let y = y - TAB_BAR_HEIGHT;
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

        // Get active editor for rendering
        let Some(editor) = self.workspace.active_editor() else {
            return;
        };

        // Draw line number background (below tab bar)
        renderer.draw_rect(
            0.0,
            TAB_BAR_HEIGHT,
            self.line_number_margin,
            viewport_height as f32 - TAB_BAR_HEIGHT,
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

        // Draw visible lines
        for screen_line in 0..=visible_lines {
            let buffer_line = base_scroll_line + screen_line;
            if buffer_line >= total_lines {
                break;
            }

            // Apply fractional scroll offset, accounting for tab bar
            let y = TAB_BAR_HEIGHT + (screen_line as f32 - scroll_frac) * line_height;

            // Draw line number
            let line_num_str = format!("{:>4}", buffer_line + 1);
            renderer.draw_text(&line_num_str, 4.0, y, renderer.colors.line_number);

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
            let cursor_y = TAB_BAR_HEIGHT + cursor_screen_line * line_height;

            // Only draw if cursor is within visible area
            if cursor_y >= TAB_BAR_HEIGHT && cursor_y < viewport_height as f32 {
                renderer.draw_rect(cursor_x, cursor_y, 2.0, line_height, renderer.colors.cursor);
            }
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
        // Don't drag in tab bar
        if self.app.is_in_tab_bar(self.mouse_position.y as f32) {
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
