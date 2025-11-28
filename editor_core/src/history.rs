//! Undo/Redo history system.

use crate::cursor::Selection;

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
}

impl EditGroup {
    pub fn new(selection_before: Selection) -> Self {
        Self {
            operations: Vec::new(),
            selection_before,
            selection_after: selection_before,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn push(&mut self, op: EditOperation) {
        self.operations.push(op);
    }

    pub fn set_selection_after(&mut self, selection: Selection) {
        self.selection_after = selection;
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
        }
    }

    /// Starts a new edit group.
    pub fn begin_edit(&mut self, selection: Selection) {
        if self.current_group.is_some() {
            // Auto-commit previous group
            self.commit_edit();
        }
        self.current_group = Some(EditGroup::new(selection));
    }

    /// Records an operation in the current group.
    pub fn record(&mut self, op: EditOperation) {
        if let Some(group) = &mut self.current_group {
            group.push(op);
        }
    }

    /// Commits the current edit group.
    pub fn commit_edit(&mut self) {
        if let Some(group) = self.current_group.take() {
            if !group.is_empty() {
                self.push_undo(group);
            }
        }
    }

    /// Sets the selection after the current edit.
    pub fn set_selection_after(&mut self, selection: Selection) {
        if let Some(group) = &mut self.current_group {
            group.set_selection_after(selection);
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
}
