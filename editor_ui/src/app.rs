//! Main editor application with GPU rendering.

use crate::gpu_renderer::GpuRenderer;
use crate::input::{execute_command, InputHandler};
use cp_editor_core::Editor;
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

/// The main editor application.
pub struct EditorApp {
    /// The editor state.
    pub editor: Editor,
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
}

impl EditorApp {
    /// Creates a new editor application.
    pub fn new(font_size: f32) -> Self {
        Self {
            editor: Editor::new(),
            input_handler: InputHandler::new(),
            font_size,
            line_number_margin: 60.0,
            cursor_visible: true,
            last_cursor_blink: Instant::now(),
            cursor_blink_enabled: true,
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
        let scroll_offset = self.editor.scroll_offset();
        let buffer = self.editor.buffer();

        // Calculate which line was clicked
        let screen_line = (y / line_height).floor() as usize;
        let buffer_line = scroll_offset + screen_line;
        let buffer_line = buffer_line.min(buffer.len_lines().saturating_sub(1));

        // Calculate which column was clicked (accounting for line number margin and horizontal scroll)
        let horizontal_scroll = self.editor.horizontal_scroll();
        let text_x = (x - self.line_number_margin).max(0.0);
        let col = (text_x / char_width).round() as usize + horizontal_scroll;

        // Clamp column to line length
        let line_len = buffer.line_len_chars(buffer_line);
        let col = col.min(line_len);

        (buffer_line, col)
    }

    /// Renders the editor to the GPU renderer.
    pub fn render(&self, renderer: &mut GpuRenderer) {
        renderer.clear();

        let line_height = renderer.atlas().line_height;
        let char_width = renderer.atlas().char_width;
        let (_, viewport_height) = renderer.dimensions();

        // Draw line number background
        renderer.draw_rect(
            0.0,
            0.0,
            self.line_number_margin,
            viewport_height as f32,
            renderer.colors.line_number_bg,
        );

        let smooth_scroll = self.editor.smooth_scroll();
        let horizontal_scroll = self.editor.horizontal_scroll();
        let visible_lines = self.editor.visible_lines();
        let buffer = self.editor.buffer();
        let total_lines = buffer.len_lines();

        // Calculate smooth scroll offset (fractional part for sub-line scrolling)
        let scroll_frac = smooth_scroll - smooth_scroll.floor();
        let base_scroll_line = smooth_scroll.floor() as usize;

        // Get cursor position for selection rendering
        let cursor_pos = self.editor.cursor_position();
        let selection_range = self.editor.selected_range();

        // Draw visible lines (render one extra line for smooth scrolling)
        for screen_line in 0..=visible_lines {
            let buffer_line = base_scroll_line + screen_line;
            if buffer_line >= total_lines {
                break;
            }

            // Apply fractional scroll offset
            let y = (screen_line as f32 - scroll_frac) * line_height;

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

            // Draw line text (with horizontal scroll offset)
            if let Some(line_text) = buffer.line(buffer_line) {
                // Skip characters before horizontal scroll
                let visible_text: String = line_text.chars().skip(horizontal_scroll).collect();
                let x = self.line_number_margin;
                renderer.draw_text(&visible_text, x, y, renderer.colors.text);
            }
        }

        // Draw cursor (only if visible due to blinking and within viewport)
        if self.cursor_visible
            && cursor_pos.line >= base_scroll_line
            && cursor_pos.line <= base_scroll_line + visible_lines
            && cursor_pos.col >= horizontal_scroll
        {
            let cursor_screen_line = cursor_pos.line as f32 - smooth_scroll;
            let cursor_screen_col = cursor_pos.col - horizontal_scroll;
            let cursor_x = self.line_number_margin + cursor_screen_col as f32 * char_width;
            let cursor_y = cursor_screen_line * line_height;

            // Only draw if cursor is within visible area
            if cursor_y >= 0.0 && cursor_y < viewport_height as f32 {
                renderer.draw_rect(cursor_x, cursor_y, 2.0, line_height, renderer.colors.cursor);
            }
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
            let (line, col) = self.app.screen_to_buffer_position(
                self.mouse_position.x as f32,
                self.mouse_position.y as f32,
                gpu.char_width(),
                gpu.line_height(),
            );
            self.app.editor.set_cursor_position(line, col, extend_selection);
            self.app.reset_cursor_blink();
        }
    }

    fn handle_mouse_drag(&mut self) {
        if let Some(gpu) = &self.gpu {
            let (line, col) = self.app.screen_to_buffer_position(
                self.mouse_position.x as f32,
                self.mouse_position.y as f32,
                gpu.char_width(),
                gpu.line_height(),
            );
            // Always extend selection during drag
            self.app.editor.set_cursor_position(line, col, true);
        }
    }
}

impl ApplicationHandler for AppState {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("CP Editor")
                .with_inner_size(PhysicalSize::new(1280u32, 720u32));

            let window = Arc::new(
                event_loop
                    .create_window(window_attributes)
                    .expect("Failed to create window"),
            );

            let gpu = GpuState::new(window.clone(), self.app.font_size);

            let size = window.inner_size();
            let visible_lines = (size.height as f32 / gpu.line_height()) as usize;
            let visible_cols = ((size.width as f32 - self.app.line_number_margin) / gpu.char_width()) as usize;
            self.app.editor.set_visible_lines(visible_lines.max(1));
            self.app.editor.set_visible_cols(visible_cols.max(1));

            self.window = Some(window.clone());
            self.gpu = Some(gpu);

            // Set up continuous redraw for cursor blinking
            event_loop.set_control_flow(ControlFlow::Poll);

            // Request initial redraw
            window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    if let Some(gpu) = &mut self.gpu {
                        gpu.resize(new_size);
                        let visible_lines = (new_size.height as f32 / gpu.line_height()) as usize;
                        let visible_cols = ((new_size.width as f32 - self.app.line_number_margin) / gpu.char_width()) as usize;
                        self.app.editor.set_visible_lines(visible_lines.max(1));
                        self.app.editor.set_visible_cols(visible_cols.max(1));
                    }
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
                // Handle both initial press and repeat events
                if state == ElementState::Pressed {
                    if let Some(command) = self
                        .app
                        .input_handler
                        .handle_key_event_new(&logical_key, state)
                    {
                        if execute_command(&mut self.app.editor, command) {
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
                                    execute_command(&mut self.app.editor, command);
                                    self.app.reset_cursor_blink();
                                    if let Some(window) = &self.window {
                                        window.request_redraw();
                                    }
                                }
                            }
                        }
                    }
                }

                // Log repeat events for debugging (can be removed later)
                if repeat {
                    log::trace!("Key repeat: {:?}", logical_key);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(command) = self.app.input_handler.handle_scroll(delta) {
                    execute_command(&mut self.app.editor, command);
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
                        // Update composition state
                        if text.is_empty() {
                            self.app.input_handler.ime.cancel_composition();
                        } else {
                            if !self.app.input_handler.ime.composing {
                                self.app.input_handler.ime.start_composition();
                            }
                            let cursor_pos = cursor.map(|(start, _)| start).unwrap_or(text.len());
                            self.app.input_handler.ime.update_composition(&text, cursor_pos);
                        }
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    Ime::Commit(text) => {
                        // Commit the text
                        self.app.input_handler.ime.end_composition();
                        self.app.editor.insert_text(&text);
                        self.app.reset_cursor_blink();
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
                
                // Update smooth scroll animation
                let scroll_needs_redraw = self.app.editor.update_smooth_scroll();

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
