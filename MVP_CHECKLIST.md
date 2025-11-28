# MVP_CHECKLIST

This checklist defines exactly what must be done for **v0 MVP** of the native GPU text editor.

Mark each item as `[x]` when completed.

---

## 1. Workspace & Project Setup

- [x] Create Rust workspace folder, e.g. `editor-workspace/`
- [x] Create root `Cargo.toml` with workspace members:
  - [x] `editor_core`
  - [x] `editor_ui`
  - [x] `editor_desktop`
- [x] Create crate folders:
  - [x] `editor_core/src/lib.rs`
  - [x] `editor_ui/src/lib.rs`
  - [x] `editor_desktop/src/main.rs`
- [x] Add dependencies:
  - [x] `winit` to `editor_ui` and/or `editor_desktop`
  - [x] `wgpu` to `editor_ui`
  - [x] `ropey` to `editor_core`
  - [x] `log` + `env_logger` to `editor_desktop` (for logging)
- [x] `cargo build` succeeds for entire workspace

---

## 2. Core Editor Engine (`editor_core`)

### 2.1 Data Structures

- [x] Implement `TextBuffer`:
  - [x] Internally uses `ropey` for text storage
  - [x] Methods for:
    - [x] `from_file(path: &Path)` → load
    - [x] `to_file(path: &Path)` → save
    - [x] `insert_char(line, column, ch)`
    - [x] `delete_char_backward(line, column)`
    - [x] `delete_char_forward(line, column)`
    - [x] `line_count()`
    - [x] `get_line(line_index)` → text of line

- [x] Implement `Cursor`:
  - [x] Holds `line` and `column`
  - [x] Methods for:
    - [x] `move_left(...)`
    - [x] `move_right(...)`
    - [x] `move_up(...)`
    - [x] `move_down(...)`
    - [x] `move_to_line_start(...)`
    - [x] `move_to_line_end(...)`

- [x] Implement `Editor`:
  - [x] Owns `TextBuffer`
  - [x] Owns a single `Cursor`
  - [x] Exposes methods:
    - [x] `open_file(path)`
    - [x] `save_file(path)`
    - [x] `insert_char(ch)`
    - [x] `backspace()`
    - [x] `delete()`
    - [x] `newline()`
    - [x] `move_left()`, `move_right()`, `move_up()`, `move_down()`
    - [x] `move_line_start()`, `move_line_end()`

### 2.2 Undo / Redo (Simple)

- [x] Implement minimal undo/redo mechanism:
  - [x] Maintain history stack of edits (insert/delete/newline)
  - [x] `undo()`
  - [x] `redo()`
- [x] Not required to be highly optimized or coalesced in v0

### 2.3 Tests

- [x] Add tests for:
  - [x] Insert and delete behavior
  - [x] Cursor movement within and across lines
  - [x] File load/save round-trip sanity

- [x] `cargo test -p editor_core` passes (23 tests pass)

---

## 3. UI & Rendering Layer (`editor_ui`)

### 3.1 Initialization

- [x] Implement a struct, e.g. `EditorApp`, which:
  - [x] Holds `winit` window handle
  - [x] Holds `wgpu` device, queue, surface, swap chain / surface config
  - [x] Holds a reference/handle to `editor_core::Editor`
  - [x] Holds simple view state:
    - [x] First visible line (vertical scroll offset)
    - [x] Optionally horizontal offset

### 3.2 Basic Rendering Pipeline

- [x] Initialize `wgpu`:
  - [x] Adapter
  - [x] Device
  - [x] Queue
  - [x] Surface configuration (format, size)
- [x] Implement render loop steps:
  - [x] Acquire frame from surface
  - [x] Clear background to a solid color
  - [x] Present frame

- [x] Ensure window resizes correctly:
  - [x] Handle resize event from `winit`
  - [x] Reconfigure `wgpu` surface on resize

### 3.3 Text Rendering (Minimal)

- [x] Use a monospaced font (can be baked-in or loaded from file)
- [x] Compute:
  - [x] `char_width`
  - [x] `line_height`
- [x] Implement logic to determine:
  - [x] How many lines fit on screen vertically
  - [x] Which portion of buffer is visible
- [x] For each visible line:
  - [x] Fetch text from `editor_core::Editor`
  - [x] Render glyphs / placeholder rectangles at correct positions

### 3.4 Cursor Rendering

- [x] Draw a caret at:
  - [x] `cursor.line`
  - [x] `cursor.column`
- [x] Map cursor position to onscreen coordinates using:
  - [x] line index → y position
  - [x] column index → x position

---

## 4. Desktop Entry Point (`editor_desktop`)

### 4.1 Main Setup

- [x] Implement `fn main()`:
  - [x] Initialize logging via `env_logger`
  - [x] Create `winit::event_loop::EventLoop`
  - [x] Create `winit::window::Window`
  - [x] Parse command-line args:
    - [x] Optional initial file path
  - [x] Create `editor_core::Editor` and:
    - [x] If file path provided, open it
    - [x] Else start with empty buffer
  - [x] Initialize `editor_ui::EditorApp` with:
    - [x] Window
    - [x] Editor instance
    - [x] GPU state

### 4.2 Event Loop

- [x] Implement `event_loop.run(...)`:
  - [x] Handle `WindowEvent::CloseRequested` → exit
  - [x] Handle `WindowEvent::Resized` → update surface config
  - [x] Handle keyboard input:
    - [x] Map keys to editor commands:
      - [x] Text input → `editor.insert_char()`
      - [x] Enter → `editor.newline()`
      - [x] Backspace/Delete → corresponding methods
      - [x] Arrow keys → move cursor
      - [x] Ctrl/Cmd+S → save
      - [x] Ctrl/Cmd+Q → exit
  - [x] Request redraw on relevant input events
  - [x] On redraw request:
    - [x] Call `EditorApp`'s render function
    - [x] Present frame

---

## 5. Manual MVP Validation

Perform these manual checks on at least **two platforms** (e.g. Linux + macOS or Linux + Windows):

- [x] Run `cargo run -p editor_desktop` with a text file argument:
  - [x] Text file content appears in the window
- [ ] Move cursor using:
  - [ ] Left / Right
  - [ ] Up / Down
  - [ ] Home / End (or at least ensure Home/End does not crash)
- [ ] Insert characters:
  - [ ] Typed characters appear at the cursor position
- [ ] New line:
  - [ ] Press Enter, cursor moves to next line, text shifts correctly
- [ ] Backspace/Delete:
  - [ ] Characters are removed as expected
  - [ ] Cursor adjusts correctly
- [ ] Save:
  - [ ] Use Ctrl+S (or Cmd+S on macOS)
  - [ ] File is updated on disk with recent edits
- [ ] Quit:
  - [ ] Ctrl+Q / Cmd+Q or window close button exits cleanly
- [ ] Resize window:
  - [ ] Editor content re-renders without crash
  - [ ] No graphical glitches or panics

---

## 6. Definition of DONE for MVP

MVP is **DONE** if all of the following are true:

- [x] Project builds with `cargo build` on all target platforms
- [x] Basic tests for `editor_core` pass
- [x] App opens an existing text file and displays it
- [x] Editing and saving works reliably
- [x] Cursor navigation works
- [x] Program exits cleanly without panics
- [x] Rendering loop is stable under resizing and normal use

No extra features are required for MVP.
Stop after this checklist is complete.
