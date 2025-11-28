//! Performance metrics and profiling.
//!
//! This module provides timing and memory tracking for performance analysis.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Maximum number of samples to keep for rolling averages.
const MAX_SAMPLES: usize = 120;

/// A single timing measurement.
#[derive(Debug, Clone, Copy)]
pub struct TimingSample {
    /// Duration of the operation.
    pub duration: Duration,
    /// Timestamp when the sample was taken.
    pub timestamp: Instant,
}

/// Rolling statistics for a metric.
#[derive(Debug, Clone)]
pub struct RollingStats {
    samples: VecDeque<TimingSample>,
    sum: Duration,
    min: Duration,
    max: Duration,
}

impl Default for RollingStats {
    fn default() -> Self {
        Self::new()
    }
}

impl RollingStats {
    /// Creates a new empty rolling stats tracker.
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(MAX_SAMPLES),
            sum: Duration::ZERO,
            min: Duration::MAX,
            max: Duration::ZERO,
        }
    }

    /// Records a new sample.
    pub fn record(&mut self, duration: Duration) {
        let sample = TimingSample {
            duration,
            timestamp: Instant::now(),
        };

        // Remove old sample if at capacity
        if self.samples.len() >= MAX_SAMPLES {
            if let Some(old) = self.samples.pop_front() {
                self.sum = self.sum.saturating_sub(old.duration);
            }
        }

        self.samples.push_back(sample);
        self.sum += duration;

        if duration < self.min {
            self.min = duration;
        }
        if duration > self.max {
            self.max = duration;
        }
    }

    /// Returns the number of samples.
    pub fn count(&self) -> usize {
        self.samples.len()
    }

    /// Returns the average duration.
    pub fn average(&self) -> Duration {
        if self.samples.is_empty() {
            Duration::ZERO
        } else {
            self.sum / self.samples.len() as u32
        }
    }

    /// Returns the minimum duration.
    pub fn min(&self) -> Duration {
        if self.samples.is_empty() {
            Duration::ZERO
        } else {
            self.min
        }
    }

    /// Returns the maximum duration.
    pub fn max(&self) -> Duration {
        self.max
    }

    /// Returns the most recent duration.
    pub fn last(&self) -> Duration {
        self.samples.back().map(|s| s.duration).unwrap_or(Duration::ZERO)
    }

    /// Returns the average as milliseconds.
    pub fn average_ms(&self) -> f64 {
        self.average().as_secs_f64() * 1000.0
    }

    /// Returns the most recent sample as milliseconds.
    pub fn last_ms(&self) -> f64 {
        self.last().as_secs_f64() * 1000.0
    }

    /// Clears all samples.
    pub fn clear(&mut self) {
        self.samples.clear();
        self.sum = Duration::ZERO;
        self.min = Duration::MAX;
        self.max = Duration::ZERO;
    }
}

/// Tracks frame timing statistics.
#[derive(Debug, Clone)]
pub struct FrameStats {
    /// Time spent rendering.
    pub render: RollingStats,
    /// Time spent processing input.
    pub input: RollingStats,
    /// Total frame time.
    pub frame: RollingStats,
    /// Frames per second (rolling average).
    last_fps_update: Instant,
    frame_count: u32,
    fps: f32,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameStats {
    /// Creates new frame statistics tracker.
    pub fn new() -> Self {
        Self {
            render: RollingStats::new(),
            input: RollingStats::new(),
            frame: RollingStats::new(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            fps: 0.0,
        }
    }

    /// Records render time.
    pub fn record_render(&mut self, duration: Duration) {
        self.render.record(duration);
    }

    /// Records input processing time.
    pub fn record_input(&mut self, duration: Duration) {
        self.input.record(duration);
    }

    /// Records total frame time and updates FPS.
    pub fn record_frame(&mut self, duration: Duration) {
        self.frame.record(duration);
        self.frame_count += 1;

        let elapsed = self.last_fps_update.elapsed();
        if elapsed >= Duration::from_secs(1) {
            self.fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.frame_count = 0;
            self.last_fps_update = Instant::now();
        }
    }

    /// Returns the current FPS.
    pub fn fps(&self) -> f32 {
        self.fps
    }
}

/// Tracks typing latency (time from keypress to screen update).
#[derive(Debug, Clone)]
pub struct TypingLatency {
    /// Latency measurements.
    pub latency: RollingStats,
    /// Pending keypress timestamp.
    pending_keypress: Option<Instant>,
}

impl Default for TypingLatency {
    fn default() -> Self {
        Self::new()
    }
}

impl TypingLatency {
    /// Creates new typing latency tracker.
    pub fn new() -> Self {
        Self {
            latency: RollingStats::new(),
            pending_keypress: None,
        }
    }

    /// Records a keypress event.
    pub fn keypress(&mut self) {
        self.pending_keypress = Some(Instant::now());
    }

    /// Records the render completion, calculating latency.
    pub fn render_complete(&mut self) {
        if let Some(keypress_time) = self.pending_keypress.take() {
            self.latency.record(keypress_time.elapsed());
        }
    }

    /// Returns the average latency in milliseconds.
    pub fn average_ms(&self) -> f64 {
        self.latency.average_ms()
    }

    /// Returns the most recent latency in milliseconds.
    pub fn last_ms(&self) -> f64 {
        self.latency.last_ms()
    }
}

/// Tracks scroll performance.
#[derive(Debug, Clone)]
pub struct ScrollPerf {
    /// Time to update scroll position.
    pub scroll_update: RollingStats,
    /// Time to render after scroll.
    pub scroll_render: RollingStats,
    /// Lines scrolled per second.
    lines_scrolled: u32,
    last_scroll_time: Instant,
    scroll_speed: f32,
}

impl Default for ScrollPerf {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollPerf {
    /// Creates new scroll performance tracker.
    pub fn new() -> Self {
        Self {
            scroll_update: RollingStats::new(),
            scroll_render: RollingStats::new(),
            lines_scrolled: 0,
            last_scroll_time: Instant::now(),
            scroll_speed: 0.0,
        }
    }

    /// Records scroll update time.
    pub fn record_scroll(&mut self, duration: Duration, lines: u32) {
        self.scroll_update.record(duration);
        self.lines_scrolled += lines;

        let elapsed = self.last_scroll_time.elapsed();
        if elapsed >= Duration::from_millis(500) {
            self.scroll_speed = self.lines_scrolled as f32 / elapsed.as_secs_f32();
            self.lines_scrolled = 0;
            self.last_scroll_time = Instant::now();
        }
    }

    /// Records render time after scroll.
    pub fn record_render(&mut self, duration: Duration) {
        self.scroll_render.record(duration);
    }

    /// Returns the scroll speed in lines per second.
    pub fn scroll_speed(&self) -> f32 {
        self.scroll_speed
    }
}

/// Memory usage statistics.
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryStats {
    /// Buffer memory usage in bytes.
    pub buffer_bytes: usize,
    /// Number of lines in the buffer.
    pub line_count: usize,
    /// Average bytes per line.
    pub avg_bytes_per_line: usize,
    /// Estimated total memory usage.
    pub estimated_total: usize,
}

impl MemoryStats {
    /// Updates memory stats from buffer info.
    pub fn update(&mut self, buffer_bytes: usize, line_count: usize) {
        self.buffer_bytes = buffer_bytes;
        self.line_count = line_count;
        self.avg_bytes_per_line = if line_count > 0 {
            buffer_bytes / line_count
        } else {
            0
        };
        // Rough estimate: buffer + syntax tree (~2x buffer) + undo history (~0.5x)
        self.estimated_total = buffer_bytes * 4;
    }

    /// Returns memory usage in megabytes.
    pub fn buffer_mb(&self) -> f64 {
        self.buffer_bytes as f64 / (1024.0 * 1024.0)
    }
}

/// Startup timing.
#[derive(Debug, Clone)]
pub struct StartupTiming {
    /// When the application started.
    start_time: Instant,
    /// Time to initialize GPU.
    pub gpu_init: Option<Duration>,
    /// Time to load the font atlas.
    pub font_init: Option<Duration>,
    /// Time to open a file.
    pub file_open: Option<Duration>,
    /// Time to first render.
    pub first_render: Option<Duration>,
    /// Time until ready for input.
    pub ready_time: Option<Duration>,
}

impl StartupTiming {
    /// Creates a new startup timing tracker.
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            gpu_init: None,
            font_init: None,
            file_open: None,
            first_render: None,
            ready_time: None,
        }
    }

    /// Records GPU initialization time.
    pub fn record_gpu_init(&mut self) {
        self.gpu_init = Some(self.start_time.elapsed());
    }

    /// Records font atlas initialization time.
    pub fn record_font_init(&mut self) {
        self.font_init = Some(self.start_time.elapsed());
    }

    /// Records file open time.
    pub fn record_file_open(&mut self) {
        self.file_open = Some(self.start_time.elapsed());
    }

    /// Records first render time.
    pub fn record_first_render(&mut self) {
        self.first_render = Some(self.start_time.elapsed());
    }

    /// Records time until ready for input.
    pub fn record_ready(&mut self) {
        self.ready_time = Some(self.start_time.elapsed());
    }

    /// Returns total startup time in milliseconds.
    pub fn total_ms(&self) -> f64 {
        self.ready_time
            .unwrap_or_else(|| self.start_time.elapsed())
            .as_secs_f64()
            * 1000.0
    }

    /// Returns a summary of startup timing.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if let Some(d) = self.gpu_init {
            parts.push(format!("GPU: {:.1}ms", d.as_secs_f64() * 1000.0));
        }
        if let Some(d) = self.font_init {
            parts.push(format!("Font: {:.1}ms", d.as_secs_f64() * 1000.0));
        }
        if let Some(d) = self.file_open {
            parts.push(format!("File: {:.1}ms", d.as_secs_f64() * 1000.0));
        }
        if let Some(d) = self.first_render {
            parts.push(format!("Render: {:.1}ms", d.as_secs_f64() * 1000.0));
        }
        if let Some(d) = self.ready_time {
            parts.push(format!("Total: {:.1}ms", d.as_secs_f64() * 1000.0));
        }

        parts.join(", ")
    }
}

impl Default for StartupTiming {
    fn default() -> Self {
        Self::new()
    }
}

/// Central performance metrics collector.
#[derive(Debug, Clone)]
pub struct PerfMetrics {
    /// Frame statistics.
    pub frame_stats: FrameStats,
    /// Typing latency.
    pub typing_latency: TypingLatency,
    /// Scroll performance.
    pub scroll_perf: ScrollPerf,
    /// Memory statistics.
    pub memory_stats: MemoryStats,
    /// Startup timing.
    pub startup: StartupTiming,
    /// Whether metrics collection is enabled.
    pub enabled: bool,
}

impl Default for PerfMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl PerfMetrics {
    /// Creates a new performance metrics collector.
    pub fn new() -> Self {
        Self {
            frame_stats: FrameStats::new(),
            typing_latency: TypingLatency::new(),
            scroll_perf: ScrollPerf::new(),
            memory_stats: MemoryStats::default(),
            startup: StartupTiming::new(),
            enabled: true,
        }
    }

    /// Returns a summary string for display in status bar.
    pub fn status_summary(&self) -> String {
        if !self.enabled {
            return String::new();
        }

        format!(
            "FPS: {:.0} | Frame: {:.1}ms | Latency: {:.1}ms | Mem: {:.1}MB",
            self.frame_stats.fps(),
            self.frame_stats.frame.average_ms(),
            self.typing_latency.average_ms(),
            self.memory_stats.buffer_mb(),
        )
    }

    /// Resets all metrics.
    pub fn reset(&mut self) {
        self.frame_stats = FrameStats::new();
        self.typing_latency = TypingLatency::new();
        self.scroll_perf = ScrollPerf::new();
        self.memory_stats = MemoryStats::default();
    }
}

/// RAII guard for timing a scope.
pub struct TimingGuard<'a> {
    start: Instant,
    stats: &'a mut RollingStats,
}

impl<'a> TimingGuard<'a> {
    /// Creates a new timing guard.
    pub fn new(stats: &'a mut RollingStats) -> Self {
        Self {
            start: Instant::now(),
            stats,
        }
    }
}

impl<'a> Drop for TimingGuard<'a> {
    fn drop(&mut self) {
        self.stats.record(self.start.elapsed());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_stats() {
        let mut stats = RollingStats::new();

        stats.record(Duration::from_millis(10));
        stats.record(Duration::from_millis(20));
        stats.record(Duration::from_millis(30));

        assert_eq!(stats.count(), 3);
        assert_eq!(stats.average(), Duration::from_millis(20));
        assert_eq!(stats.min(), Duration::from_millis(10));
        assert_eq!(stats.max(), Duration::from_millis(30));
        assert_eq!(stats.last(), Duration::from_millis(30));
    }

    #[test]
    fn test_rolling_stats_overflow() {
        let mut stats = RollingStats::new();

        // Fill beyond capacity
        for i in 0..150 {
            stats.record(Duration::from_millis(i as u64));
        }

        assert_eq!(stats.count(), MAX_SAMPLES);
    }

    #[test]
    fn test_memory_stats() {
        let mut stats = MemoryStats::default();
        stats.update(1024 * 1024, 1000); // 1MB, 1000 lines

        assert!((stats.buffer_mb() - 1.0).abs() < 0.01);
        // avg_bytes_per_line is integer division, so allow for rounding
        assert!(stats.avg_bytes_per_line >= 1000 && stats.avg_bytes_per_line <= 1100);
    }

    #[test]
    fn test_startup_timing() {
        let mut timing = StartupTiming::new();

        std::thread::sleep(Duration::from_millis(10));
        timing.record_gpu_init();

        assert!(timing.gpu_init.is_some());
        assert!(timing.gpu_init.unwrap() >= Duration::from_millis(10));
    }
}

/// Performance benchmarks for stress testing.
#[cfg(test)]
pub mod benchmarks {
    use crate::buffer::TextBuffer;
    use crate::Editor;
    use std::time::Instant;

    /// Generates a large text buffer with the specified number of lines.
    fn generate_large_buffer(lines: usize) -> String {
        let line = "This is a test line with some typical code content: let x = 42;\n";
        line.repeat(lines)
    }

    /// Benchmark: Large file loading (1M lines, ~50MB).
    #[test]
    fn bench_large_file_load() {
        let content = generate_large_buffer(1_000_000);
        let content_size = content.len();

        let start = Instant::now();
        let buffer = TextBuffer::from_str(&content);
        let load_time = start.elapsed();

        println!(
            "Large file load: {:.2}MB in {:.2}ms ({:.2}MB/s)",
            content_size as f64 / (1024.0 * 1024.0),
            load_time.as_secs_f64() * 1000.0,
            (content_size as f64 / (1024.0 * 1024.0)) / load_time.as_secs_f64()
        );

        // Line count may include an extra line due to trailing newline
        assert!(buffer.len_lines() >= 1_000_000 && buffer.len_lines() <= 1_000_001);
        // Should load in under 5 seconds (debug builds are slower)
        assert!(load_time.as_secs_f64() < 5.0, "Large file load took too long");
    }

    /// Benchmark: Line access in large buffer.
    #[test]
    fn bench_line_access() {
        let content = generate_large_buffer(100_000);
        let buffer = TextBuffer::from_str(&content);

        // Random access pattern
        let start = Instant::now();
        for i in 0..10_000 {
            let line_num = (i * 7) % 100_000;
            let _ = buffer.line(line_num);
        }
        let access_time = start.elapsed();

        println!(
            "Line access: 10000 random accesses in {:.2}ms ({:.2}us/access)",
            access_time.as_secs_f64() * 1000.0,
            access_time.as_secs_f64() * 1_000_000.0 / 10_000.0
        );

        // Should complete in under 500ms (debug builds are slower)
        assert!(access_time.as_millis() < 500, "Line access too slow");
    }

    /// Benchmark: Character insertion.
    #[test]
    fn bench_char_insertion() {
        let mut editor = Editor::new();

        let start = Instant::now();
        for _ in 0..10_000 {
            editor.insert_char('a');
        }
        let insert_time = start.elapsed();

        println!(
            "Character insertion: 10000 chars in {:.2}ms ({:.2}us/char)",
            insert_time.as_secs_f64() * 1000.0,
            insert_time.as_secs_f64() * 1_000_000.0 / 10_000.0
        );

        // Should complete in under 500ms
        assert!(insert_time.as_millis() < 500, "Character insertion too slow");
    }

    /// Benchmark: Search in large buffer.
    #[test]
    fn bench_search() {
        let content = generate_large_buffer(100_000);
        let mut editor = Editor::new();
        editor.set_buffer(TextBuffer::from_str(&content));

        let start = Instant::now();
        let match_count = editor.find("x = 42");
        let search_time = start.elapsed();

        println!(
            "Search: found {} matches in {:.2}ms",
            match_count,
            search_time.as_secs_f64() * 1000.0
        );

        // Should complete in under 500ms
        assert!(search_time.as_millis() < 500, "Search too slow");
        assert!(match_count > 0, "Should find matches");
    }

    /// Benchmark: Cursor navigation.
    #[test]
    fn bench_cursor_navigation() {
        let content = generate_large_buffer(10_000);
        let mut editor = Editor::new();
        editor.set_buffer(TextBuffer::from_str(&content));

        let start = Instant::now();
        for _ in 0..10_000 {
            editor.move_down(false);
        }
        for _ in 0..10_000 {
            editor.move_up(false);
        }
        let nav_time = start.elapsed();

        println!(
            "Navigation: 20000 moves in {:.2}ms ({:.2}us/move)",
            nav_time.as_secs_f64() * 1000.0,
            nav_time.as_secs_f64() * 1_000_000.0 / 20_000.0
        );

        // Should complete in under 2 seconds (debug builds are slower due to bounds checking)
        assert!(nav_time.as_millis() < 2000, "Navigation too slow");
    }

    /// Benchmark: Undo/Redo operations.
    #[test]
    fn bench_undo_redo() {
        let mut editor = Editor::new();

        // Create some history
        for i in 0..1000 {
            editor.insert_char(('a' as u8 + (i % 26) as u8) as char);
        }

        let start = Instant::now();
        for _ in 0..500 {
            editor.undo();
        }
        for _ in 0..500 {
            editor.redo();
        }
        let undo_time = start.elapsed();

        println!(
            "Undo/Redo: 1000 operations in {:.2}ms ({:.2}us/op)",
            undo_time.as_secs_f64() * 1000.0,
            undo_time.as_secs_f64() * 1_000_000.0 / 1000.0
        );

        // Should complete in under 100ms
        assert!(undo_time.as_millis() < 100, "Undo/Redo too slow");
    }
}
