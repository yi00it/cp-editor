//! JSON-RPC transport layer for LSP communication.
//!
//! Handles reading and writing LSP messages over stdin/stdout of the language server.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStdin as AsyncChildStdin, ChildStdout as AsyncChildStdout};

/// JSON-RPC message types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
}

/// JSON-RPC request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC notification (no id, no response expected).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Request ID (can be number or string).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

impl From<u64> for RequestId {
    fn from(id: u64) -> Self {
        RequestId::Number(id as i64)
    }
}

impl From<i64> for RequestId {
    fn from(id: i64) -> Self {
        RequestId::Number(id)
    }
}

/// Async transport for LSP communication.
pub struct AsyncTransport {
    stdin: AsyncChildStdin,
    stdout: tokio::io::BufReader<AsyncChildStdout>,
}

impl AsyncTransport {
    /// Creates a new async transport.
    pub fn new(stdin: AsyncChildStdin, stdout: AsyncChildStdout) -> Self {
        Self {
            stdin,
            stdout: tokio::io::BufReader::new(stdout),
        }
    }

    /// Splits the transport into separate read and write halves.
    pub fn split(self) -> (TransportReader, TransportWriter) {
        (
            TransportReader { stdout: self.stdout },
            TransportWriter { stdin: self.stdin },
        )
    }
}

/// Write half of the transport.
pub struct TransportWriter {
    stdin: AsyncChildStdin,
}

impl TransportWriter {
    /// Sends a JSON-RPC request.
    pub async fn send_request(
        &mut self,
        id: impl Into<RequestId>,
        method: &str,
        params: Option<Value>,
    ) -> std::io::Result<()> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            method: method.to_string(),
            params,
        };
        self.send_message(&serde_json::to_value(request)?).await
    }

    /// Sends a JSON-RPC notification.
    pub async fn send_notification(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> std::io::Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };
        self.send_message(&serde_json::to_value(notification)?).await
    }

    /// Sends a raw JSON-RPC message.
    async fn send_message(&mut self, message: &Value) -> std::io::Result<()> {
        let content = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(content.as_bytes()).await?;
        self.stdin.flush().await?;

        log::trace!("Sent: {}", content);
        Ok(())
    }
}

/// Read half of the transport.
pub struct TransportReader {
    stdout: tokio::io::BufReader<AsyncChildStdout>,
}

impl TransportReader {
    /// Reads the next JSON-RPC message.
    pub async fn read_message(&mut self) -> std::io::Result<Value> {
        // Read headers
        let mut content_length: Option<usize> = None;
        let mut header_line = String::new();

        loop {
            header_line.clear();
            let bytes_read = self.stdout.read_line(&mut header_line).await?;
            if bytes_read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Server closed connection",
                ));
            }

            let line = header_line.trim();
            if line.is_empty() {
                break;
            }

            if let Some(len_str) = line.strip_prefix("Content-Length: ") {
                content_length = Some(len_str.parse().map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid Content-Length")
                })?);
            }
        }

        let content_length = content_length.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing Content-Length header")
        })?;

        // Read content
        let mut content = vec![0u8; content_length];
        self.stdout.read_exact(&mut content).await?;

        let content_str = String::from_utf8(content).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8 in message")
        })?;

        log::trace!("Received: {}", content_str);

        serde_json::from_str(&content_str).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Invalid JSON: {}", e))
        })
    }
}

/// Parses a JSON-RPC message to determine its type.
pub fn parse_message(value: &Value) -> Option<JsonRpcMessage> {
    // Check if it's a response (has id and result/error but no method)
    if value.get("id").is_some() && value.get("method").is_none() {
        return serde_json::from_value(value.clone())
            .ok()
            .map(JsonRpcMessage::Response);
    }

    // Check if it's a request (has id and method)
    if value.get("id").is_some() && value.get("method").is_some() {
        return serde_json::from_value(value.clone())
            .ok()
            .map(JsonRpcMessage::Request);
    }

    // Check if it's a notification (has method but no id)
    if value.get("method").is_some() && value.get("id").is_none() {
        return serde_json::from_value(value.clone())
            .ok()
            .map(JsonRpcMessage::Notification);
    }

    None
}
