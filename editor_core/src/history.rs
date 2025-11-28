//! Undo/Redo history system.

use crate::cursor::Selection;
use std::time::{Duration, Instant};

/// Default time window for coalescing edits (in milliseconds).
const COALESCE_WINDOW_MS: u64 = 300;

/// Represents a single edit operation that can be undone/redone.
#[derive(Debug, Clone)]
pub enum EditOperation {
    /// Insert text at position.
    Insert {
        position: usize,
        text: String,
    },
    /// Delete text at range.
    Delete {
        position: usize,
        text: String,
    },
}

impl EditOperation {
    /// Returns the inverse operation (for undo).
    pub fn inverse(&self) -> EditOperation {
        match self {
            EditOperation::Insert { position, text } => EditOperation::Delete {
                position: *position,
                text: text.clone(),
            },
            EditOperation::Delete { position, text } => EditOperation::Insert {
                position: *position,
                text: text.clone(),
            },
        }
    }
}

/// A group of edit operations that should be undone/redone together.
#[derive(Debug, Clone)]
pub struct EditGroup {
    /// The operations in this group (in order of execution).
    pub operations: Vec<EditOperation>,
    /// Cursor selection before the edit.
    pub selection_before: Selection,
    /// Cursor selection after the edit.
    pub selection_after: Selection,
    /// Timestamp of the last edit in this group.
    pub last_edit_time: Option<Instant>,
}

impl EditGroup {
    pub fn new(selection_before: Selection) -> Self {
        Self {
            operations: Vec::new(),
            selection_before,
            selection_after: selection_before,
            last_edit_time: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn push(&mut self, op: EditOperation) {
        self.operations.push(op);
        self.last_edit_time = Some(Instant::now());
    }

    pub fn set_selection_after(&mut self, selection: Selection) {
        self.selection_after = selection;
    }

    /// Returns true if this group can be coalesced with a new edit.
    /// Coalescing is allowed if:
    /// 1. The time since last edit is within the coalesce window
    /// 2. The new operation is compatible (e.g., consecutive inserts or deletes)
    pub fn can_coalesce(&self, new_op: &EditOperation, coalesce_window: Duration) -> bool {
        // Check time window
        if let Some(last_time) = self.last_edit_time {
            if last_time.elapsed() > coalesce_window {
                return false;
            }
        } else {
            return false;
        }

        // Check compatibility
        if self.operations.is_empty() {
            return true;
        }

        let last_op = self.operations.last().unwrap();
        match (last_op, new_op) {
            // Consecutive single-char inserts
            (
                EditOperation::Insert { position: pos1, text: text1 },
                EditOperation::Insert { position: pos2, text: text2 },
            ) => {
                // Only coalesce single character insertions that are consecutive
                text1.len() == 1 && text2.len() == 1
                    && *pos2 == *pos1 + text1.chars().count()
                    // Don't coalesce after newline
                    && !text1.ends_with('\n')
            }
            // Consecutive backspace deletions
            (
                EditOperation::Delete { position: pos1, .. },
                EditOperation::Delete { position: pos2, text: text2 },
            ) => {
                // Only coalesce single character deletions that are consecutive
                text2.len() == 1 && *pos2 == pos1.saturating_sub(1)
            }
            _ => false,
        }
    }

    /// Merges operations from another group into this one.
    pub fn merge(&mut self, other: EditGroup) {
        self.operations.extend(other.operations);
        self.selection_after = other.selection_after;
        self.last_edit_time = other.last_edit_time;
    }
}

/// Manages undo/redo history.
#[derive(Debug)]
pub struct History {
    /// Stack of operations that can be undone.
    undo_stack: Vec<EditGroup>,
    /// Stack of operations that can be redone.
    redo_stack: Vec<EditGroup>,
    /// Maximum number of undo levels.
    max_size: usize,
    /// Current edit group being built.
    current_group: Option<EditGroup>,
    /// Time window for coalescing edits.
    coalesce_window: Duration,
    /// Whether coalescing is enabled.
    coalesce_enabled: bool,
}

impl Default for History {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl History {
    /// Creates a new history with the given maximum size.
    pub fn new(max_size: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_size,
            current_group: None,
            coalesce_window: Duration::from_millis(COALESCE_WINDOW_MS),
            coalesce_enabled: true,
        }
    }

    /// Sets the coalesce window duration.
    pub fn set_coalesce_window(&mut self, window: Duration) {
        self.coalesce_window = window;
    }

    /// Enables or disables coalescing.
    pub fn set_coalesce_enabled(&mut self, enabled: bool) {
        self.coalesce_enabled = enabled;
    }

    /// Starts a new edit group.
    /// If coalescing is enabled and the previous group can be coalesced,
    /// we'll continue using it instead of starting a fresh group.
    pub fn begin_edit(&mut self, selection: Selection) {
        if self.current_group.is_some() {
            // Auto-commit previous group
            self.commit_edit();
        }
        self.current_group = Some(EditGroup::new(selection));
    }

    /// Records an operation in the current group.
    /// If coalescing is enabled and conditions are met, the operation
    /// may be added to the previous undo group instead.
    pub fn record(&mut self, op: EditOperation) {
        // Try to coalesce with the last committed group
        if self.coalesce_enabled && self.current_group.is_none() {
            if let Some(last_group) = self.undo_stack.last_mut() {
                if last_group.can_coalesce(&op, self.coalesce_window) {
                    last_group.push(op);
                    return;
                }
            }
        }

        // Otherwise, add to current group
        if let Some(group) = &mut self.current_group {
            group.push(op);
        }
    }

    /// Commits the current edit group.
    pub fn commit_edit(&mut self) {
        if let Some(group) = self.current_group.take() {
            if !group.is_empty() {
                // Try to coalesce with the last group if within time window
                if self.coalesce_enabled {
                    if let Some(last_group) = self.undo_stack.last_mut() {
                        // Check if first operation of new group can coalesce with last of previous
                        if let Some(first_op) = group.operations.first() {
                            if last_group.can_coalesce(first_op, self.coalesce_window) {
                                last_group.merge(group);
                                return;
                            }
                        }
                    }
                }
                self.push_undo(group);
            }
        }
    }

    /// Sets the selection after the current edit.
    pub fn set_selection_after(&mut self, selection: Selection) {
        if let Some(group) = &mut self.current_group {
            group.set_selection_after(selection);
        } else if self.coalesce_enabled {
            // If we're coalescing into the last group, update its selection
            if let Some(last_group) = self.undo_stack.last_mut() {
                last_group.set_selection_after(selection);
            }
        }
    }

    /// Pushes an edit group to the undo stack.
    fn push_undo(&mut self, group: EditGroup) {
        self.undo_stack.push(group);
        // Clear redo stack on new edit
        self.redo_stack.clear();
        // Enforce size limit
        while self.undo_stack.len() > self.max_size {
            self.undo_stack.remove(0);
        }
    }

    /// Returns true if undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Returns true if redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Pops the last edit group for undo.
    /// Returns the operations to undo and the selection to restore.
    pub fn undo(&mut self) -> Option<(Vec<EditOperation>, Selection)> {
        // Commit any pending edit
        self.commit_edit();
        
        self.undo_stack.pop().map(|group| {
            let selection = group.selection_before;
            // Create inverse operations in reverse order
            let ops: Vec<EditOperation> = group
                .operations
                .iter()
                .rev()
                .map(|op| op.inverse())
                .collect();
            // Push to redo stack
            self.redo_stack.push(group);
            (ops, selection)
        })
    }

    /// Pops the last undone edit group for redo.
    /// Returns the operations to redo and the selection to restore.
    pub fn redo(&mut self) -> Option<(Vec<EditOperation>, Selection)> {
        self.redo_stack.pop().map(|group| {
            let selection = group.selection_after;
            let ops = group.operations.clone();
            // Push back to undo stack
            self.undo_stack.push(group);
            (ops, selection)
        })
    }

    /// Clears all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.current_group = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undo_redo() {
        let mut history = History::new(100);
        let sel = Selection::new(0);
        
        // Record an insert
        history.begin_edit(sel);
        history.record(EditOperation::Insert {
            position: 0,
            text: "hello".to_string(),
        });
        history.set_selection_after(Selection::new(5));
        history.commit_edit();
        
        assert!(history.can_undo());
        assert!(!history.can_redo());
        
        // Undo
        let (ops, _sel) = history.undo().unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            EditOperation::Delete { position, text } => {
                assert_eq!(*position, 0);
                assert_eq!(text, "hello");
            }
            _ => panic!("Expected Delete"),
        }
        
        assert!(!history.can_undo());
        assert!(history.can_redo());
        
        // Redo
        let (ops, _sel) = history.redo().unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            EditOperation::Insert { position, text } => {
                assert_eq!(*position, 0);
                assert_eq!(text, "hello");
            }
            _ => panic!("Expected Insert"),
        }
    }

    #[test]
    fn test_redo_cleared_on_new_edit() {
        let mut history = History::new(100);
        let sel = Selection::new(0);
        
        // First edit
        history.begin_edit(sel);
        history.record(EditOperation::Insert {
            position: 0,
            text: "a".to_string(),
        });
        history.commit_edit();
        
        // Undo
        history.undo();
        assert!(history.can_redo());
        
        // New edit should clear redo
        history.begin_edit(sel);
        history.record(EditOperation::Insert {
            position: 0,
            text: "b".to_string(),
        });
        history.commit_edit();
        
        assert!(!history.can_redo());
    }

    #[test]
    fn test_coalescing_consecutive_inserts() {
        let mut history = History::new(100);
        // Very short window for testing - in reality this would be longer
        history.set_coalesce_window(Duration::from_millis(1000));

        // First character
        history.begin_edit(Selection::new(0));
        history.record(EditOperation::Insert {
            position: 0,
            text: "a".to_string(),
        });
        history.set_selection_after(Selection::new(1));
        history.commit_edit();

        // Second character (should coalesce)
        history.begin_edit(Selection::new(1));
        history.record(EditOperation::Insert {
            position: 1,
            text: "b".to_string(),
        });
        history.set_selection_after(Selection::new(2));
        history.commit_edit();

        // Third character (should coalesce)
        history.begin_edit(Selection::new(2));
        history.record(EditOperation::Insert {
            position: 2,
            text: "c".to_string(),
        });
        history.set_selection_after(Selection::new(3));
        history.commit_edit();

        // Should only have one undo group with all three operations
        assert_eq!(history.undo_stack.len(), 1);
        assert_eq!(history.undo_stack[0].operations.len(), 3);

        // Single undo should revert all three characters
        let (ops, _) = history.undo().unwrap();
        assert_eq!(ops.len(), 3);
    }

    #[test]
    fn test_coalescing_breaks_on_newline() {
        let mut history = History::new(100);
        history.set_coalesce_window(Duration::from_millis(1000));

        // First character
        history.begin_edit(Selection::new(0));
        history.record(EditOperation::Insert {
            position: 0,
            text: "a".to_string(),
        });
        history.commit_edit();

        // Newline (coalesces with 'a' since both are consecutive single chars)
        history.begin_edit(Selection::new(1));
        history.record(EditOperation::Insert {
            position: 1,
            text: "\n".to_string(),
        });
        history.commit_edit();

        // Character after newline (should NOT coalesce because previous was newline)
        history.begin_edit(Selection::new(2));
        history.record(EditOperation::Insert {
            position: 2,
            text: "b".to_string(),
        });
        history.commit_edit();

        // Should have two groups: ["a", "\n"] and ["b"]
        // Because 'b' after newline doesn't coalesce
        assert_eq!(history.undo_stack.len(), 2);
        assert_eq!(history.undo_stack[0].operations.len(), 2); // a and \n
        assert_eq!(history.undo_stack[1].operations.len(), 1); // b
    }

    #[test]
    fn test_coalescing_disabled() {
        let mut history = History::new(100);
        history.set_coalesce_enabled(false);
        history.set_coalesce_window(Duration::from_millis(1000));

        // Two consecutive inserts
        history.begin_edit(Selection::new(0));
        history.record(EditOperation::Insert {
            position: 0,
            text: "a".to_string(),
        });
        history.commit_edit();

        history.begin_edit(Selection::new(1));
        history.record(EditOperation::Insert {
            position: 1,
            text: "b".to_string(),
        });
        history.commit_edit();

        // Should have two separate groups when coalescing is disabled
        assert_eq!(history.undo_stack.len(), 2);
    }
}
