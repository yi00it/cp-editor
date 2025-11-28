//! LSP Client - Manages the language server process and communication.
//!
//! The client runs on a separate tokio runtime thread and communicates
//! with the UI via channels.

use crate::messages::{
    DocumentSymbol, LogLevel, LspNotification, LspRequest, LspResponse, RequestId,
};
use crate::transport::{self, AsyncTransport, JsonRpcMessage, JsonRpcNotification, JsonRpcResponse};
use crate::types::{
    CompletionItem, Diagnostic, HoverInfo, Location, Position, TextEdit, WorkspaceEdit,
};
use crossbeam_channel::{Receiver, Sender};
use lsp_types::*;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use tokio::process::Command;
use tokio::sync::mpsc;

/// Converts a path to an LSP URI.
fn path_to_uri(path: &Path) -> Uri {
    let path_str = if cfg!(windows) {
        format!("file:///{}", path.to_string_lossy().replace('\\', "/"))
    } else {
        format!("file://{}", path.to_string_lossy())
    };
    path_str.parse().expect("Invalid URI from path")
}

/// Converts an LSP URI to a path.
fn uri_to_path(uri: &Uri) -> PathBuf {
    let uri_str = uri.as_str();
    if let Some(path_str) = uri_str.strip_prefix("file://") {
        // On Windows, paths look like file:///C:/...
        if cfg!(windows) && path_str.starts_with('/') {
            PathBuf::from(&path_str[1..])
        } else {
            PathBuf::from(path_str)
        }
    } else {
        PathBuf::from(uri_str)
    }
}

/// Language server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Command to start the server.
    pub command: String,
    /// Arguments to the command.
    pub args: Vec<String>,
    /// Working directory.
    pub working_dir: Option<PathBuf>,
}

impl ServerConfig {
    /// Creates a configuration for rust-analyzer.
    pub fn rust_analyzer() -> Self {
        Self {
            command: "rust-analyzer".to_string(),
            args: vec![],
            working_dir: None,
        }
    }

    /// Creates a configuration for a generic LSP server.
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
            working_dir: None,
        }
    }
}

/// Handle for sending requests to the LSP client.
#[derive(Clone)]
pub struct LspHandle {
    request_tx: Sender<LspRequest>,
    next_id: Arc<AtomicU64>,
}

impl LspHandle {
    /// Generates a new request ID.
    pub fn next_id(&self) -> RequestId {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Sends a request to the LSP client.
    pub fn send(&self, request: LspRequest) -> Result<(), crossbeam_channel::SendError<LspRequest>> {
        self.request_tx.send(request)
    }

    /// Initializes the LSP server.
    pub fn initialize(&self, root_path: PathBuf) -> RequestId {
        let id = self.next_id();
        let _ = self.send(LspRequest::Initialize { id, root_path });
        id
    }

    /// Notifies that a document was opened.
    pub fn did_open(&self, path: PathBuf, language_id: &str, text: String) {
        let _ = self.send(LspRequest::DidOpen {
            path,
            language_id: language_id.to_string(),
            version: 1,
            text,
        });
    }

    /// Notifies that a document changed.
    pub fn did_change(&self, path: PathBuf, version: i32, text: String) {
        let _ = self.send(LspRequest::DidChange {
            path,
            version,
            text,
        });
    }

    /// Notifies that a document was saved.
    pub fn did_save(&self, path: PathBuf) {
        let _ = self.send(LspRequest::DidSave { path });
    }

    /// Notifies that a document was closed.
    pub fn did_close(&self, path: PathBuf) {
        let _ = self.send(LspRequest::DidClose { path });
    }

    /// Requests hover information.
    pub fn hover(&self, path: PathBuf, position: Position) -> RequestId {
        let id = self.next_id();
        let _ = self.send(LspRequest::Hover { id, path, position });
        id
    }

    /// Requests completions.
    pub fn completion(&self, path: PathBuf, position: Position) -> RequestId {
        let id = self.next_id();
        let _ = self.send(LspRequest::Completion { id, path, position });
        id
    }

    /// Requests go to definition.
    pub fn goto_definition(&self, path: PathBuf, position: Position) -> RequestId {
        let id = self.next_id();
        let _ = self.send(LspRequest::GotoDefinition { id, path, position });
        id
    }

    /// Requests find references.
    pub fn find_references(
        &self,
        path: PathBuf,
        position: Position,
        include_declaration: bool,
    ) -> RequestId {
        let id = self.next_id();
        let _ = self.send(LspRequest::FindReferences {
            id,
            path,
            position,
            include_declaration,
        });
        id
    }

    /// Requests rename symbol.
    pub fn rename(&self, path: PathBuf, position: Position, new_name: String) -> RequestId {
        let id = self.next_id();
        let _ = self.send(LspRequest::Rename {
            id,
            path,
            position,
            new_name,
        });
        id
    }

    /// Shuts down the LSP server.
    pub fn shutdown(&self) {
        let _ = self.send(LspRequest::Shutdown);
    }
}

/// The LSP client.
pub struct LspClient {
    /// Handle for sending requests.
    handle: LspHandle,
    /// Receiver for responses and notifications.
    response_rx: Receiver<LspResponse>,
    notification_rx: Receiver<LspNotification>,
    /// Whether the server is running.
    running: Arc<AtomicBool>,
}

impl LspClient {
    /// Starts a new LSP client with the given server configuration.
    pub fn start(config: ServerConfig) -> std::io::Result<Self> {
        let (request_tx, request_rx) = crossbeam_channel::unbounded();
        let (response_tx, response_rx) = crossbeam_channel::unbounded();
        let (notification_tx, notification_rx) = crossbeam_channel::unbounded();
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        // Spawn the client thread with tokio runtime
        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(async {
                if let Err(e) = run_client(config, request_rx, response_tx, notification_tx, running_clone).await {
                    log::error!("LSP client error: {}", e);
                }
            });
        });

        Ok(Self {
            handle: LspHandle {
                request_tx,
                next_id: Arc::new(AtomicU64::new(1)),
            },
            response_rx,
            notification_rx,
            running,
        })
    }

    /// Returns a handle for sending requests.
    pub fn handle(&self) -> LspHandle {
        self.handle.clone()
    }

    /// Tries to receive a response (non-blocking).
    pub fn try_recv_response(&self) -> Option<LspResponse> {
        self.response_rx.try_recv().ok()
    }

    /// Tries to receive a notification (non-blocking).
    pub fn try_recv_notification(&self) -> Option<LspNotification> {
        self.notification_rx.try_recv().ok()
    }

    /// Returns whether the server is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Shuts down the client.
    pub fn shutdown(&self) {
        self.handle.shutdown();
        self.running.store(false, Ordering::SeqCst);
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Internal message for the send task.
enum SendMessage {
    Request {
        id: i64,
        method: String,
        params: Option<Value>,
        #[allow(dead_code)]
        original_id: RequestId,
    },
    Notification {
        method: String,
        params: Option<Value>,
    },
    Shutdown,
}

/// Pending request info.
struct PendingRequest {
    method: String,
    original_id: RequestId,
}

/// Runs the LSP client loop.
async fn run_client(
    config: ServerConfig,
    request_rx: Receiver<LspRequest>,
    response_tx: Sender<LspResponse>,
    notification_tx: Sender<LspNotification>,
    running: Arc<AtomicBool>,
) -> std::io::Result<()> {
    // Start the server process
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if let Some(ref dir) = config.working_dir {
        cmd.current_dir(dir);
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            log::error!("Failed to start LSP server '{}': {}", config.command, e);
            let _ = notification_tx.send(LspNotification::ServerExited { code: None });
            return Err(e);
        }
    };

    log::info!("Started LSP server: {}", config.command);

    let stdin = child.stdin.take().expect("Failed to get stdin");
    let stdout = child.stdout.take().expect("Failed to get stdout");

    let transport = AsyncTransport::new(stdin, stdout);

    // Split transport for concurrent read/write
    let (mut transport_read, mut transport_write) = transport.split();

    // Channel for sending messages to the write task
    let (send_tx, mut send_rx) = mpsc::unbounded_channel::<SendMessage>();

    // Pending requests
    let pending: Arc<tokio::sync::Mutex<HashMap<transport::RequestId, PendingRequest>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    // Next request ID
    let next_id = Arc::new(AtomicU64::new(1));

    // Spawn write task
    let write_running = running.clone();
    let write_task = tokio::spawn(async move {
        while let Some(msg) = send_rx.recv().await {
            if !write_running.load(Ordering::SeqCst) {
                break;
            }
            match msg {
                SendMessage::Request { id, method, params, .. } => {
                    if let Err(e) = transport_write.send_request(id, &method, params).await {
                        log::error!("Failed to send request: {}", e);
                    }
                }
                SendMessage::Notification { method, params } => {
                    if let Err(e) = transport_write.send_notification(&method, params).await {
                        log::error!("Failed to send notification: {}", e);
                    }
                }
                SendMessage::Shutdown => {
                    let _ = transport_write.send_request(0i64, "shutdown", None).await;
                    let _ = transport_write.send_notification("exit", None).await;
                    break;
                }
            }
        }
    });

    // Spawn read task
    let read_running = running.clone();
    let read_pending = pending.clone();
    let read_response_tx = response_tx.clone();
    let read_notification_tx = notification_tx.clone();
    let read_task = tokio::spawn(async move {
        while read_running.load(Ordering::SeqCst) {
            match transport_read.read_message().await {
                Ok(msg) => {
                    handle_server_message(
                        msg,
                        &read_pending,
                        &read_response_tx,
                        &read_notification_tx,
                    )
                    .await;
                }
                Err(e) => {
                    if read_running.load(Ordering::SeqCst) {
                        log::error!("Error reading from LSP server: {}", e);
                    }
                    break;
                }
            }
        }
    });

    // Process incoming requests from UI
    let send_tx_clone = send_tx.clone();
    let process_running = running.clone();
    let process_pending = pending.clone();
    let process_next_id = next_id.clone();
    let _process_notification_tx = notification_tx.clone();

    // Spawn request processing task
    let process_task = tokio::spawn(async move {
        while process_running.load(Ordering::SeqCst) {
            // Non-blocking receive with timeout
            match tokio::time::timeout(
                std::time::Duration::from_millis(10),
                tokio::task::spawn_blocking({
                    let request_rx = request_rx.clone();
                    move || request_rx.recv_timeout(std::time::Duration::from_millis(10))
                }),
            )
            .await
            {
                Ok(Ok(Ok(request))) => {
                    process_request(
                        request,
                        &send_tx_clone,
                        &process_pending,
                        &process_next_id,
                    )
                    .await;
                }
                Ok(Ok(Err(_))) => {
                    // Timeout, continue
                }
                Ok(Err(_)) | Err(_) => {
                    // Task error or timeout, continue
                }
            }
        }
    });

    // Wait for tasks
    let _ = tokio::join!(write_task, read_task, process_task);

    // Clean up
    running.store(false, Ordering::SeqCst);
    let exit_code = child.try_wait().ok().flatten().map(|s| s.code().unwrap_or(-1));
    let _ = notification_tx.send(LspNotification::ServerExited { code: exit_code });

    log::info!("LSP client shut down");
    Ok(())
}

/// Processes a request from the UI.
async fn process_request(
    request: LspRequest,
    send_tx: &mpsc::UnboundedSender<SendMessage>,
    pending: &Arc<tokio::sync::Mutex<HashMap<transport::RequestId, PendingRequest>>>,
    next_id: &Arc<AtomicU64>,
) {
    match request {
        LspRequest::Initialize { id, root_path } => {
            let rpc_id = next_id.fetch_add(1, Ordering::SeqCst) as i64;
            let root_uri = Some(path_to_uri(&root_path));

            let params = InitializeParams {
                process_id: Some(std::process::id()),
                root_path: Some(root_path.to_string_lossy().to_string()),
                root_uri,
                capabilities: ClientCapabilities {
                    text_document: Some(TextDocumentClientCapabilities {
                        hover: Some(HoverClientCapabilities {
                            dynamic_registration: Some(false),
                            content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                        }),
                        completion: Some(CompletionClientCapabilities {
                            dynamic_registration: Some(false),
                            completion_item: Some(CompletionItemCapability {
                                snippet_support: Some(true),
                                documentation_format: Some(vec![
                                    MarkupKind::Markdown,
                                    MarkupKind::PlainText,
                                ]),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        definition: Some(GotoCapability {
                            dynamic_registration: Some(false),
                            link_support: Some(true),
                        }),
                        references: Some(DynamicRegistrationClientCapabilities {
                            dynamic_registration: Some(false),
                        }),
                        rename: Some(RenameClientCapabilities {
                            dynamic_registration: Some(false),
                            prepare_support: Some(true),
                            ..Default::default()
                        }),
                        publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                            related_information: Some(true),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            };

            {
                let mut pending = pending.lock().await;
                pending.insert(
                    transport::RequestId::Number(rpc_id),
                    PendingRequest {
                        method: "initialize".to_string(),
                        original_id: id,
                    },
                );
            }

            let _ = send_tx.send(SendMessage::Request {
                id: rpc_id,
                method: "initialize".to_string(),
                params: Some(serde_json::to_value(params).unwrap()),
                original_id: id,
            });
        }
        LspRequest::Shutdown => {
            let _ = send_tx.send(SendMessage::Shutdown);
        }
        LspRequest::DidOpen {
            path,
            language_id,
            version,
            text,
        } => {
            let uri = path_to_uri(&path);
            let params = DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri,
                    language_id,
                    version,
                    text,
                },
            };
            let _ = send_tx.send(SendMessage::Notification {
                method: "textDocument/didOpen".to_string(),
                params: Some(serde_json::to_value(params).unwrap()),
            });
        }
        LspRequest::DidChange { path, version, text } => {
            let uri = path_to_uri(&path);
            let params = DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier { uri, version },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text,
                }],
            };
            let _ = send_tx.send(SendMessage::Notification {
                method: "textDocument/didChange".to_string(),
                params: Some(serde_json::to_value(params).unwrap()),
            });
        }
        LspRequest::DidSave { path } => {
            let uri = path_to_uri(&path);
            let params = DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier { uri },
                text: None,
            };
            let _ = send_tx.send(SendMessage::Notification {
                method: "textDocument/didSave".to_string(),
                params: Some(serde_json::to_value(params).unwrap()),
            });
        }
        LspRequest::DidClose { path } => {
            let uri = path_to_uri(&path);
            let params = DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri },
            };
            let _ = send_tx.send(SendMessage::Notification {
                method: "textDocument/didClose".to_string(),
                params: Some(serde_json::to_value(params).unwrap()),
            });
        }
        LspRequest::Hover { id, path, position } => {
            send_text_document_request(
                "textDocument/hover",
                id,
                path,
                position,
                send_tx,
                pending,
                next_id,
            )
            .await;
        }
        LspRequest::Completion { id, path, position } => {
            send_text_document_request(
                "textDocument/completion",
                id,
                path,
                position,
                send_tx,
                pending,
                next_id,
            )
            .await;
        }
        LspRequest::GotoDefinition { id, path, position } => {
            send_text_document_request(
                "textDocument/definition",
                id,
                path,
                position,
                send_tx,
                pending,
                next_id,
            )
            .await;
        }
        LspRequest::FindReferences {
            id,
            path,
            position,
            include_declaration,
        } => {
            let rpc_id = next_id.fetch_add(1, Ordering::SeqCst) as i64;
            let uri = path_to_uri(&path);
            let params = ReferenceParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: position.into(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: ReferenceContext {
                    include_declaration,
                },
            };

            {
                let mut pending = pending.lock().await;
                pending.insert(
                    transport::RequestId::Number(rpc_id),
                    PendingRequest {
                        method: "textDocument/references".to_string(),
                        original_id: id,
                    },
                );
            }

            let _ = send_tx.send(SendMessage::Request {
                id: rpc_id,
                method: "textDocument/references".to_string(),
                params: Some(serde_json::to_value(params).unwrap()),
                original_id: id,
            });
        }
        LspRequest::Rename {
            id,
            path,
            position,
            new_name,
        } => {
            let rpc_id = next_id.fetch_add(1, Ordering::SeqCst) as i64;
            let uri = path_to_uri(&path);
            let params = RenameParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: position.into(),
                },
                new_name,
                work_done_progress_params: Default::default(),
            };

            {
                let mut pending = pending.lock().await;
                pending.insert(
                    transport::RequestId::Number(rpc_id),
                    PendingRequest {
                        method: "textDocument/rename".to_string(),
                        original_id: id,
                    },
                );
            }

            let _ = send_tx.send(SendMessage::Request {
                id: rpc_id,
                method: "textDocument/rename".to_string(),
                params: Some(serde_json::to_value(params).unwrap()),
                original_id: id,
            });
        }
        LspRequest::DocumentSymbols { id, path } => {
            let rpc_id = next_id.fetch_add(1, Ordering::SeqCst) as i64;
            let uri = path_to_uri(&path);
            let params = DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };

            {
                let mut pending = pending.lock().await;
                pending.insert(
                    transport::RequestId::Number(rpc_id),
                    PendingRequest {
                        method: "textDocument/documentSymbol".to_string(),
                        original_id: id,
                    },
                );
            }

            let _ = send_tx.send(SendMessage::Request {
                id: rpc_id,
                method: "textDocument/documentSymbol".to_string(),
                params: Some(serde_json::to_value(params).unwrap()),
                original_id: id,
            });
        }
    }
}

/// Helper to send a text document position request.
async fn send_text_document_request(
    method: &str,
    original_id: RequestId,
    path: PathBuf,
    position: Position,
    send_tx: &mpsc::UnboundedSender<SendMessage>,
    pending: &Arc<tokio::sync::Mutex<HashMap<transport::RequestId, PendingRequest>>>,
    next_id: &Arc<AtomicU64>,
) {
    let rpc_id = next_id.fetch_add(1, Ordering::SeqCst) as i64;
    let uri = path_to_uri(&path);
    let params = TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri },
        position: position.into(),
    };

    {
        let mut pending = pending.lock().await;
        pending.insert(
            transport::RequestId::Number(rpc_id),
            PendingRequest {
                method: method.to_string(),
                original_id,
            },
        );
    }

    let _ = send_tx.send(SendMessage::Request {
        id: rpc_id,
        method: method.to_string(),
        params: Some(serde_json::to_value(params).unwrap()),
        original_id,
    });
}

/// Handles a message from the server.
async fn handle_server_message(
    msg: Value,
    pending: &Arc<tokio::sync::Mutex<HashMap<transport::RequestId, PendingRequest>>>,
    response_tx: &Sender<LspResponse>,
    notification_tx: &Sender<LspNotification>,
) {
    if let Some(parsed) = transport::parse_message(&msg) {
        match parsed {
            JsonRpcMessage::Response(resp) => {
                handle_response(resp, pending, response_tx).await;
            }
            JsonRpcMessage::Notification(notif) => {
                handle_notification(notif, notification_tx);
            }
            JsonRpcMessage::Request(req) => {
                // Server-initiated requests (like workspace/configuration)
                log::debug!("Server request: {} (id: {:?})", req.method, req.id);
            }
        }
    }
}

/// Handles a response from the server.
async fn handle_response(
    resp: JsonRpcResponse,
    pending: &Arc<tokio::sync::Mutex<HashMap<transport::RequestId, PendingRequest>>>,
    response_tx: &Sender<LspResponse>,
) {
    let pending_req = {
        let mut pending = pending.lock().await;
        pending.remove(&resp.id)
    };

    let Some(req_info) = pending_req else {
        log::warn!("Received response for unknown request: {:?}", resp.id);
        return;
    };

    let response = if let Some(error) = resp.error {
        LspResponse::Error {
            id: req_info.original_id,
            message: error.message,
        }
    } else {
        match req_info.method.as_str() {
            "initialize" => {
                let caps: InitializeResult = match resp.result {
                    Some(v) => serde_json::from_value(v).unwrap_or_default(),
                    None => InitializeResult::default(),
                };
                LspResponse::Initialized {
                    id: req_info.original_id,
                    capabilities_summary: format_capabilities(&caps.capabilities),
                }
            }
            "textDocument/hover" => {
                let hover: Option<Hover> = resp
                    .result
                    .and_then(|v| serde_json::from_value(v).ok());
                LspResponse::Hover {
                    id: req_info.original_id,
                    info: hover.map(convert_hover),
                }
            }
            "textDocument/completion" => {
                let items = parse_completion_response(resp.result);
                LspResponse::Completion {
                    id: req_info.original_id,
                    items,
                }
            }
            "textDocument/definition" => {
                let locations = parse_location_response(resp.result);
                LspResponse::GotoDefinition {
                    id: req_info.original_id,
                    locations,
                }
            }
            "textDocument/references" => {
                let locations = parse_location_response(resp.result);
                LspResponse::References {
                    id: req_info.original_id,
                    locations,
                }
            }
            "textDocument/rename" => {
                let edit = resp
                    .result
                    .and_then(|v| serde_json::from_value::<lsp_types::WorkspaceEdit>(v).ok())
                    .map(convert_workspace_edit);
                LspResponse::Rename {
                    id: req_info.original_id,
                    edit,
                }
            }
            "textDocument/documentSymbol" => {
                let symbols = parse_document_symbols(resp.result);
                LspResponse::DocumentSymbols {
                    id: req_info.original_id,
                    symbols,
                }
            }
            _ => {
                log::debug!("Unhandled response method: {}", req_info.method);
                return;
            }
        }
    };

    let _ = response_tx.send(response);
}

/// Handles a notification from the server.
fn handle_notification(notif: JsonRpcNotification, notification_tx: &Sender<LspNotification>) {
    match notif.method.as_str() {
        "textDocument/publishDiagnostics" => {
            if let Some(params) = notif.params {
                if let Ok(diag_params) =
                    serde_json::from_value::<PublishDiagnosticsParams>(params)
                {
                    let path = uri_to_path(&diag_params.uri);
                    let diagnostics: Vec<Diagnostic> = diag_params
                        .diagnostics
                        .into_iter()
                        .map(|d| d.into())
                        .collect();
                    let _ = notification_tx.send(LspNotification::Diagnostics { path, diagnostics });
                }
            }
        }
        "$/progress" => {
            if let Some(params) = notif.params {
                if let Ok(progress) = serde_json::from_value::<ProgressParams>(params) {
                    let token = match progress.token {
                        NumberOrString::Number(n) => n.to_string(),
                        NumberOrString::String(s) => s,
                    };

                    let (message, percentage) = match progress.value {
                        ProgressParamsValue::WorkDone(work_done) => match work_done {
                            WorkDoneProgress::Begin(b) => (b.message, b.percentage),
                            WorkDoneProgress::Report(r) => (r.message, r.percentage),
                            WorkDoneProgress::End(e) => (e.message, None),
                        },
                    };

                    let _ = notification_tx.send(LspNotification::Progress {
                        token,
                        message,
                        percentage,
                    });
                }
            }
        }
        "window/logMessage" => {
            if let Some(params) = notif.params {
                if let Ok(log_params) = serde_json::from_value::<LogMessageParams>(params) {
                    let level = match log_params.typ {
                        MessageType::ERROR => LogLevel::Error,
                        MessageType::WARNING => LogLevel::Warning,
                        MessageType::INFO => LogLevel::Info,
                        MessageType::LOG => LogLevel::Log,
                        _ => LogLevel::Log,
                    };
                    let _ = notification_tx.send(LspNotification::LogMessage {
                        level,
                        message: log_params.message,
                    });
                }
            }
        }
        "initialized" => {
            // Server acknowledges initialization
            let _ = notification_tx.send(LspNotification::ServerReady);
        }
        _ => {
            log::trace!("Unhandled notification: {}", notif.method);
        }
    }
}

/// Formats server capabilities as a summary string.
fn format_capabilities(caps: &ServerCapabilities) -> String {
    let mut features = Vec::new();

    if caps.hover_provider.is_some() {
        features.push("hover");
    }
    if caps.completion_provider.is_some() {
        features.push("completion");
    }
    if caps.definition_provider.is_some() {
        features.push("definition");
    }
    if caps.references_provider.is_some() {
        features.push("references");
    }
    if caps.rename_provider.is_some() {
        features.push("rename");
    }
    if caps.document_symbol_provider.is_some() {
        features.push("symbols");
    }

    features.join(", ")
}

/// Converts LSP hover to our type.
fn convert_hover(hover: Hover) -> HoverInfo {
    let contents = match hover.contents {
        HoverContents::Scalar(marked) => match marked {
            MarkedString::String(s) => s,
            MarkedString::LanguageString(ls) => format!("```{}\n{}\n```", ls.language, ls.value),
        },
        HoverContents::Array(arr) => arr
            .into_iter()
            .map(|m| match m {
                MarkedString::String(s) => s,
                MarkedString::LanguageString(ls) => {
                    format!("```{}\n{}\n```", ls.language, ls.value)
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
        HoverContents::Markup(markup) => markup.value,
    };

    HoverInfo {
        contents,
        range: hover.range.map(|r| r.into()),
    }
}

/// Parses completion response.
fn parse_completion_response(result: Option<Value>) -> Vec<CompletionItem> {
    let Some(value) = result else {
        return vec![];
    };

    // Try as CompletionList first
    if let Ok(list) = serde_json::from_value::<CompletionList>(value.clone()) {
        return list.items.into_iter().map(|i| i.into()).collect();
    }

    // Try as Vec<CompletionItem>
    if let Ok(items) = serde_json::from_value::<Vec<lsp_types::CompletionItem>>(value) {
        return items.into_iter().map(|i| i.into()).collect();
    }

    vec![]
}

/// Parses location response (definition, references).
fn parse_location_response(result: Option<Value>) -> Vec<Location> {
    let Some(value) = result else {
        return vec![];
    };

    // Try as GotoDefinitionResponse
    if let Ok(resp) = serde_json::from_value::<GotoDefinitionResponse>(value.clone()) {
        return match resp {
            GotoDefinitionResponse::Scalar(loc) => vec![convert_location(loc)],
            GotoDefinitionResponse::Array(locs) => {
                locs.into_iter().map(convert_location).collect()
            }
            GotoDefinitionResponse::Link(links) => links
                .into_iter()
                .map(|l| Location {
                    path: uri_to_path(&l.target_uri),
                    range: l.target_selection_range.into(),
                })
                .collect(),
        };
    }

    // Try as Vec<Location>
    if let Ok(locs) = serde_json::from_value::<Vec<lsp_types::Location>>(value) {
        return locs.into_iter().map(convert_location).collect();
    }

    vec![]
}

/// Converts LSP location to our type.
fn convert_location(loc: lsp_types::Location) -> Location {
    Location {
        path: uri_to_path(&loc.uri),
        range: loc.range.into(),
    }
}

/// Converts LSP workspace edit to our type.
fn convert_workspace_edit(edit: lsp_types::WorkspaceEdit) -> WorkspaceEdit {
    let mut changes = Vec::new();

    if let Some(edits) = edit.changes {
        for (uri, text_edits) in edits {
            let path = uri_to_path(&uri);
            let edits: Vec<TextEdit> = text_edits.into_iter().map(|e| e.into()).collect();
            changes.push((path, edits));
        }
    }

    WorkspaceEdit { changes }
}

/// Parses document symbols response.
fn parse_document_symbols(result: Option<Value>) -> Vec<DocumentSymbol> {
    let Some(value) = result else {
        return vec![];
    };

    // Try as DocumentSymbolResponse
    if let Ok(resp) = serde_json::from_value::<DocumentSymbolResponse>(value.clone()) {
        return match resp {
            DocumentSymbolResponse::Flat(symbols) => symbols
                .into_iter()
                .map(|s| DocumentSymbol {
                    name: s.name,
                    kind: s.kind.into(),
                    range: s.location.range.into(),
                    selection_range: s.location.range.into(),
                    children: vec![],
                })
                .collect(),
            DocumentSymbolResponse::Nested(symbols) => {
                symbols.into_iter().map(convert_document_symbol).collect()
            }
        };
    }

    vec![]
}

/// Converts LSP document symbol to our type.
fn convert_document_symbol(sym: lsp_types::DocumentSymbol) -> DocumentSymbol {
    DocumentSymbol {
        name: sym.name,
        kind: sym.kind.into(),
        range: sym.range.into(),
        selection_range: sym.selection_range.into(),
        children: sym
            .children
            .unwrap_or_default()
            .into_iter()
            .map(convert_document_symbol)
            .collect(),
    }
}
