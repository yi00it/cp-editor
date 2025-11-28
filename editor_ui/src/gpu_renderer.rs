//! GPU-based text renderer using wgpu.
//!
//! Renders text directly on the GPU using instanced quads.

use crate::font::GlyphAtlas;
use wgpu::util::DeviceExt;

/// Colors for the editor UI.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub background: [f32; 4],
    pub text: [f32; 4],
    pub cursor: [f32; 4],
    pub selection: [f32; 4],
    pub line_number: [f32; 4],
    pub line_number_bg: [f32; 4],
    pub tab_bar_bg: [f32; 4],
    pub tab_active_bg: [f32; 4],
    pub tab_inactive_bg: [f32; 4],
    pub search_match: [f32; 4],
    pub search_match_current: [f32; 4],
    pub search_bar_bg: [f32; 4],
    pub input_field_bg: [f32; 4],
    pub input_field_border: [f32; 4],
    pub diagnostic_error: [f32; 4],
    pub diagnostic_warning: [f32; 4],
    pub diagnostic_info: [f32; 4],
    pub diagnostic_hint: [f32; 4],
    pub hover_bg: [f32; 4],
    pub hover_border: [f32; 4],
    pub completion_bg: [f32; 4],
    pub completion_selected_bg: [f32; 4],
    pub completion_border: [f32; 4],
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            background: [0.102, 0.102, 0.122, 1.0],    // #1A1A1F
            text: [0.902, 0.902, 0.902, 1.0],          // #E6E6E6
            cursor: [0.902, 0.902, 0.902, 1.0],        // #E6E6E6
            selection: [0.302, 0.400, 0.600, 0.5],     // Semi-transparent blue
            line_number: [0.502, 0.502, 0.502, 1.0],   // #808080
            line_number_bg: [0.078, 0.078, 0.094, 1.0], // #141418
            tab_bar_bg: [0.078, 0.078, 0.094, 1.0],    // #141418
            tab_active_bg: [0.102, 0.102, 0.122, 1.0], // #1A1A1F (same as background)
            tab_inactive_bg: [0.059, 0.059, 0.071, 1.0], // #0F0F12
            search_match: [0.600, 0.500, 0.200, 0.4],  // Yellow-orange background
            search_match_current: [0.800, 0.600, 0.200, 0.6], // Brighter for current match
            search_bar_bg: [0.059, 0.059, 0.071, 1.0], // Same as inactive tab
            input_field_bg: [0.102, 0.102, 0.122, 1.0], // Same as background
            input_field_border: [0.302, 0.302, 0.322, 1.0], // Light gray border
            diagnostic_error: [0.937, 0.325, 0.314, 1.0],   // #EF5350 - Red
            diagnostic_warning: [1.0, 0.757, 0.027, 1.0],   // #FFC107 - Amber
            diagnostic_info: [0.259, 0.647, 0.961, 1.0],    // #42A5F5 - Blue
            diagnostic_hint: [0.502, 0.502, 0.502, 1.0],    // #808080 - Gray
            hover_bg: [0.15, 0.15, 0.18, 0.95],             // Dark background with slight transparency
            hover_border: [0.3, 0.3, 0.35, 1.0],            // Subtle border
            completion_bg: [0.12, 0.12, 0.15, 0.98],        // Slightly darker for completion
            completion_selected_bg: [0.25, 0.35, 0.55, 1.0], // Blue highlight for selected
            completion_border: [0.3, 0.3, 0.35, 1.0],       // Same as hover border
        }
    }
}

/// A vertex for rendering quads (text glyphs or rectangles).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Position in pixels.
    pub position: [f32; 2],
    /// Texture coordinates (0-1 range, for glyph atlas).
    pub tex_coords: [f32; 2],
    /// RGBA color.
    pub color: [f32; 4],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,  // position
        1 => Float32x2,  // tex_coords
        2 => Float32x4,  // color
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Uniform buffer for projection matrix.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    projection: [[f32; 4]; 4],
}

impl Uniforms {
    fn new(width: f32, height: f32) -> Self {
        // Orthographic projection: (0,0) top-left, (width, height) bottom-right
        let projection = [
            [2.0 / width, 0.0, 0.0, 0.0],
            [0.0, -2.0 / height, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0, 1.0],
        ];
        Self { projection }
    }
}

/// GPU-based text and shape renderer.
pub struct GpuRenderer {
    /// Glyph atlas.
    atlas: GlyphAtlas,
    /// Viewport width.
    width: u32,
    /// Viewport height.
    height: u32,
    /// Colors.
    pub colors: Colors,

    // GPU resources
    render_pipeline: wgpu::RenderPipeline,
    rect_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    atlas_texture: wgpu::Texture,
    atlas_bind_group: wgpu::BindGroup,
    
    /// Vertices for text glyphs (rendered with atlas texture).
    text_vertices: Vec<Vertex>,
    /// Vertices for solid rectangles (background, cursor, selection).
    rect_vertices: Vec<Vertex>,
    
    /// Maximum number of vertices in buffers.
    max_vertices: usize,
    text_vertex_buffer: wgpu::Buffer,
    rect_vertex_buffer: wgpu::Buffer,
}

impl GpuRenderer {
    /// Creates a new GPU renderer.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        font_size: f32,
    ) -> Self {
        let atlas = GlyphAtlas::new(font_size);
        let colors = Colors::default();

        // Create glyph atlas texture
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph Atlas"),
            size: wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Upload atlas data
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.texture_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas.width),
                rows_per_image: Some(atlas.height),
            },
            wgpu::Extent3d {
                width: atlas.width,
                height: atlas.height,
                depth_or_array_layers: 1,
            },
        );

        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create uniform buffer
        let uniforms = Uniforms::new(width as f32, height as f32);
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group layouts
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let atlas_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Atlas Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Create bind groups
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atlas Bind Group"),
            layout: &atlas_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        // Create shaders
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text.wgsl").into()),
        });

        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/rect.wgsl").into()),
        });

        // Create pipelines
        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &atlas_bind_group_layout],
            push_constant_ranges: &[],
        });

        let rect_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Rect Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Render Pipeline"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rect Render Pipeline"),
            layout: Some(&rect_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // Create vertex buffers with initial capacity
        let max_vertices = 65536;
        let text_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Text Vertex Buffer"),
            size: (max_vertices * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let rect_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Rect Vertex Buffer"),
            size: (max_vertices * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            atlas,
            width,
            height,
            colors,
            render_pipeline,
            rect_pipeline,
            uniform_buffer,
            uniform_bind_group,
            atlas_texture,
            atlas_bind_group,
            text_vertices: Vec::with_capacity(max_vertices),
            rect_vertices: Vec::with_capacity(max_vertices),
            max_vertices,
            text_vertex_buffer,
            rect_vertex_buffer,
        }
    }

    /// Returns the glyph atlas.
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// Resizes the renderer.
    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;

        // Update projection matrix
        let uniforms = Uniforms::new(width as f32, height as f32);
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Clears all queued vertices.
    pub fn clear(&mut self) {
        self.text_vertices.clear();
        self.rect_vertices.clear();
    }

    /// Draws a filled rectangle.
    pub fn draw_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) {
        if self.rect_vertices.len() + 6 > self.max_vertices {
            return; // Buffer full
        }

        let x0 = x;
        let y0 = y;
        let x1 = x + width;
        let y1 = y + height;

        // Two triangles forming a quad
        self.rect_vertices.extend_from_slice(&[
            Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color },
            Vertex { position: [x1, y0], tex_coords: [1.0, 0.0], color },
            Vertex { position: [x1, y1], tex_coords: [1.0, 1.0], color },
            Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color },
            Vertex { position: [x1, y1], tex_coords: [1.0, 1.0], color },
            Vertex { position: [x0, y1], tex_coords: [0.0, 1.0], color },
        ]);
    }

    /// Draws a single character.
    pub fn draw_char(&mut self, ch: char, x: f32, y: f32, color: [f32; 4]) {
        let glyph = match self.atlas.get_glyph(ch) {
            Some(g) => g,
            None => return,
        };

        if glyph.width == 0 || glyph.height == 0 {
            return;
        }

        if self.text_vertices.len() + 6 > self.max_vertices {
            return; // Buffer full
        }

        // Calculate screen position
        let gx = x + glyph.offset_x;
        let baseline_y = y + self.atlas.ascent;
        let gy = baseline_y - glyph.offset_y - glyph.height as f32;

        let x0 = gx;
        let y0 = gy;
        let x1 = gx + glyph.width as f32;
        let y1 = gy + glyph.height as f32;

        // Texture coordinates (normalized)
        let atlas_width = self.atlas.width as f32;
        let atlas_height = self.atlas.height as f32;
        let u0 = glyph.atlas_x as f32 / atlas_width;
        let v0 = glyph.atlas_y as f32 / atlas_height;
        let u1 = (glyph.atlas_x + glyph.width) as f32 / atlas_width;
        let v1 = (glyph.atlas_y + glyph.height) as f32 / atlas_height;

        // Two triangles forming a quad
        self.text_vertices.extend_from_slice(&[
            Vertex { position: [x0, y0], tex_coords: [u0, v0], color },
            Vertex { position: [x1, y0], tex_coords: [u1, v0], color },
            Vertex { position: [x1, y1], tex_coords: [u1, v1], color },
            Vertex { position: [x0, y0], tex_coords: [u0, v0], color },
            Vertex { position: [x1, y1], tex_coords: [u1, v1], color },
            Vertex { position: [x0, y1], tex_coords: [u0, v1], color },
        ]);
    }

    /// Draws a string at the given position.
    pub fn draw_text(&mut self, text: &str, mut x: f32, y: f32, color: [f32; 4]) {
        for ch in text.chars() {
            self.draw_char(ch, x, y, color);
            x += self.atlas.char_width;
        }
    }

    /// Draws a squiggly underline (for diagnostics).
    /// The underline is drawn at the bottom of the line height.
    pub fn draw_squiggle(&mut self, x: f32, y: f32, width: f32, line_height: f32, color: [f32; 4]) {
        let underline_y = y + line_height - 2.0;
        let wave_height: f32 = 2.0;
        let wave_period: f32 = 4.0;
        let line_thickness: f32 = 1.5;

        // Draw a series of small diagonal lines to create a squiggle effect
        let mut current_x = x;
        let mut going_up = true;

        while current_x < x + width {
            let segment_width = wave_period.min(x + width - current_x);
            let x0 = current_x;
            let x1 = current_x + segment_width;

            let (y0, y1) = if going_up {
                (underline_y + wave_height, underline_y)
            } else {
                (underline_y, underline_y + wave_height)
            };

            // Draw a small quad for the diagonal segment
            // We'll approximate with a rectangle that covers the diagonal
            self.rect_vertices.extend_from_slice(&[
                Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color },
                Vertex { position: [x1, y0 - line_thickness], tex_coords: [1.0, 0.0], color },
                Vertex { position: [x1, y1], tex_coords: [1.0, 1.0], color },
                Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color },
                Vertex { position: [x1, y1], tex_coords: [1.0, 1.0], color },
                Vertex { position: [x0, y1 + line_thickness], tex_coords: [0.0, 1.0], color },
            ]);

            current_x += segment_width;
            going_up = !going_up;
        }
    }

    /// Draws a simple underline (alternative to squiggle).
    pub fn draw_underline(&mut self, x: f32, y: f32, width: f32, line_height: f32, color: [f32; 4]) {
        let underline_y = y + line_height - 2.0;
        self.draw_rect(x, underline_y, width, 2.0, color);
    }

    /// Returns the viewport dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Renders all queued geometry.
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
    ) {
        // Upload vertices to GPU
        if !self.rect_vertices.is_empty() {
            queue.write_buffer(
                &self.rect_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.rect_vertices),
            );
        }

        if !self.text_vertices.is_empty() {
            queue.write_buffer(
                &self.text_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.text_vertices),
            );
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Main Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.colors.background[0] as f64,
                            g: self.colors.background[1] as f64,
                            b: self.colors.background[2] as f64,
                            a: self.colors.background[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            // Draw rectangles first (backgrounds, selections)
            if !self.rect_vertices.is_empty() {
                render_pass.set_pipeline(&self.rect_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.rect_vertex_buffer.slice(..));
                render_pass.draw(0..self.rect_vertices.len() as u32, 0..1);
            }

            // Draw text on top
            if !self.text_vertices.is_empty() {
                render_pass.set_pipeline(&self.render_pipeline);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_bind_group(1, &self.atlas_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.text_vertex_buffer.slice(..));
                render_pass.draw(0..self.text_vertices.len() as u32, 0..1);
            }
        }

        queue.submit(std::iter::once(encoder.finish()));
    }
}
