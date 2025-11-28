# ROADMAP – Native GPU Text Editor

This roadmap defines post-MVP phases in a **strict order**.
Each phase builds on the previous one.
Do NOT skip phases.

---

## PHASE 1 – Rendering & Interaction Quality (Stability Phase)

Goal: Make the editor *feel* solid and smooth.

### Rendering Improvements
- [x] Implement proper glyph atlas caching
- [x] Batch glyph draw calls (reduce per-frame GPU submissions)
- [ ] Avoid full redraws when possible (dirty regions) - *deferred to Phase 7*
- [x] Improve caret rendering:
  - [x] Blink timer
  - [x] Pixel-perfect alignment
- [x] Proper selection rendering (background quad behind text)

### Scrolling & Viewport
- [x] Smooth vertical scrolling
- [x] Horizontal scrolling support
- [x] Mouse wheel scrolling
- [x] Scroll when cursor leaves viewport
- [x] Maintain first_visible_line / column correctly

### Input Quality
- [x] Text repeat (key hold)
- [x] IME-safe structure (don't fully implement IME yet)
- [x] Mouse cursor positioning (click to move cursor)
- [x] Click + drag selection

**EXIT CRITERIA:**
- Editor scrolls smoothly
- Cursor & selection feel consistent
- Rendering is stable under heavy scrolling

---

## PHASE 2 – Multi-file & Workspace Basics ✓ DONE

Goal: Move from "single file editor" to "real editor".

### Buffers & Files
- [x] Multiple buffers in memory
- [x] Open multiple files
- [x] Track modified/dirty buffers
- [x] Close buffer logic
- [x] Unsaved-changes protection (basic - warns in log)

### Basic UI Structure
- [x] Tab bar (top)
- [x] Active tab switching
- [x] Visual indication of dirty file (● indicator)

### File Operations
- [x] Open file dialog (Ctrl+O)
- [x] Save As (Ctrl+Shift+S)
- [x] Recent files list (simple in-memory list)

**EXIT CRITERIA:**
- Multiple files can be edited at once ✓
- No data loss ✓
- Navigation between files feels safe ✓

---

## PHASE 3 – Text Engine Power (Editor Brain Upgrade) ✓ DONE

Goal: Professional text manipulation.

### Core Editing
- [x] Multi-cursor support
- [x] Block / column selection
- [x] Word-based navigation (Ctrl+Left / Ctrl+Right)
- [x] Line duplication
- [x] Move line up/down
- [x] Smart Home / End behavior

### Undo / Redo Enhancements
- [x] Grouped edits
- [x] Undo per buffer
- [x] Time-based coalescing

**EXIT CRITERIA:**
- Advanced editors' muscle memory works ✓
- Power users feel comfortable ✓

---

## PHASE 4 – Syntax Highlighting (GPU-Aware) ✓ DONE

Goal: Add syntax awareness without destroying performance.

### Design Rules (Important)
- Highlighting MUST NOT block rendering ✓
- Perform parsing incrementally ✓
- Do NOT repaint entire buffer every keystroke ✓

### Implementation
- [x] Tokenization per language (Rust and JSON implemented)
- [x] Incremental re-tokenization (via tree-sitter)
- [x] GPU-side color mapping for tokens (per-character coloring)
- [x] Theme system (dark theme with syntax colors)

**EXIT CRITERIA:**
- Files highlight correctly ✓
- No visible lag during typing ✓
- Rendering remains smooth ✓

---

## PHASE 5 – Search, Replace & Navigation ✓ DONE

Goal: Make large files manageable.

### Search
- [x] Incremental search
- [x] Highlight matches
- [x] Jump between matches (F3 / Shift+F3)
- [x] Search across file

### Replace
- [x] Replace current
- [x] Replace all
- [x] Confirmation step for large changes (Shift+Enter for replace all)

### Navigation
- [x] Go to line (Ctrl+G)
- [ ] Symbol outline (basic version) - *deferred to Phase 6 (LSP)*
- [ ] Simple minimap (optional) - *deferred to Phase 7*

**EXIT CRITERIA:**
- Finding and modifying text is fast and reliable ✓

---

## PHASE 6 – Language Intelligence (LSP Phase) ✓ DONE

Goal: Transform editor into an IDE-capable tool.

### Architecture
- [x] LSP client as a **separate module**
- [x] Async communication isolated from render loop

### Features
- [x] Diagnostics (errors, warnings)
- [x] Hover information
- [x] Go to definition (F12)
- [x] Rename symbol (F2)
- [x] Auto-completion (basic popup, Ctrl+Space)

**EXIT CRITERIA:**
- LSP works without UI freezes ✓
- Editor remains stable even if server misbehaves ✓

---

## PHASE 7 – Performance & Large File Mastery

Goal: Be usable on *huge* files.

### Optimization
- [ ] Virtualized rendering (only visible lines)
- [ ] Fast seek in rope buffer
- [ ] Memory profiling
- [ ] Startup time profiling

### Stress Tests
- [ ] Open 100MB+ files
- [ ] Fast scroll test
- [ ] Typing latency measurements

**EXIT CRITERIA:**
- Editor stays responsive under stress
- No memory leaks
- CPU usage reasonable at idle

---

## PHASE 8 – Platform Polish & Distribution

Goal: Ship like a real product.

### Platform Integration
- [ ] Native menus (macOS menu bar, Windows)
- [ ] Clipboard integration
- [ ] Drag-and-drop files onto editor
- [ ] Proper DPI scaling

### Packaging
- [ ] Windows installer
- [ ] macOS app bundle
- [ ] Linux AppImage / package
- [ ] Auto-update strategy (optional)

**EXIT CRITERIA:**
- App installs and runs cleanly on all platforms

---

## PHASE 9 – Extensibility (Only If Needed)

Goal: Allow growth without killing performance.

### Plugin System (Careful)
- [ ] Define minimal plugin API
- [ ] Sandbox plugins
- [ ] Async-safe execution
- [ ] No plugin allowed to block render/UI thread

**WARNING:**
Do NOT rush plugins.
Many editors die here.

---

## FINAL NOTE

This editor is:
- CPU = **logic**
- GPU = **visuals**
- Strict separation = **longevity**

If performance ever conflicts with features:
**Performance wins.**

This roadmap prioritizes **correctness → speed → power**.
