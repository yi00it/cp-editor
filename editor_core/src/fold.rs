//! Code folding support.
//!
//! Provides detection and management of foldable code regions.

use crate::buffer::TextBuffer;

/// A foldable region in the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoldRegion {
    /// Start line of the fold (inclusive).
    pub start_line: usize,
    /// End line of the fold (inclusive).
    pub end_line: usize,
    /// Whether this region is currently folded.
    pub is_folded: bool,
}

impl FoldRegion {
    /// Creates a new fold region.
    pub fn new(start_line: usize, end_line: usize) -> Self {
        Self {
            start_line,
            end_line,
            is_folded: false,
        }
    }

    /// Returns the number of lines in this fold region.
    pub fn line_count(&self) -> usize {
        self.end_line - self.start_line + 1
    }

    /// Returns the number of hidden lines when folded.
    pub fn hidden_lines(&self) -> usize {
        if self.is_folded {
            self.end_line - self.start_line
        } else {
            0
        }
    }
}

/// Manages code folding for a buffer.
#[derive(Debug, Clone, Default)]
pub struct FoldManager {
    /// All fold regions, sorted by start line.
    regions: Vec<FoldRegion>,
    /// Whether fold detection is enabled.
    enabled: bool,
}

impl FoldManager {
    /// Creates a new fold manager.
    pub fn new() -> Self {
        Self {
            regions: Vec::new(),
            enabled: true,
        }
    }

    /// Returns whether folding is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables folding.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            // Unfold all when disabling
            for region in &mut self.regions {
                region.is_folded = false;
            }
        }
    }

    /// Clears all fold regions.
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    /// Returns all fold regions.
    pub fn regions(&self) -> &[FoldRegion] {
        &self.regions
    }

    /// Returns the fold region at the given line, if any.
    pub fn region_at_line(&self, line: usize) -> Option<&FoldRegion> {
        self.regions.iter().find(|r| r.start_line == line)
    }

    /// Returns a mutable reference to the fold region at the given line.
    pub fn region_at_line_mut(&mut self, line: usize) -> Option<&mut FoldRegion> {
        self.regions.iter_mut().find(|r| r.start_line == line)
    }

    /// Toggles the fold state at the given line.
    /// Returns true if a fold was toggled.
    pub fn toggle_fold_at_line(&mut self, line: usize) -> bool {
        if let Some(region) = self.region_at_line_mut(line) {
            region.is_folded = !region.is_folded;
            true
        } else {
            false
        }
    }

    /// Folds all regions.
    pub fn fold_all(&mut self) {
        for region in &mut self.regions {
            region.is_folded = true;
        }
    }

    /// Unfolds all regions.
    pub fn unfold_all(&mut self) {
        for region in &mut self.regions {
            region.is_folded = false;
        }
    }

    /// Returns true if the given line is hidden (inside a folded region).
    pub fn is_line_hidden(&self, line: usize) -> bool {
        self.regions.iter().any(|r| {
            r.is_folded && line > r.start_line && line <= r.end_line
        })
    }

    /// Returns true if the given line is the start of a fold region.
    pub fn is_fold_start(&self, line: usize) -> bool {
        self.regions.iter().any(|r| r.start_line == line)
    }

    /// Returns true if the given line is folded.
    pub fn is_line_folded(&self, line: usize) -> bool {
        self.regions.iter().any(|r| r.start_line == line && r.is_folded)
    }

    /// Detects fold regions based on brace matching.
    /// This is a simple implementation that looks for { } pairs.
    pub fn detect_brace_folds(&mut self, buffer: &TextBuffer) {
        self.regions.clear();

        let mut brace_stack: Vec<usize> = Vec::new(); // Stack of line numbers with opening braces

        for line in 0..buffer.len_lines() {
            if let Some(line_text) = buffer.line(line) {
                // Count braces on this line
                for ch in line_text.chars() {
                    match ch {
                        '{' => {
                            brace_stack.push(line);
                        }
                        '}' => {
                            if let Some(start_line) = brace_stack.pop() {
                                // Only create fold if it spans multiple lines
                                if line > start_line {
                                    self.regions.push(FoldRegion::new(start_line, line));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Sort by start line
        self.regions.sort_by_key(|r| r.start_line);
    }

    /// Detects fold regions based on indentation.
    /// Creates folds for blocks with increased indentation.
    pub fn detect_indent_folds(&mut self, buffer: &TextBuffer) {
        self.regions.clear();

        if buffer.len_lines() == 0 {
            return;
        }

        let mut indent_stack: Vec<(usize, usize)> = Vec::new(); // (line, indent_level)

        for line in 0..buffer.len_lines() {
            if let Some(line_text) = buffer.line(line) {
                let trimmed = line_text.trim();
                if trimmed.is_empty() {
                    continue; // Skip empty lines
                }

                let indent = line_text.chars().take_while(|c| c.is_whitespace()).count();

                // Close any folds with indent >= current indent
                while let Some(&(start_line, start_indent)) = indent_stack.last() {
                    if start_indent >= indent {
                        indent_stack.pop();
                        if line > start_line + 1 {
                            self.regions.push(FoldRegion::new(start_line, line - 1));
                        }
                    } else {
                        break;
                    }
                }

                // Check if this line ends with a fold-starting character
                if trimmed.ends_with('{') || trimmed.ends_with(':') {
                    indent_stack.push((line, indent));
                }
            }
        }

        // Close any remaining folds at end of file
        let last_line = buffer.len_lines().saturating_sub(1);
        while let Some((start_line, _)) = indent_stack.pop() {
            if last_line > start_line {
                self.regions.push(FoldRegion::new(start_line, last_line));
            }
        }

        // Sort and deduplicate
        self.regions.sort_by_key(|r| r.start_line);
        self.regions.dedup_by_key(|r| r.start_line);
    }

    /// Converts a buffer line to a visual line (accounting for folded regions).
    pub fn buffer_line_to_visual(&self, buffer_line: usize) -> usize {
        let mut visual = buffer_line;
        for region in &self.regions {
            if region.is_folded {
                if buffer_line > region.end_line {
                    // Line is after this fold - subtract hidden lines
                    visual -= region.hidden_lines();
                } else if buffer_line > region.start_line {
                    // Line is inside this fold - map to fold start
                    return self.buffer_line_to_visual(region.start_line);
                }
            }
        }
        visual
    }

    /// Converts a visual line to a buffer line (accounting for folded regions).
    pub fn visual_line_to_buffer(&self, visual_line: usize) -> usize {
        let mut buffer_line = visual_line;
        for region in &self.regions {
            if region.is_folded && buffer_line >= region.start_line {
                buffer_line += region.hidden_lines();
            }
        }
        buffer_line
    }

    /// Returns the total number of visible lines (accounting for folds).
    pub fn visible_line_count(&self, total_lines: usize) -> usize {
        let mut hidden = 0;
        for region in &self.regions {
            if region.is_folded {
                hidden += region.hidden_lines();
            }
        }
        total_lines.saturating_sub(hidden).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fold_region() {
        let mut region = FoldRegion::new(5, 10);
        assert_eq!(region.line_count(), 6);
        assert_eq!(region.hidden_lines(), 0);

        region.is_folded = true;
        assert_eq!(region.hidden_lines(), 5);
    }

    #[test]
    fn test_fold_manager_toggle() {
        let mut manager = FoldManager::new();
        manager.regions.push(FoldRegion::new(0, 5));
        manager.regions.push(FoldRegion::new(10, 15));

        assert!(!manager.is_line_folded(0));
        assert!(manager.toggle_fold_at_line(0));
        assert!(manager.is_line_folded(0));

        assert!(manager.is_line_hidden(3));
        assert!(!manager.is_line_hidden(7));
    }

    #[test]
    fn test_brace_fold_detection() {
        let buffer = TextBuffer::from_str("fn main() {\n    println!(\"Hello\");\n}\n");
        let mut manager = FoldManager::new();
        manager.detect_brace_folds(&buffer);

        assert_eq!(manager.regions.len(), 1);
        assert_eq!(manager.regions[0].start_line, 0);
        assert_eq!(manager.regions[0].end_line, 2);
    }
}
