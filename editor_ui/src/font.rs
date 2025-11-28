//! Font loading and glyph atlas generation.

use fontdue::{Font, FontSettings};

/// Embedded monospace font (JetBrains Mono or similar).
/// For v0, we embed a simple monospace font.
const EMBEDDED_FONT: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");

/// Glyph metrics for a single character.
#[derive(Debug, Clone, Copy)]
pub struct GlyphMetrics {
    /// X position in atlas (pixels).
    pub atlas_x: u32,
    /// Y position in atlas (pixels).
    pub atlas_y: u32,
    /// Width of glyph in atlas (pixels).
    pub width: u32,
    /// Height of glyph in atlas (pixels).
    pub height: u32,
    /// Horizontal offset when rendering.
    pub offset_x: f32,
    /// Vertical offset when rendering.
    pub offset_y: f32,
    /// Horizontal advance after rendering this glyph.
    pub advance: f32,
}

/// A texture atlas containing pre-rendered glyphs.
pub struct GlyphAtlas {
    /// The font used for rendering.
    #[allow(dead_code)]
    font: Font,
    /// Font size in pixels.
    font_size: f32,
    /// Atlas texture data (single channel, grayscale).
    pub texture_data: Vec<u8>,
    /// Atlas width in pixels.
    pub width: u32,
    /// Atlas height in pixels.
    pub height: u32,
    /// Metrics for ASCII characters (32-126).
    glyphs: Vec<Option<GlyphMetrics>>,
    /// Line height in pixels.
    pub line_height: f32,
    /// Character width (monospace).
    pub char_width: f32,
    /// Ascent (distance from baseline to top).
    pub ascent: f32,
    /// Descent (distance from baseline to bottom).
    pub descent: f32,
}

impl GlyphAtlas {
    /// Creates a new glyph atlas with the given font size.
    pub fn new(font_size: f32) -> Self {
        let font = Font::from_bytes(EMBEDDED_FONT, FontSettings::default())
            .expect("Failed to load embedded font");

        let metrics = font.horizontal_line_metrics(font_size).unwrap();
        let ascent = metrics.ascent;
        let descent = metrics.descent;
        let line_height = metrics.new_line_size;

        // Calculate atlas size - we render ASCII 32-126 (95 characters)
        // Arrange in a grid
        let chars_per_row = 16;
        let num_chars = 95;
        let rows = (num_chars + chars_per_row - 1) / chars_per_row;

        // Estimate max glyph size
        let max_glyph_size = (font_size * 1.5) as u32;
        let atlas_width = (chars_per_row as u32) * max_glyph_size;
        let atlas_height = (rows as u32) * max_glyph_size;

        let mut texture_data = vec![0u8; (atlas_width * atlas_height) as usize];
        let mut glyphs = vec![None; 128];

        // Get the advance width for a standard character (monospace)
        let (std_metrics, _) = font.rasterize('M', font_size);
        let char_width = std_metrics.advance_width;

        // Rasterize each ASCII character
        let mut x = 0u32;
        let mut y = 0u32;
        let mut row_height = 0u32;

        for c in 32u8..=126u8 {
            let ch = c as char;
            let (metrics, bitmap) = font.rasterize(ch, font_size);

            let glyph_width = metrics.width as u32;
            let glyph_height = metrics.height as u32;

            // Move to next row if needed
            if x + glyph_width > atlas_width {
                x = 0;
                y += row_height + 1;
                row_height = 0;
            }

            // Copy bitmap to atlas
            for gy in 0..glyph_height {
                for gx in 0..glyph_width {
                    let src_idx = (gy * glyph_width + gx) as usize;
                    let dst_x = x + gx;
                    let dst_y = y + gy;
                    let dst_idx = (dst_y * atlas_width + dst_x) as usize;
                    if src_idx < bitmap.len() && dst_idx < texture_data.len() {
                        texture_data[dst_idx] = bitmap[src_idx];
                    }
                }
            }

            glyphs[c as usize] = Some(GlyphMetrics {
                atlas_x: x,
                atlas_y: y,
                width: glyph_width,
                height: glyph_height,
                offset_x: metrics.xmin as f32,
                offset_y: metrics.ymin as f32,
                advance: metrics.advance_width,
            });

            x += glyph_width + 1;
            row_height = row_height.max(glyph_height);
        }

        Self {
            font,
            font_size,
            texture_data,
            width: atlas_width,
            height: atlas_height,
            glyphs,
            line_height,
            char_width,
            ascent,
            descent,
        }
    }

    /// Returns the metrics for a character, if available.
    pub fn get_glyph(&self, ch: char) -> Option<&GlyphMetrics> {
        let idx = ch as usize;
        if idx < self.glyphs.len() {
            self.glyphs[idx].as_ref()
        } else {
            // Return space glyph for unknown characters
            self.glyphs[' ' as usize].as_ref()
        }
    }

    /// Returns the font size.
    pub fn font_size(&self) -> f32 {
        self.font_size
    }
}
