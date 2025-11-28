//! Software text renderer using softbuffer.

use crate::font::GlyphAtlas;

/// Colors for the editor UI (as u32 ARGB).
pub struct Colors {
    pub background: u32,
    pub text: u32,
    pub cursor: u32,
    pub selection: u32,
    pub line_number: u32,
    pub line_number_bg: u32,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            background: 0xFF1A1A1F,    // Dark background
            text: 0xFFE6E6E6,          // Light text
            cursor: 0xFFE6E6E6,        // Light cursor
            selection: 0x804D6699,     // Semi-transparent blue
            line_number: 0xFF808080,   // Gray
            line_number_bg: 0xFF141418, // Darker background
        }
    }
}

/// Software text renderer.
pub struct Renderer {
    /// Glyph atlas.
    atlas: GlyphAtlas,
    /// Frame buffer (ARGB format).
    buffer: Vec<u32>,
    /// Viewport width.
    width: u32,
    /// Viewport height.
    height: u32,
    /// Colors.
    pub colors: Colors,
}

impl Renderer {
    /// Creates a new renderer.
    pub fn new(width: u32, height: u32, font_size: f32) -> Self {
        let atlas = GlyphAtlas::new(font_size);
        let buffer = vec![0xFF1A1A1F; (width * height) as usize];

        Self {
            atlas,
            buffer,
            width,
            height,
            colors: Colors::default(),
        }
    }

    /// Returns the glyph atlas.
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// Resizes the renderer.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.buffer.resize((width * height) as usize, self.colors.background);
    }

    /// Clears the buffer with the background color.
    pub fn clear(&mut self) {
        self.buffer.fill(self.colors.background);
    }

    /// Draws a filled rectangle.
    pub fn draw_rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: u32) {
        let x0 = x.max(0) as u32;
        let y0 = y.max(0) as u32;
        let x1 = ((x + width) as u32).min(self.width);
        let y1 = ((y + height) as u32).min(self.height);

        // Check if color has alpha
        let alpha = (color >> 24) & 0xFF;
        
        if alpha == 0xFF {
            // Fully opaque - simple fill
            for py in y0..y1 {
                let row_start = (py * self.width) as usize;
                for px in x0..x1 {
                    self.buffer[row_start + px as usize] = color;
                }
            }
        } else if alpha > 0 {
            // Alpha blend
            let src_r = ((color >> 16) & 0xFF) as u32;
            let src_g = ((color >> 8) & 0xFF) as u32;
            let src_b = (color & 0xFF) as u32;
            let src_a = alpha as u32;
            let inv_a = 255 - src_a;

            for py in y0..y1 {
                let row_start = (py * self.width) as usize;
                for px in x0..x1 {
                    let idx = row_start + px as usize;
                    let dst = self.buffer[idx];
                    let dst_r = ((dst >> 16) & 0xFF) as u32;
                    let dst_g = ((dst >> 8) & 0xFF) as u32;
                    let dst_b = (dst & 0xFF) as u32;

                    let r = (src_r * src_a + dst_r * inv_a) / 255;
                    let g = (src_g * src_a + dst_g * inv_a) / 255;
                    let b = (src_b * src_a + dst_b * inv_a) / 255;

                    self.buffer[idx] = 0xFF000000 | (r << 16) | (g << 8) | b;
                }
            }
        }
    }

    /// Draws a single character.
    pub fn draw_char(&mut self, ch: char, x: f32, y: f32, color: u32) {
        let glyph = match self.atlas.get_glyph(ch) {
            Some(g) => g,
            None => return,
        };

        if glyph.width == 0 || glyph.height == 0 {
            return;
        }

        // Calculate screen position
        // fontdue's ymin is the offset from the baseline (positive = above baseline)
        let gx = (x + glyph.offset_x) as i32;
        let baseline_y = y + self.atlas.ascent;
        let gy = (baseline_y - glyph.offset_y - glyph.height as f32) as i32;

        let src_r = ((color >> 16) & 0xFF) as u32;
        let src_g = ((color >> 8) & 0xFF) as u32;
        let src_b = (color & 0xFF) as u32;

        for py in 0..glyph.height {
            let screen_y = gy + py as i32;
            if screen_y < 0 || screen_y >= self.height as i32 {
                continue;
            }

            let row_start = (screen_y as u32 * self.width) as usize;
            let atlas_row = (glyph.atlas_y + py) * self.atlas.width + glyph.atlas_x;

            for px in 0..glyph.width {
                let screen_x = gx + px as i32;
                if screen_x < 0 || screen_x >= self.width as i32 {
                    continue;
                }

                let atlas_idx = (atlas_row + px) as usize;
                let alpha = self.atlas.texture_data[atlas_idx] as u32;

                if alpha == 0 {
                    continue;
                }

                let idx = row_start + screen_x as usize;
                
                if alpha == 255 {
                    self.buffer[idx] = color;
                } else {
                    let dst = self.buffer[idx];
                    let dst_r = ((dst >> 16) & 0xFF) as u32;
                    let dst_g = ((dst >> 8) & 0xFF) as u32;
                    let dst_b = (dst & 0xFF) as u32;
                    let inv_a = 255 - alpha;

                    let r = (src_r * alpha + dst_r * inv_a) / 255;
                    let g = (src_g * alpha + dst_g * inv_a) / 255;
                    let b = (src_b * alpha + dst_b * inv_a) / 255;

                    self.buffer[idx] = 0xFF000000 | (r << 16) | (g << 8) | b;
                }
            }
        }
    }

    /// Draws a string at the given position.
    pub fn draw_text(&mut self, text: &str, mut x: f32, y: f32, color: u32) {
        for ch in text.chars() {
            self.draw_char(ch, x, y, color);
            x += self.atlas.char_width;
        }
    }

    /// Returns the buffer for display.
    pub fn buffer(&self) -> &[u32] {
        &self.buffer
    }

    /// Returns the viewport dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
