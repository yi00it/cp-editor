# CP Editor

**CP** stands for:
- **C**oding **P**latform - Built for developers
- **C**ross **P**latform - Runs on Linux, macOS, and Windows

A fully native, GPU-accelerated text editor written in Rust.

## Features

- GPU-accelerated text rendering using wgpu
- Cross-platform: Linux, macOS, Windows
- Monospace font support (JetBrains Mono)
- Smooth scrolling and cursor blinking
- Mouse support (click to position, drag to select)
- Full undo/redo support
- Keyboard-driven workflow

## Building

```bash
cargo build --release
```

## Usage

```bash
# Open a new empty buffer
cp-editor

# Open a file
cp-editor path/to/file.txt

# Or run directly with cargo
cargo run --release -p cp-editor -- path/to/file.txt
```

## Keybindings

| Action | Windows/Linux | macOS |
|--------|---------------|-------|
| Save | Ctrl+S | Cmd+S |
| Quit | Ctrl+Q | Cmd+Q |
| Undo | Ctrl+Z | Cmd+Z |
| Redo | Ctrl+Shift+Z or Ctrl+Y | Cmd+Shift+Z |
| Select All | Ctrl+A | Cmd+A |
| Move Cursor | Arrow keys | Arrow keys |
| Home/End | Home/End | Home/End |
| Page Up/Down | PageUp/PageDown | PageUp/PageDown |
| Delete | Backspace/Delete | Backspace/Delete |
| New Line | Enter | Enter |

Hold Shift with any movement key to extend selection.

## Project Structure

```
CP-Editor/
├── editor_core/     # cp-editor-core: Pure editor logic (no GPU, no winit)
├── editor_ui/       # cp-editor-ui: Rendering + input mapping (wgpu + winit)
├── editor_desktop/  # cp-editor: Desktop entry point (main binary)
└── Cargo.toml       # Workspace definition
```

## Requirements

- Rust 1.76+
- A GPU with Vulkan, Metal, or DirectX 12 support

## License

MIT
