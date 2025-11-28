//! LSP state management for the editor UI.
//!
//! This module provides LSP integration for the editor, managing LSP clients
//! and polling for updates without blocking the UI.

use cp_editor_core::{CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity, HoverInfo};
use cp_editor_lsp::{LspClient, LspHandle, LspNotification, LspResponse, ServerConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Manages LSP clients and state for the editor.
pub struct LspManager {
    /// Active LSP clients by language.
    clients: HashMap<String, LspClient>,
    /// Pending request IDs mapped to their type.
    pending_requests: HashMap<u64, PendingRequest>,
    /// Whether LSP is enabled.
    enabled: bool,
    /// Current workspace root.
    workspace_root: Option<PathBuf>,
}

/// Types of pending requests.
#[derive(Debug, Clone)]
enum PendingRequest {
    Hover { path: PathBuf },
    Completion { path: PathBuf },
    GotoDefinition { path: PathBuf },
    References { path: PathBuf },
    Rename { path: PathBuf },
}

/// LSP event to be handled by the UI.
#[derive(Debug, Clone)]
pub enum LspEvent {
    /// Diagnostics updated for a file.
    Diagnostics {
        path: PathBuf,
        diagnostics: Vec<Diagnostic>,
    },
    /// Hover information received.
    Hover {
        path: PathBuf,
        info: Option<HoverInfo>,
    },
    /// Completion items received.
    Completion {
        path: PathBuf,
        items: Vec<CompletionItem>,
    },
    /// Go to definition result.
    GotoDefinition {
        path: PathBuf,
        locations: Vec<(PathBuf, usize, usize)>,
    },
    /// Server initialized.
    ServerReady { language: String },
    /// Server error.
    Error { message: String },
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LspManager {
    /// Creates a new LSP manager.
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            pending_requests: HashMap::new(),
            enabled: true,
            workspace_root: None,
        }
    }

    /// Sets the workspace root.
    pub fn set_workspace_root(&mut self, path: Option<PathBuf>) {
        self.workspace_root = path;
    }

    /// Returns the current workspace root.
    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// Returns true if LSP is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enables or disables LSP.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.shutdown_all();
        }
    }

    /// Returns an LSP handle for the given language.
    fn get_handle(&self, language: &str) -> Option<LspHandle> {
        self.clients.get(language).map(|c| c.handle())
    }

    /// Starts an LSP client for the given language if not already running.
    pub fn start_client(&mut self, language: &str) -> bool {
        if !self.enabled {
            return false;
        }

        if self.clients.contains_key(language) {
            return true;
        }

        let config = match language {
            "rust" => Some(ServerConfig::rust_analyzer()),
            "python" => Some(ServerConfig::new("pylsp", vec![])),
            "javascript" | "typescript" => {
                Some(ServerConfig::new("typescript-language-server", vec!["--stdio".to_string()]))
            }
            "go" => Some(ServerConfig::new("gopls", vec![])),
            "c" | "cpp" => Some(ServerConfig::new("clangd", vec![])),
            _ => None,
        };

        if let Some(config) = config {
            match LspClient::start(config) {
                Ok(client) => {
                    log::info!("Started LSP client for {}", language);
                    self.clients.insert(language.to_string(), client);

                    // Initialize the server if we have a workspace root
                    if let Some(ref root) = self.workspace_root {
                        if let Some(handle) = self.get_handle(language) {
                            handle.initialize(root.clone());
                        }
                    }
                    return true;
                }
                Err(e) => {
                    log::warn!("Failed to start LSP for {}: {}", language, e);
                }
            }
        }

        false
    }

    /// Notifies LSP that a document was opened.
    pub fn did_open(&mut self, path: &Path, language: &str, text: &str) {
        if !self.enabled {
            return;
        }

        // Start client if needed
        self.start_client(language);

        if let Some(handle) = self.get_handle(language) {
            handle.did_open(path.to_path_buf(), language, text.to_string());
        }
    }

    /// Notifies LSP that a document changed.
    pub fn did_change(&mut self, path: &Path, language: &str, version: i32, text: &str) {
        if !self.enabled {
            return;
        }

        if let Some(handle) = self.get_handle(language) {
            handle.did_change(path.to_path_buf(), version, text.to_string());
        }
    }

    /// Notifies LSP that a document was saved.
    pub fn did_save(&mut self, path: &Path, language: &str) {
        if !self.enabled {
            return;
        }

        if let Some(handle) = self.get_handle(language) {
            handle.did_save(path.to_path_buf());
        }
    }

    /// Notifies LSP that a document was closed.
    pub fn did_close(&mut self, path: &Path, language: &str) {
        if !self.enabled {
            return;
        }

        if let Some(handle) = self.get_handle(language) {
            handle.did_close(path.to_path_buf());
        }
    }

    /// Requests hover information.
    pub fn hover(&mut self, path: &Path, language: &str, line: usize, col: usize) {
        if !self.enabled {
            return;
        }

        if let Some(handle) = self.get_handle(language) {
            let id = handle.hover(
                path.to_path_buf(),
                cp_editor_lsp::Position::new(line as u32, col as u32),
            );
            self.pending_requests
                .insert(id, PendingRequest::Hover { path: path.to_path_buf() });
        }
    }

    /// Requests completions.
    pub fn completion(&mut self, path: &Path, language: &str, line: usize, col: usize) {
        if !self.enabled {
            return;
        }

        if let Some(handle) = self.get_handle(language) {
            let id = handle.completion(
                path.to_path_buf(),
                cp_editor_lsp::Position::new(line as u32, col as u32),
            );
            self.pending_requests
                .insert(id, PendingRequest::Completion { path: path.to_path_buf() });
        }
    }

    /// Requests go to definition.
    pub fn goto_definition(&mut self, path: &Path, language: &str, line: usize, col: usize) {
        if !self.enabled {
            return;
        }

        if let Some(handle) = self.get_handle(language) {
            let id = handle.goto_definition(
                path.to_path_buf(),
                cp_editor_lsp::Position::new(line as u32, col as u32),
            );
            self.pending_requests
                .insert(id, PendingRequest::GotoDefinition { path: path.to_path_buf() });
        }
    }

    /// Requests find references.
    pub fn find_references(&mut self, path: &Path, language: &str, line: usize, col: usize) {
        if !self.enabled {
            return;
        }

        if let Some(handle) = self.get_handle(language) {
            let id = handle.find_references(
                path.to_path_buf(),
                cp_editor_lsp::Position::new(line as u32, col as u32),
                true, // include declaration
            );
            self.pending_requests
                .insert(id, PendingRequest::References { path: path.to_path_buf() });
        }
    }

    /// Requests rename symbol.
    pub fn rename(&mut self, path: &Path, language: &str, line: usize, col: usize, new_name: &str) {
        if !self.enabled {
            return;
        }

        if let Some(handle) = self.get_handle(language) {
            let id = handle.rename(
                path.to_path_buf(),
                cp_editor_lsp::Position::new(line as u32, col as u32),
                new_name.to_string(),
            );
            self.pending_requests
                .insert(id, PendingRequest::Rename { path: path.to_path_buf() });
        }
    }

    /// Polls for LSP events. Call this from the event loop.
    /// Returns a list of events to be processed by the UI.
    pub fn poll(&mut self) -> Vec<LspEvent> {
        // First, collect all responses and notifications
        let mut responses = Vec::new();
        let mut notifications = Vec::new();

        for client in self.clients.values() {
            // Poll for responses
            while let Some(response) = client.try_recv_response() {
                responses.push(response);
            }

            // Poll for notifications
            while let Some(notification) = client.try_recv_notification() {
                notifications.push(notification);
            }
        }

        // Now process them
        let mut events = Vec::new();

        for response in responses {
            if let Some(event) = self.handle_response(response) {
                events.push(event);
            }
        }

        for notification in notifications {
            if let Some(event) = self.handle_notification(notification) {
                events.push(event);
            }
        }

        events
    }

    /// Handles a response from the LSP server.
    fn handle_response(&mut self, response: LspResponse) -> Option<LspEvent> {
        match response {
            LspResponse::Initialized { id, capabilities_summary } => {
                log::info!("LSP server initialized (id: {}): {}", id, capabilities_summary);
                None
            }
            LspResponse::InitializeFailed { id, error } => {
                log::error!("LSP initialization failed (id: {}): {}", id, error);
                Some(LspEvent::Error { message: error })
            }
            LspResponse::Hover { id, info } => {
                if let Some(PendingRequest::Hover { path }) = self.pending_requests.remove(&id) {
                    let hover_info = info.map(|h| HoverInfo::new(h.contents));
                    Some(LspEvent::Hover {
                        path,
                        info: hover_info,
                    })
                } else {
                    None
                }
            }
            LspResponse::Completion { id, items } => {
                if let Some(PendingRequest::Completion { path }) = self.pending_requests.remove(&id) {
                    let completion_items: Vec<CompletionItem> = items
                        .into_iter()
                        .map(|item| CompletionItem {
                            label: item.label,
                            kind: item.kind.map(convert_completion_kind),
                            detail: item.detail,
                            insert_text: item.insert_text,
                        })
                        .collect();
                    Some(LspEvent::Completion {
                        path,
                        items: completion_items,
                    })
                } else {
                    None
                }
            }
            LspResponse::GotoDefinition { id, locations } => {
                if let Some(PendingRequest::GotoDefinition { path }) = self.pending_requests.remove(&id) {
                    let locs: Vec<(PathBuf, usize, usize)> = locations
                        .into_iter()
                        .map(|l| (l.path, l.range.start.line as usize, l.range.start.character as usize))
                        .collect();
                    Some(LspEvent::GotoDefinition {
                        path,
                        locations: locs,
                    })
                } else {
                    None
                }
            }
            LspResponse::References { id, locations: _ } => {
                self.pending_requests.remove(&id);
                // TODO: Handle references
                None
            }
            LspResponse::Rename { id, edit: _ } => {
                self.pending_requests.remove(&id);
                // TODO: Handle rename
                None
            }
            LspResponse::DocumentSymbols { id, symbols: _ } => {
                self.pending_requests.remove(&id);
                // TODO: Handle symbols
                None
            }
            LspResponse::Error { id, message } => {
                self.pending_requests.remove(&id);
                log::warn!("LSP request {} failed: {}", id, message);
                None
            }
        }
    }

    /// Handles a notification from the LSP server.
    fn handle_notification(&self, notification: LspNotification) -> Option<LspEvent> {
        match notification {
            LspNotification::Diagnostics { path, diagnostics } => {
                let diags: Vec<Diagnostic> = diagnostics
                    .into_iter()
                    .map(|d| {
                        let mut diag = Diagnostic::new(
                            d.range.start.line as usize,
                            d.range.start.character as usize,
                            d.range.end.line as usize,
                            d.range.end.character as usize,
                            convert_severity(d.severity),
                            d.message,
                        );
                        diag.code = d.code;
                        diag.source = d.source;
                        diag
                    })
                    .collect();
                Some(LspEvent::Diagnostics { path, diagnostics: diags })
            }
            LspNotification::ServerReady => {
                log::info!("LSP server is ready");
                None
            }
            LspNotification::ServerExited { code } => {
                log::info!("LSP server exited with code {:?}", code);
                None
            }
            LspNotification::Progress { token, message, percentage } => {
                if let Some(msg) = message {
                    log::debug!("LSP progress [{}]: {} ({}%)", token, msg, percentage.unwrap_or(0));
                }
                None
            }
            LspNotification::LogMessage { level, message } => {
                match level {
                    cp_editor_lsp::messages::LogLevel::Error => log::error!("LSP: {}", message),
                    cp_editor_lsp::messages::LogLevel::Warning => log::warn!("LSP: {}", message),
                    cp_editor_lsp::messages::LogLevel::Info => log::info!("LSP: {}", message),
                    cp_editor_lsp::messages::LogLevel::Log => log::debug!("LSP: {}", message),
                }
                None
            }
        }
    }

    /// Shuts down all LSP clients.
    pub fn shutdown_all(&mut self) {
        for (language, client) in self.clients.drain() {
            log::info!("Shutting down LSP client for {}", language);
            client.shutdown();
        }
        self.pending_requests.clear();
    }
}

/// Converts LSP severity to editor severity.
fn convert_severity(severity: cp_editor_lsp::DiagnosticSeverity) -> DiagnosticSeverity {
    match severity {
        cp_editor_lsp::DiagnosticSeverity::Error => DiagnosticSeverity::Error,
        cp_editor_lsp::DiagnosticSeverity::Warning => DiagnosticSeverity::Warning,
        cp_editor_lsp::DiagnosticSeverity::Information => DiagnosticSeverity::Information,
        cp_editor_lsp::DiagnosticSeverity::Hint => DiagnosticSeverity::Hint,
    }
}

/// Converts LSP completion kind to editor completion kind.
fn convert_completion_kind(kind: cp_editor_lsp::CompletionKind) -> CompletionKind {
    match kind {
        cp_editor_lsp::CompletionKind::Text => CompletionKind::Text,
        cp_editor_lsp::CompletionKind::Method => CompletionKind::Method,
        cp_editor_lsp::CompletionKind::Function => CompletionKind::Function,
        cp_editor_lsp::CompletionKind::Constructor => CompletionKind::Constructor,
        cp_editor_lsp::CompletionKind::Field => CompletionKind::Field,
        cp_editor_lsp::CompletionKind::Variable => CompletionKind::Variable,
        cp_editor_lsp::CompletionKind::Class => CompletionKind::Class,
        cp_editor_lsp::CompletionKind::Interface => CompletionKind::Interface,
        cp_editor_lsp::CompletionKind::Module => CompletionKind::Module,
        cp_editor_lsp::CompletionKind::Property => CompletionKind::Property,
        cp_editor_lsp::CompletionKind::Keyword => CompletionKind::Keyword,
        cp_editor_lsp::CompletionKind::Snippet => CompletionKind::Snippet,
        cp_editor_lsp::CompletionKind::Constant => CompletionKind::Constant,
        cp_editor_lsp::CompletionKind::Struct => CompletionKind::Struct,
        cp_editor_lsp::CompletionKind::Enum => CompletionKind::Enum,
        cp_editor_lsp::CompletionKind::EnumMember => CompletionKind::EnumMember,
        cp_editor_lsp::CompletionKind::TypeParameter => CompletionKind::TypeParameter,
        _ => CompletionKind::Other,
    }
}

/// Maps file extensions to LSP language IDs.
pub fn language_id_from_path(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "js" => Some("javascript"),
        "ts" => Some("typescript"),
        "jsx" => Some("javascriptreact"),
        "tsx" => Some("typescriptreact"),
        "go" => Some("go"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
        "java" => Some("java"),
        "rb" => Some("ruby"),
        "json" => Some("json"),
        "html" => Some("html"),
        "css" => Some("css"),
        "md" => Some("markdown"),
        "sh" | "bash" => Some("shellscript"),
        "yaml" | "yml" => Some("yaml"),
        "toml" => Some("toml"),
        _ => None,
    }
}
