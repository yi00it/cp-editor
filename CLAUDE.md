# CP Editor

GPU-accelerated text editor written in Rust. "CP" stands for "Coding Platform" and "Cross Platform."

## Build & Run

```bash
# Build
cargo build --release

# Run (empty file)
cargo run -p cp-editor --release

# Run (open file)
cargo run -p cp-editor --release -- path/to/file.txt

# Test
cargo test -p cp-editor-core  # Core logic tests
cargo test                     # Full test suite
```

## Architecture

Three-layer workspace structure:

```
editor_desktop  →  editor_ui  →  editor_core
(entry point)      (rendering)    (pure logic)
```

- **editor_core**: Pure editor logic (no I/O, no GUI). Uses `ropey` for rope-based text buffer.
- **editor_ui**: GPU rendering (wgpu) and input handling (winit). Contains embedded JetBrains Mono font.
- **editor_desktop**: Main binary entry point.

## Key Files

| File | Purpose |
|------|---------|
| `editor_core/src/editor.rs` | Main Editor struct with all editing operations |
| `editor_core/src/buffer.rs` | TextBuffer wrapper around ropey rope |
| `editor_core/src/cursor.rs` | Cursor positioning and selection |
| `editor_core/src/history.rs` | Undo/redo system |
| `editor_core/src/workspace.rs` | Multi-buffer/tab workspace management |
| `editor_ui/src/app.rs` | EditorApp (event handling, state, tab bar) |
| `editor_ui/src/gpu_renderer.rs` | GPU rendering pipeline |
| `editor_ui/src/input.rs` | Keyboard/mouse handling |

## Keybindings

| Action | Windows/Linux | macOS |
|--------|---------------|-------|
| Save | Ctrl+S | Cmd+S |
| Save As | Ctrl+Shift+S | Cmd+Shift+S |
| Open File | Ctrl+O | Cmd+O |
| New Tab | Ctrl+N | Cmd+N |
| Close Tab | Ctrl+W | Cmd+W |
| Next Tab | Ctrl+Tab | Cmd+Tab |
| Previous Tab | Ctrl+Shift+Tab | Cmd+Shift+Tab |
| Switch to Tab 1-9 | Ctrl+1-9 | Cmd+1-9 |
| Quit | Ctrl+Q | Cmd+Q |
| Undo | Ctrl+Z | Cmd+Z |
| Redo | Ctrl+Shift+Z / Ctrl+Y | Cmd+Shift+Z |
| Select All | Ctrl+A | Cmd+A |

## Tech Stack

- **ropey**: Rope-based text buffer
- **wgpu**: GPU rendering
- **winit**: Cross-platform windowing
- **fontdue**: Font rasterization
- **rfd**: Native file dialogs

## Conventions

- Rust 2021 edition (toolchain 1.91.1)
- snake_case for functions/variables, PascalCase for types
- Tests in each module
- Document public APIs

## Requirements

- Rust 1.76+
- GPU with Vulkan, Metal, or DirectX 12 support
