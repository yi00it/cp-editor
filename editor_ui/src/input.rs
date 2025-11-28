//! Input handling and key mapping.

use cp_editor_core::Editor;
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
    Quit,

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
    MoveToLineStart,
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
    SelectToLineStart,
    SelectToLineEnd,
    SelectPageUp,
    SelectPageDown,
    SelectToBufferStart,
    SelectToBufferEnd,
    SelectAll,

    // Undo/Redo
    Undo,
    Redo,

    // Scrolling
    ScrollUp(f32),
    ScrollDown(f32),
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

        match key {
            Key::Named(NamedKey::Enter) => Some(EditorCommand::InsertNewline),
            Key::Named(NamedKey::Backspace) => Some(EditorCommand::DeleteBackward),
            Key::Named(NamedKey::Delete) => Some(EditorCommand::DeleteForward),
            Key::Named(NamedKey::ArrowLeft) => {
                if shift {
                    Some(EditorCommand::SelectLeft)
                } else {
                    Some(EditorCommand::MoveLeft)
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                if shift {
                    Some(EditorCommand::SelectRight)
                } else {
                    Some(EditorCommand::MoveRight)
                }
            }
            Key::Named(NamedKey::ArrowUp) => {
                if shift {
                    Some(EditorCommand::SelectUp)
                } else {
                    Some(EditorCommand::MoveUp)
                }
            }
            Key::Named(NamedKey::ArrowDown) => {
                if shift {
                    Some(EditorCommand::SelectDown)
                } else {
                    Some(EditorCommand::MoveDown)
                }
            }
            Key::Named(NamedKey::Home) => {
                if primary {
                    if shift {
                        Some(EditorCommand::SelectToBufferStart)
                    } else {
                        Some(EditorCommand::MoveToBufferStart)
                    }
                } else if shift {
                    Some(EditorCommand::SelectToLineStart)
                } else {
                    Some(EditorCommand::MoveToLineStart)
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
            Key::Named(NamedKey::Tab) => Some(EditorCommand::InsertChar('\t')),
            Key::Named(NamedKey::Space) => Some(EditorCommand::InsertChar(' ')),

            // Character shortcuts
            Key::Character(ch) if primary => match ch.as_str() {
                "s" | "S" => Some(EditorCommand::Save),
                "q" | "Q" => Some(EditorCommand::Quit),
                "z" => Some(EditorCommand::Undo),
                "Z" => Some(EditorCommand::Redo),
                "y" | "Y" => Some(EditorCommand::Redo),
                "a" | "A" => Some(EditorCommand::SelectAll),
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

pub fn execute_command(editor: &mut Editor, command: EditorCommand) -> bool {
    match command {
        EditorCommand::Save => {
            if let Err(e) = editor.save() {
                log::error!("Failed to save: {}", e);
            }
            false
        }
        EditorCommand::Quit => true,

        EditorCommand::InsertChar(ch) => {
            editor.insert_char(ch);
            false
        }
        EditorCommand::InsertNewline => {
            editor.insert_newline();
            false
        }

        EditorCommand::DeleteBackward => {
            editor.delete_backward();
            false
        }
        EditorCommand::DeleteForward => {
            editor.delete_forward();
            false
        }

        EditorCommand::MoveLeft => {
            editor.move_left(false);
            false
        }
        EditorCommand::MoveRight => {
            editor.move_right(false);
            false
        }
        EditorCommand::MoveUp => {
            editor.move_up(false);
            false
        }
        EditorCommand::MoveDown => {
            editor.move_down(false);
            false
        }
        EditorCommand::MoveToLineStart => {
            editor.move_to_line_start(false);
            false
        }
        EditorCommand::MoveToLineEnd => {
            editor.move_to_line_end(false);
            false
        }
        EditorCommand::MovePageUp => {
            editor.move_page_up(false);
            false
        }
        EditorCommand::MovePageDown => {
            editor.move_page_down(false);
            false
        }
        EditorCommand::MoveToBufferStart => {
            editor.move_to_buffer_start(false);
            false
        }
        EditorCommand::MoveToBufferEnd => {
            editor.move_to_buffer_end(false);
            false
        }

        EditorCommand::SelectLeft => {
            editor.move_left(true);
            false
        }
        EditorCommand::SelectRight => {
            editor.move_right(true);
            false
        }
        EditorCommand::SelectUp => {
            editor.move_up(true);
            false
        }
        EditorCommand::SelectDown => {
            editor.move_down(true);
            false
        }
        EditorCommand::SelectToLineStart => {
            editor.move_to_line_start(true);
            false
        }
        EditorCommand::SelectToLineEnd => {
            editor.move_to_line_end(true);
            false
        }
        EditorCommand::SelectPageUp => {
            editor.move_page_up(true);
            false
        }
        EditorCommand::SelectPageDown => {
            editor.move_page_down(true);
            false
        }
        EditorCommand::SelectToBufferStart => {
            editor.move_to_buffer_start(true);
            false
        }
        EditorCommand::SelectToBufferEnd => {
            editor.move_to_buffer_end(true);
            false
        }
        EditorCommand::SelectAll => {
            editor.select_all();
            false
        }

        EditorCommand::Undo => {
            editor.undo();
            false
        }
        EditorCommand::Redo => {
            editor.redo();
            false
        }

        EditorCommand::ScrollUp(lines) => {
            let current = editor.scroll_offset();
            editor.set_scroll_offset(current.saturating_sub(lines as usize));
            false
        }
        EditorCommand::ScrollDown(lines) => {
            let current = editor.scroll_offset();
            editor.set_scroll_offset(current + lines as usize);
            false
        }
    }
}
