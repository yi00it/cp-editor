//! Input handling and key mapping.

use winit::event::{ElementState, MouseScrollDelta};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// IME (Input Method Editor) composition state.
/// This tracks the state of text being composed through an IME.
#[derive(Debug, Default, Clone)]
pub struct ImeState {
    /// Whether IME composition is currently active.
    pub composing: bool,
    /// The current composition text (pre-edit text).
    pub composition: String,
    /// Cursor position within the composition text.
    pub cursor: usize,
}

impl ImeState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new composition.
    pub fn start_composition(&mut self) {
        self.composing = true;
        self.composition.clear();
        self.cursor = 0;
    }

    /// Updates the composition text.
    pub fn update_composition(&mut self, text: &str, cursor: usize) {
        self.composition = text.to_string();
        self.cursor = cursor;
    }

    /// Ends the composition and returns the final text.
    pub fn end_composition(&mut self) -> String {
        self.composing = false;
        let text = std::mem::take(&mut self.composition);
        self.cursor = 0;
        text
    }

    /// Cancels the composition without committing.
    pub fn cancel_composition(&mut self) {
        self.composing = false;
        self.composition.clear();
        self.cursor = 0;
    }
}

/// Represents an editor command.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorCommand {
    // File operations
    Save,
    SaveAs,
    OpenFile,
    NewFile,
    CloseTab,
    Quit,

    // Tab operations
    NextTab,
    PrevTab,
    SwitchToTab(usize),

    // Text input
    InsertChar(char),
    InsertNewline,

    // Deletion
    DeleteBackward,
    DeleteForward,

    // Cursor movement
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveWordLeft,
    MoveWordRight,
    MoveToLineStart,
    MoveToLineStartSmart,
    MoveToLineEnd,
    MovePageUp,
    MovePageDown,
    MoveToBufferStart,
    MoveToBufferEnd,

    // Selection
    SelectLeft,
    SelectRight,
    SelectUp,
    SelectDown,
    SelectWordLeft,
    SelectWordRight,
    SelectToLineStart,
    SelectToLineStartSmart,
    SelectToLineEnd,
    SelectPageUp,
    SelectPageDown,
    SelectToBufferStart,
    SelectToBufferEnd,
    SelectAll,

    // Line operations
    DuplicateLine,
    MoveLineUp,
    MoveLineDown,

    // Block selection
    ToggleBlockSelection,

    // Multi-cursor
    AddCursorAbove,
    AddCursorBelow,
    CollapseCursors,

    // Undo/Redo
    Undo,
    Redo,

    // Scrolling
    ScrollUp(f32),
    ScrollDown(f32),

    // Search & Replace
    OpenSearch,
    OpenReplace,
    FindNext,
    FindPrev,
    CloseSearch,

    // Navigation
    GoToLine,

    // LSP commands
    GotoDefinition,
    TriggerCompletion,
}

/// Input handler that maps keyboard/mouse events to editor commands.
pub struct InputHandler {
    modifiers: ModifiersState,
    /// IME composition state.
    pub ime: ImeState,
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            modifiers: ModifiersState::empty(),
            ime: ImeState::new(),
        }
    }

    pub fn update_modifiers_state(&mut self, modifiers: ModifiersState) {
        self.modifiers = modifiers;
    }

    fn is_primary_modifier(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.modifiers.super_key()
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.modifiers.control_key()
        }
    }

    fn is_shift(&self) -> bool {
        self.modifiers.shift_key()
    }

    fn is_alt(&self) -> bool {
        self.modifiers.alt_key()
    }

    /// Handle character input (for text entry).
    pub fn handle_char_input(&self, ch: char) -> Option<EditorCommand> {
        // Skip control characters and characters that are handled by key events
        if ch.is_control() || self.is_primary_modifier() {
            return None;
        }
        Some(EditorCommand::InsertChar(ch))
    }

    /// Handle key events using the new winit 0.30 API.
    pub fn handle_key_event_new(
        &self,
        key: &Key,
        state: ElementState,
    ) -> Option<EditorCommand> {
        if state != ElementState::Pressed {
            return None;
        }

        let primary = self.is_primary_modifier();
        let shift = self.is_shift();
        let alt = self.is_alt();

        match key {
            Key::Named(NamedKey::Enter) => Some(EditorCommand::InsertNewline),
            Key::Named(NamedKey::Backspace) => Some(EditorCommand::DeleteBackward),
            Key::Named(NamedKey::Delete) => Some(EditorCommand::DeleteForward),
            Key::Named(NamedKey::ArrowLeft) => {
                if primary && shift {
                    Some(EditorCommand::SelectWordLeft)
                } else if primary {
                    Some(EditorCommand::MoveWordLeft)
                } else if shift {
                    Some(EditorCommand::SelectLeft)
                } else {
                    Some(EditorCommand::MoveLeft)
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                if primary && shift {
                    Some(EditorCommand::SelectWordRight)
                } else if primary {
                    Some(EditorCommand::MoveWordRight)
                } else if shift {
                    Some(EditorCommand::SelectRight)
                } else {
                    Some(EditorCommand::MoveRight)
                }
            }
            Key::Named(NamedKey::ArrowUp) => {
                if primary && alt {
                    // Ctrl+Alt+Up: Add cursor above
                    Some(EditorCommand::AddCursorAbove)
                } else if alt && shift {
                    // Alt+Shift+Up: Extend block selection up
                    Some(EditorCommand::SelectUp)
                } else if alt {
                    Some(EditorCommand::MoveLineUp)
                } else if shift {
                    Some(EditorCommand::SelectUp)
                } else {
                    Some(EditorCommand::MoveUp)
                }
            }
            Key::Named(NamedKey::ArrowDown) => {
                if primary && alt {
                    // Ctrl+Alt+Down: Add cursor below
                    Some(EditorCommand::AddCursorBelow)
                } else if alt && shift {
                    // Alt+Shift+Down: Extend block selection down
                    Some(EditorCommand::SelectDown)
                } else if alt {
                    Some(EditorCommand::MoveLineDown)
                } else if shift {
                    Some(EditorCommand::SelectDown)
                } else {
                    Some(EditorCommand::MoveDown)
                }
            }
            Key::Named(NamedKey::Escape) => {
                // Escape: Close search or collapse multiple cursors to one
                Some(EditorCommand::CloseSearch)
            }
            Key::Named(NamedKey::F3) => {
                if shift {
                    Some(EditorCommand::FindPrev)
                } else {
                    Some(EditorCommand::FindNext)
                }
            }
            Key::Named(NamedKey::F12) => Some(EditorCommand::GotoDefinition),
            Key::Named(NamedKey::Home) => {
                if primary {
                    if shift {
                        Some(EditorCommand::SelectToBufferStart)
                    } else {
                        Some(EditorCommand::MoveToBufferStart)
                    }
                } else if shift {
                    // Smart home with selection
                    Some(EditorCommand::SelectToLineStartSmart)
                } else {
                    // Smart home: toggles between first non-whitespace and line start
                    Some(EditorCommand::MoveToLineStartSmart)
                }
            }
            Key::Named(NamedKey::End) => {
                if primary {
                    if shift {
                        Some(EditorCommand::SelectToBufferEnd)
                    } else {
                        Some(EditorCommand::MoveToBufferEnd)
                    }
                } else if shift {
                    Some(EditorCommand::SelectToLineEnd)
                } else {
                    Some(EditorCommand::MoveToLineEnd)
                }
            }
            Key::Named(NamedKey::PageUp) => {
                if shift {
                    Some(EditorCommand::SelectPageUp)
                } else {
                    Some(EditorCommand::MovePageUp)
                }
            }
            Key::Named(NamedKey::PageDown) => {
                if shift {
                    Some(EditorCommand::SelectPageDown)
                } else {
                    Some(EditorCommand::MovePageDown)
                }
            }
            // Tab navigation (must come before generic Tab handling)
            Key::Named(NamedKey::Tab) if primary && shift => Some(EditorCommand::PrevTab),
            Key::Named(NamedKey::Tab) if primary => Some(EditorCommand::NextTab),
            Key::Named(NamedKey::Tab) => Some(EditorCommand::InsertChar('\t')),
            Key::Named(NamedKey::Space) if primary => Some(EditorCommand::TriggerCompletion),
            Key::Named(NamedKey::Space) => Some(EditorCommand::InsertChar(' ')),

            // Character shortcuts
            Key::Character(ch) if primary => match ch.as_str() {
                "s" | "S" if shift => Some(EditorCommand::SaveAs),
                "s" | "S" => Some(EditorCommand::Save),
                "o" | "O" => Some(EditorCommand::OpenFile),
                "n" | "N" => Some(EditorCommand::NewFile),
                "w" | "W" => Some(EditorCommand::CloseTab),
                "q" | "Q" => Some(EditorCommand::Quit),
                "z" => Some(EditorCommand::Undo),
                "Z" => Some(EditorCommand::Redo),
                "y" | "Y" => Some(EditorCommand::Redo),
                "a" | "A" => Some(EditorCommand::SelectAll),
                "d" | "D" => Some(EditorCommand::DuplicateLine),
                "b" | "B" if shift => Some(EditorCommand::ToggleBlockSelection),
                // Search & Navigation
                "f" | "F" => Some(EditorCommand::OpenSearch),
                "h" | "H" => Some(EditorCommand::OpenReplace),
                "g" | "G" => Some(EditorCommand::GoToLine),
                // Tab switching with Ctrl+1-9
                "1" => Some(EditorCommand::SwitchToTab(0)),
                "2" => Some(EditorCommand::SwitchToTab(1)),
                "3" => Some(EditorCommand::SwitchToTab(2)),
                "4" => Some(EditorCommand::SwitchToTab(3)),
                "5" => Some(EditorCommand::SwitchToTab(4)),
                "6" => Some(EditorCommand::SwitchToTab(5)),
                "7" => Some(EditorCommand::SwitchToTab(6)),
                "8" => Some(EditorCommand::SwitchToTab(7)),
                "9" => Some(EditorCommand::SwitchToTab(8)),
                _ => None,
            },

            _ => None,
        }
    }

    pub fn handle_scroll(&self, delta: MouseScrollDelta) -> Option<EditorCommand> {
        match delta {
            MouseScrollDelta::LineDelta(_, y) => {
                if y > 0.0 {
                    Some(EditorCommand::ScrollUp(y.abs()))
                } else if y < 0.0 {
                    Some(EditorCommand::ScrollDown(y.abs()))
                } else {
                    None
                }
            }
            MouseScrollDelta::PixelDelta(pos) => {
                let lines = pos.y as f32 / 20.0;
                if lines > 0.0 {
                    Some(EditorCommand::ScrollUp(lines.abs()))
                } else if lines < 0.0 {
                    Some(EditorCommand::ScrollDown(lines.abs()))
                } else {
                    None
                }
            }
        }
    }
}
