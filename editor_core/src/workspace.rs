//! Workspace management for multiple buffers/tabs.

use crate::editor::Editor;
use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};

/// Unique identifier for a buffer.
pub type BufferId = usize;

/// Information about a buffer tab.
#[derive(Debug, Clone)]
pub struct TabInfo {
    /// Buffer ID.
    pub id: BufferId,
    /// Display name (filename or "Untitled").
    pub name: String,
    /// Full file path, if any.
    pub path: Option<PathBuf>,
    /// Whether the buffer has unsaved changes.
    pub is_modified: bool,
}

/// Manages multiple editor buffers.
pub struct Workspace {
    /// All open buffers, indexed by BufferId.
    buffers: Vec<Option<Editor>>,
    /// Currently active buffer ID.
    active_buffer: Option<BufferId>,
    /// Order of tabs (buffer IDs in display order).
    tab_order: Vec<BufferId>,
    /// Next buffer ID to assign.
    next_id: BufferId,
    /// Recent files list (most recent first).
    recent_files: VecDeque<PathBuf>,
    /// Maximum number of recent files to track.
    max_recent_files: usize,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    /// Creates a new empty workspace.
    pub fn new() -> Self {
        Self {
            buffers: Vec::new(),
            active_buffer: None,
            tab_order: Vec::new(),
            next_id: 0,
            recent_files: VecDeque::new(),
            max_recent_files: 10,
        }
    }

    /// Creates a new empty buffer and returns its ID.
    pub fn new_buffer(&mut self) -> BufferId {
        let id = self.next_id;
        self.next_id += 1;

        let editor = Editor::new();

        // Ensure buffers vec is large enough
        if id >= self.buffers.len() {
            self.buffers.resize_with(id + 1, || None);
        }
        self.buffers[id] = Some(editor);
        self.tab_order.push(id);

        // Set as active if no active buffer
        if self.active_buffer.is_none() {
            self.active_buffer = Some(id);
        }

        id
    }

    /// Opens a file in a new buffer and returns its ID.
    pub fn open_file<P: AsRef<Path>>(&mut self, path: P) -> io::Result<BufferId> {
        let path = path.as_ref();

        // Check if file is already open
        if let Some(existing_id) = self.find_buffer_by_path(path) {
            self.active_buffer = Some(existing_id);
            return Ok(existing_id);
        }

        let id = self.next_id;
        self.next_id += 1;

        let mut editor = Editor::new();
        editor.open_file(path)?;

        // Add to recent files
        self.add_to_recent(path.to_path_buf());

        // Ensure buffers vec is large enough
        if id >= self.buffers.len() {
            self.buffers.resize_with(id + 1, || None);
        }
        self.buffers[id] = Some(editor);
        self.tab_order.push(id);
        self.active_buffer = Some(id);

        Ok(id)
    }

    /// Opens a file in the current buffer (replacing contents).
    pub fn open_file_in_current<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();

        if let Some(editor) = self.active_editor_mut() {
            editor.open_file(path)?;
            self.add_to_recent(path.to_path_buf());
            Ok(())
        } else {
            // No active buffer, create one
            self.open_file(path)?;
            Ok(())
        }
    }

    /// Finds a buffer by file path.
    fn find_buffer_by_path(&self, path: &Path) -> Option<BufferId> {
        for &id in &self.tab_order {
            if let Some(Some(editor)) = self.buffers.get(id) {
                if editor.file_path() == Some(path) {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Returns the currently active buffer ID.
    pub fn active_buffer_id(&self) -> Option<BufferId> {
        self.active_buffer
    }

    /// Returns a reference to the active editor.
    pub fn active_editor(&self) -> Option<&Editor> {
        self.active_buffer
            .and_then(|id| self.buffers.get(id))
            .and_then(|opt| opt.as_ref())
    }

    /// Returns a mutable reference to the active editor.
    pub fn active_editor_mut(&mut self) -> Option<&mut Editor> {
        self.active_buffer
            .and_then(|id| self.buffers.get_mut(id))
            .and_then(|opt| opt.as_mut())
    }

    /// Returns a reference to a specific buffer.
    pub fn get_buffer(&self, id: BufferId) -> Option<&Editor> {
        self.buffers.get(id).and_then(|opt| opt.as_ref())
    }

    /// Returns a mutable reference to a specific buffer.
    pub fn get_buffer_mut(&mut self, id: BufferId) -> Option<&mut Editor> {
        self.buffers.get_mut(id).and_then(|opt| opt.as_mut())
    }

    /// Sets the active buffer.
    pub fn set_active_buffer(&mut self, id: BufferId) -> bool {
        if self.buffers.get(id).map(|b| b.is_some()).unwrap_or(false) {
            self.active_buffer = Some(id);
            true
        } else {
            false
        }
    }

    /// Switches to the next tab.
    pub fn next_tab(&mut self) {
        if self.tab_order.len() <= 1 {
            return;
        }
        if let Some(active) = self.active_buffer {
            if let Some(pos) = self.tab_order.iter().position(|&id| id == active) {
                let next_pos = (pos + 1) % self.tab_order.len();
                self.active_buffer = Some(self.tab_order[next_pos]);
            }
        }
    }

    /// Switches to the previous tab.
    pub fn prev_tab(&mut self) {
        if self.tab_order.len() <= 1 {
            return;
        }
        if let Some(active) = self.active_buffer {
            if let Some(pos) = self.tab_order.iter().position(|&id| id == active) {
                let prev_pos = if pos == 0 {
                    self.tab_order.len() - 1
                } else {
                    pos - 1
                };
                self.active_buffer = Some(self.tab_order[prev_pos]);
            }
        }
    }

    /// Switches to a specific tab by index (0-based).
    pub fn switch_to_tab(&mut self, index: usize) {
        if index < self.tab_order.len() {
            self.active_buffer = Some(self.tab_order[index]);
        }
    }

    /// Returns information about all tabs.
    pub fn tabs(&self) -> Vec<TabInfo> {
        self.tab_order
            .iter()
            .filter_map(|&id| {
                self.buffers.get(id).and_then(|opt| {
                    opt.as_ref().map(|editor| TabInfo {
                        id,
                        name: editor
                            .file_path()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "Untitled".to_string()),
                        path: editor.file_path().map(|p| p.to_path_buf()),
                        is_modified: editor.is_modified(),
                    })
                })
            })
            .collect()
    }

    /// Returns the number of open tabs.
    pub fn tab_count(&self) -> usize {
        self.tab_order.len()
    }

    /// Returns the index of the active tab.
    pub fn active_tab_index(&self) -> Option<usize> {
        self.active_buffer.and_then(|id| {
            self.tab_order.iter().position(|&tab_id| tab_id == id)
        })
    }

    /// Closes a buffer by ID. Returns true if buffer was closed.
    /// Does not check for unsaved changes - caller should handle that.
    pub fn close_buffer(&mut self, id: BufferId) -> bool {
        if let Some(opt) = self.buffers.get_mut(id) {
            if opt.is_some() {
                *opt = None;

                // Remove from tab order
                if let Some(pos) = self.tab_order.iter().position(|&tab_id| tab_id == id) {
                    self.tab_order.remove(pos);
                }

                // Update active buffer if necessary
                if self.active_buffer == Some(id) {
                    self.active_buffer = self.tab_order.first().copied();
                }

                return true;
            }
        }
        false
    }

    /// Closes the active buffer. Returns the closed buffer ID if successful.
    pub fn close_active_buffer(&mut self) -> Option<BufferId> {
        if let Some(id) = self.active_buffer {
            if self.close_buffer(id) {
                return Some(id);
            }
        }
        None
    }

    /// Checks if any buffer has unsaved changes.
    pub fn has_unsaved_changes(&self) -> bool {
        self.tab_order.iter().any(|&id| {
            self.buffers
                .get(id)
                .and_then(|opt| opt.as_ref())
                .map(|e| e.is_modified())
                .unwrap_or(false)
        })
    }

    /// Returns IDs of all modified buffers.
    pub fn modified_buffers(&self) -> Vec<BufferId> {
        self.tab_order
            .iter()
            .filter(|&&id| {
                self.buffers
                    .get(id)
                    .and_then(|opt| opt.as_ref())
                    .map(|e| e.is_modified())
                    .unwrap_or(false)
            })
            .copied()
            .collect()
    }

    /// Saves the active buffer. Returns error if no path is set.
    pub fn save_active(&mut self) -> io::Result<()> {
        if let Some(editor) = self.active_editor_mut() {
            editor.save()
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "No active buffer"))
        }
    }

    /// Saves the active buffer to a new path.
    pub fn save_active_as<P: AsRef<Path>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(editor) = self.active_editor_mut() {
            editor.save_as(path)?;
            self.add_to_recent(path.to_path_buf());
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "No active buffer"))
        }
    }

    /// Adds a path to the recent files list.
    fn add_to_recent(&mut self, path: PathBuf) {
        // Remove if already present
        self.recent_files.retain(|p| p != &path);
        // Add to front
        self.recent_files.push_front(path);
        // Trim to max size
        while self.recent_files.len() > self.max_recent_files {
            self.recent_files.pop_back();
        }
    }

    /// Returns the recent files list.
    pub fn recent_files(&self) -> &VecDeque<PathBuf> {
        &self.recent_files
    }

    /// Clears the recent files list.
    pub fn clear_recent_files(&mut self) {
        self.recent_files.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_workspace() {
        let ws = Workspace::new();
        assert!(ws.active_buffer_id().is_none());
        assert_eq!(ws.tab_count(), 0);
    }

    #[test]
    fn test_new_buffer() {
        let mut ws = Workspace::new();
        let id = ws.new_buffer();

        assert_eq!(ws.tab_count(), 1);
        assert_eq!(ws.active_buffer_id(), Some(id));
        assert!(ws.active_editor().is_some());
    }

    #[test]
    fn test_multiple_buffers() {
        let mut ws = Workspace::new();
        let id1 = ws.new_buffer();
        let id2 = ws.new_buffer();
        let _id3 = ws.new_buffer();

        assert_eq!(ws.tab_count(), 3);
        // First one remains active
        assert_eq!(ws.active_buffer_id(), Some(id1));

        ws.set_active_buffer(id2);
        assert_eq!(ws.active_buffer_id(), Some(id2));
    }

    #[test]
    fn test_tab_switching() {
        let mut ws = Workspace::new();
        let id1 = ws.new_buffer();
        let id2 = ws.new_buffer();
        let id3 = ws.new_buffer();

        ws.set_active_buffer(id1);
        assert_eq!(ws.active_buffer_id(), Some(id1));

        ws.next_tab();
        assert_eq!(ws.active_buffer_id(), Some(id2));

        ws.next_tab();
        assert_eq!(ws.active_buffer_id(), Some(id3));

        ws.next_tab(); // Wrap around
        assert_eq!(ws.active_buffer_id(), Some(id1));

        ws.prev_tab(); // Back to end
        assert_eq!(ws.active_buffer_id(), Some(id3));
    }

    #[test]
    fn test_close_buffer() {
        let mut ws = Workspace::new();
        let id1 = ws.new_buffer();
        let id2 = ws.new_buffer();

        ws.set_active_buffer(id1);
        ws.close_buffer(id1);

        assert_eq!(ws.tab_count(), 1);
        assert_eq!(ws.active_buffer_id(), Some(id2));
    }

    #[test]
    fn test_tabs_info() {
        let mut ws = Workspace::new();
        ws.new_buffer();
        ws.new_buffer();

        let tabs = ws.tabs();
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].name, "Untitled");
        assert_eq!(tabs[1].name, "Untitled");
    }
}
