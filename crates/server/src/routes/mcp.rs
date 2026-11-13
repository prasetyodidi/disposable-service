use std::convert::Infallible;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use tokio_stream::Stream;
use uuid::Uuid;

use crate::auth::AuthenticatedSession;
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct McpQuery {
    #[serde(rename = "sessionId", default)]
    pub session_id: Option<String>,
}

/// Result of a `tools/call` dispatch.
///
/// Legacy tools report failures inside `result.isError` (`Content`), while the
/// new privacy tools can return a JSON-RPC `error` object (`RpcError`).
pub(crate) enum ToolOutcome {
    /// Tool content items and the `isError` flag.
    Content(Vec<serde_json::Value>, bool),
    /// JSON-RPC error object (`error.code` / `error.message`).
    RpcError { code: i32, message: String },
}

/// JSON-RPC code for invalid arguments.
pub(crate) const RPC_INVALID_PARAMS: i32 = -32602;
/// JSON-RPC code for internal failures (e.g. burn failure).
pub(crate) const RPC_INTERNAL_ERROR: i32 = -32603;

impl ToolOutcome {
    /// -32602 error whose message names the offending argument.
    pub(crate) fn invalid_arg(arg: &str, detail: &str) -> Self {
        ToolOutcome::RpcError {
            code: RPC_INVALID_PARAMS,
            message: format!("Invalid params: argument '{}' {}", arg, detail),
        }
    }

    /// -32603 internal error.
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        ToolOutcome::RpcError {
            code: RPC_INTERNAL_ERROR,
            message: message.into(),
        }
    }

    /// Successful single-text result.
    #[allow(dead_code)]
    pub(crate) fn text(text: String) -> Self {
        ToolOutcome::Content(vec![serde_json::json!({ "type": "text", "text": text })], false)
    }
}

// --- Dispatch hooks for new tools (bodies filled in by tasks 8.2 / 8.3) ---

/// Max characters for `redact_secrets.text`.
const MCP_MAX_TEXT_CHARS: usize = 1_048_576;
/// Max decoded bytes for `scrub_metadata.file_base64`.
const MCP_MAX_FILE_BYTES: usize = 26_214_400;
/// TTL (seconds) of the scrubbed file download.
const MCP_SCRUB_TTL_SECS: u64 = 300;

/// Arguments must be a JSON object (a missing `arguments` is treated as empty).
fn args_object(
    args: Option<&serde_json::Value>,
) -> Result<serde_json::Map<String, serde_json::Value>, ToolOutcome> {
    match args {
        None | Some(serde_json::Value::Null) => Ok(serde_json::Map::new()),
        Some(serde_json::Value::Object(m)) => Ok(m.clone()),
        Some(_) => Err(ToolOutcome::invalid_arg("arguments", "must be an object")),
    }
}

/// Required string argument.
fn req_str<'a>(
    map: &'a serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Result<&'a str, ToolOutcome> {
    match map.get(name) {
        None | Some(serde_json::Value::Null) => Err(ToolOutcome::invalid_arg(name, "is required")),
        Some(serde_json::Value::String(s)) => Ok(s.as_str()),
        Some(_) => Err(ToolOutcome::invalid_arg(name, "must be a string")),
    }
}

/// Optional string argument.
fn opt_str<'a>(
    map: &'a serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Result<Option<&'a str>, ToolOutcome> {
    match map.get(name) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(ToolOutcome::invalid_arg(name, "must be a string")),
    }
}

/// Optional array of strings.
fn opt_str_array(
    map: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Result<Option<Vec<String>>, ToolOutcome> {
    match map.get(name) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .map(Some)
            .ok_or_else(|| ToolOutcome::invalid_arg(name, "must be an array of strings")),
        Some(_) => Err(ToolOutcome::invalid_arg(name, "must be an array of strings")),
    }
}

/// Task 8.2: validate args, then call the shared redactor core.
async fn call_redact_secrets(_state: &AppState, args: Option<&serde_json::Value>) -> ToolOutcome {
    use crate::privacy::redactor::{redact, validate, Category, Format, RedactOptions};
    use std::collections::BTreeSet;

    let map = match args_object(args) {
        Ok(m) => m,
        Err(e) => return e,
    };
    let text = match req_str(&map, "text") {
        Ok(t) => t,
        Err(e) => return e,
    };
    let chars = text.chars().count();
    if chars == 0 || chars > MCP_MAX_TEXT_CHARS {
        return ToolOutcome::invalid_arg(
            "text",
            &format!("must be between 1 and {} characters", MCP_MAX_TEXT_CHARS),
        );
    }

    let categories: BTreeSet<Category> = match opt_str_array(&map, "categories") {
        Err(e) => return e,
        Ok(None) => Category::ALL.iter().copied().collect(),
        Ok(Some(list)) => {
            let mut set = BTreeSet::new();
            for name in &list {
                match Category::parse(name) {
                    Some(c) => {
                        set.insert(c);
                    }
                    None => {
                        return ToolOutcome::invalid_arg("categories", "contains an unknown category")
                    }
                }
            }
            set
        }
    };
    let custom_words = match opt_str_array(&map, "custom_words") {
        Ok(w) => w.unwrap_or_default(),
        Err(e) => return e,
    };
    let format = match opt_str(&map, "format") {
        Err(e) => return e,
        Ok(None) => Format::Auto,
        Ok(Some(f)) => match Format::parse(f) {
            Some(f) => f,
            None => {
                return ToolOutcome::invalid_arg("format", "must be one of: auto, env, text")
            }
        },
    };

    let opts = RedactOptions {
        categories,
        custom_words,
        format,
    };
    if let Err(e) = validate(&opts) {
        return ToolOutcome::invalid_arg("custom_words", &e.to_string());
    }

    let outcome = redact(text, &opts);
    let response = disposable_common::RedactResponse {
        text: outcome.text,
        counts: outcome
            .counts
            .into_iter()
            .map(|(c, n)| (c.as_str().to_string(), n))
            .collect(),
    };
    match serde_json::to_string(&response) {
        Ok(s) => ToolOutcome::text(s),
        Err(e) => ToolOutcome::internal(format!("Failed to serialize result: {}", e)),
    }
}

/// Task 8.2: validate args, then call the shared JWT core.
async fn call_inspect_jwt(_state: &AppState, args: Option<&serde_json::Value>) -> ToolOutcome {
    use crate::privacy::jwt_inspector::{inspect, InspectOptions, JwtError, MAX_TOKEN_CHARS};
    use zeroize::Zeroizing;

    let map = match args_object(args) {
        Ok(m) => m,
        Err(e) => return e,
    };
    let token = match req_str(&map, "token") {
        Ok(t) => Zeroizing::new(t.to_string()),
        Err(e) => return e,
    };
    let chars = token.chars().count();
    if chars == 0 || chars > MAX_TOKEN_CHARS {
        return ToolOutcome::invalid_arg(
            "token",
            &format!("must be between 1 and {} characters", MAX_TOKEN_CHARS),
        );
    }
    let secret = match opt_str(&map, "secret") {
        Ok(s) => s.map(|s| Zeroizing::new(s.to_string())),
        Err(e) => return e,
    };
    let redact = match map.get("redact") {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(_) => return ToolOutcome::invalid_arg("redact", "must be a boolean"),
    };

    let now_unix = chrono::Utc::now().timestamp();
    let result = {
        let opts = InspectOptions {
            hmac_secret: secret.as_ref().map(|s| s.as_bytes()),
            redact,
        };
        inspect(token.as_str(), &opts, now_unix)
    };
    drop(token);
    drop(secret);

    match result {
        Ok(r) => match serde_json::to_string(&r) {
            Ok(s) => ToolOutcome::text(s),
            Err(e) => ToolOutcome::internal(format!("Failed to serialize result: {}", e)),
        },
        Err(JwtError::SecretLength) => ToolOutcome::invalid_arg("secret", &JwtError::SecretLength.to_string()),
        Err(e) => ToolOutcome::Content(
            vec![serde_json::json!({ "type": "text", "text": e.to_string() })],
            true,
        ),
    }
}

/// Task 8.2: validate args, decode base64, scrub, store as ephemeral download.
async fn call_scrub_metadata(state: &AppState, args: Option<&serde_json::Value>) -> ToolOutcome {
    use crate::privacy::scrubber::{scrub, FileKind, ScrubError};
    use base64::prelude::BASE64_STANDARD;
    use base64::Engine;

    let map = match args_object(args) {
        Ok(m) => m,
        Err(e) => return e,
    };
    let b64 = match req_str(&map, "file_base64") {
        Ok(s) => s,
        Err(e) => return e,
    };
    let filename_arg = match opt_str(&map, "filename") {
        Ok(f) => f,
        Err(e) => return e,
    };

    // Reject oversize input before allocating the decoded buffer.
    let max_encoded = MCP_MAX_FILE_BYTES.div_ceil(3) * 4;
    let b64 = b64.trim();
    if b64.len() > max_encoded {
        return ToolOutcome::invalid_arg(
            "file_base64",
            &format!("decoded size must be between 1 and {} bytes", MCP_MAX_FILE_BYTES),
        );
    }
    let bytes = match BASE64_STANDARD.decode(b64) {
        Ok(b) => b,
        Err(_) => return ToolOutcome::invalid_arg("file_base64", "is not valid base64"),
    };
    if bytes.is_empty() || bytes.len() > MCP_MAX_FILE_BYTES {
        return ToolOutcome::invalid_arg(
            "file_base64",
            &format!("decoded size must be between 1 and {} bytes", MCP_MAX_FILE_BYTES),
        );
    }

    let out = match scrub(&bytes) {
        Ok(o) => o,
        Err(e) => {
            let msg = match e {
                ScrubError::TooLarge => "File too large".to_string(),
                ScrubError::UnsupportedType => {
                    "Unsupported file type (only JPEG, PNG and PDF)".to_string()
                }
                ScrubError::Unreadable(reason) => format!("File cannot be read: {}", reason),
                ScrubError::EncryptedPdf => "Encrypted PDF is not supported".to_string(),
            };
            return ToolOutcome::Content(
                vec![serde_json::json!({ "type": "text", "text": msg })],
                true,
            );
        }
    };

    let (ext, content_type) = match out.kind {
        FileKind::Jpeg => ("jpg", "image/jpeg"),
        FileKind::Png => ("png", "image/png"),
        FileKind::Pdf => ("pdf", "application/pdf"),
    };
    // Keep only a safe basename; fall back to a generic name.
    let cleaned: String = filename_arg
        .map(|f| {
            f.rsplit(['/', '\\'])
                .next()
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_control() && *c != '"')
                .take(128)
                .collect::<String>()
        })
        .unwrap_or_default();
    let filename = if cleaned.trim().is_empty() {
        format!("scrubbed.{}", ext)
    } else {
        cleaned
    };

    let token = state
        .store_ephemeral_download(
            out.bytes,
            filename.clone(),
            content_type.to_string(),
            MCP_SCRUB_TTL_SECS,
        )
        .await;
    let response = disposable_common::ScrubEphemeralResponse {
        download_url: format!("/api/apps/scrubber/download/{}", token),
        token,
        filename,
        report: out.report,
        expires_in_secs: MCP_SCRUB_TTL_SECS,
    };
    match serde_json::to_string(&response) {
        Ok(s) => ToolOutcome::text(s),
        Err(e) => ToolOutcome::internal(format!("Failed to serialize result: {}", e)),
    }
}

/// Task 8.3: require `confirm == true`, then run the burn coordinator.
///
/// On success returns a [`BurnDelivery`] holding a clone of the caller's SSE
/// `Sender` (taken BEFORE the burn, because the burn clears `mcp_sessions`).
/// The caller must deliver the result and then call [`BurnDelivery::finish`].
async fn call_burn_workspace_now(
    state: &AppState,
    args: Option<&serde_json::Value>,
    session_id: Option<&str>,
) -> (ToolOutcome, Option<BurnDelivery>) {
    let map = match args_object(args) {
        Ok(m) => m,
        Err(e) => return (e, None),
    };
    match map.get("confirm") {
        Some(serde_json::Value::Bool(true)) => {}
        _ => {
            return (
                ToolOutcome::RpcError {
                    code: RPC_INVALID_PARAMS,
                    message: "Invalid params: argument 'confirm' must be boolean true to confirm the permanent burn"
                        .to_string(),
                },
                None,
            )
        }
    }

    // No await between this check and the `run_or_join` call, so the check
    // and the start of the burn cannot interleave with another caller.
    if state.burn.is_burning() {
        return (
            ToolOutcome::RpcError {
                code: RPC_INTERNAL_ERROR,
                message: "penghancuran sedang berlangsung".to_string(),
            },
            None,
        );
    }

    let sender = match session_id {
        Some(sid) => state.mcp_sessions.read().await.get(sid).cloned(),
        None => None,
    };
    // `is_burning` was checked above; if the read lock yielded and another
    // burn started meanwhile, report it instead of joining silently.
    if state.burn.is_burning() {
        return (
            ToolOutcome::RpcError {
                code: RPC_INTERNAL_ERROR,
                message: "penghancuran sedang berlangsung".to_string(),
            },
            None,
        );
    }

    // Run in a detached task so a client disconnect (which drops this
    // future) cannot cancel the wipe midway.
    let task_state = state.clone();
    let report = match tokio::spawn(async move { task_state.burn.run_or_join(&task_state).await }).await {
        Ok(r) => r,
        Err(e) => {
            return (ToolOutcome::internal(format!("Penghancuran terhenti: {}", e)), None);
        }
    };
    if !report.verified || report.incomplete {
        let steps: Vec<String> = report
            .failed_items
            .iter()
            .map(|f| format!("{}: {}", f.category, f.reason))
            .collect();
        let message = if steps.is_empty() {
            "Penghancuran gagal atau tidak selesai".to_string()
        } else {
            format!("Penghancuran gagal atau tidak selesai; langkah belum selesai: {}", steps.join("; "))
        };
        return (ToolOutcome::internal(message), None);
    }

    let outcome = match serde_json::to_string(&*report) {
        Ok(s) => ToolOutcome::text(s),
        Err(e) => return (ToolOutcome::internal(format!("Failed to serialize result: {}", e)), None),
    };
    (outcome, Some(BurnDelivery { sender }))
}

/// Pending delivery of a successful burn result.
struct BurnDelivery {
    /// Clone of the calling SSE session's sender (None for plain HTTP calls).
    sender: Option<tokio::sync::mpsc::Sender<Event>>,
}

/// Max wait for the SSE event to be delivered before shutdown is scheduled.
const BURN_SSE_DELIVERY_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

impl BurnDelivery {
    /// SSE path: send the full result event through the cloned sender, drop it
    /// so the stream ends after the event, wait (max 5s) for delivery, then
    /// schedule shutdown. Runs in the background; never blocks the response.
    fn finish(self, state: AppState, event: Option<Event>) {
        tokio::spawn(async move {
            if let (Some(tx), Some(event)) = (self.sender, event) {
                let wait = async {
                    if tx.send(event).await.is_err() {
                        return; // receiver already gone
                    }
                    // Event is delivered once the stream has consumed the queue,
                    // or the client disconnected (`closed`).
                    let drained = async {
                        while tx.capacity() < tx.max_capacity() {
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        }
                    };
                    tokio::select! {
                        _ = drained => {}
                        _ = tx.closed() => {}
                    }
                };
                let _ = tokio::time::timeout(BURN_SSE_DELIVERY_WAIT, wait).await;
                // tx dropped here: stream ends after the delivered event.
            }
            state.burn.schedule_shutdown(async {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            });
        });
    }
}

/// SSE transport endpoint for MCP
pub async fn mcp_sse_handler(
    _auth: AuthenticatedSession,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let session_id = Uuid::new_v4().to_string();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Event>(32);

    state.add_mcp_session(session_id.clone(), tx.clone()).await;

    // Send initial endpoint event
    let endpoint_event = Event::default()
        .event("endpoint")
        .data(format!("/api/mcp/message?sessionId={}", session_id));
    let _ = tx.send(endpoint_event).await;

    let cleanup_session_id = session_id.clone();
    let cleanup_state = state.clone();

    let stream = async_stream::stream! {
        struct DropGuard<F: FnOnce()> {
            f: Option<F>,
        }
        impl<F: FnOnce()> Drop for DropGuard<F> {
            fn drop(&mut self) {
                if let Some(f) = self.f.take() {
                    f();
                }
            }
        }

        let _guard = DropGuard {
            f: Some(move || {
                tokio::spawn(async move {
                    cleanup_state.remove_mcp_session(&cleanup_session_id).await;
                });
            }),
        };

        while let Some(event) = rx.recv().await {
            yield Ok::<Event, Infallible>(event);
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// JSON-RPC 2.0 message handler for MCP
pub async fn mcp_message_handler(
    _auth: AuthenticatedSession,
    State(state): State<AppState>,
    Query(params): Query<McpQuery>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Response, (StatusCode, String)> {
    let method = payload
        .get("method")
        .and_then(|m| m.as_str())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Missing or invalid 'method'".to_string()))?;

    let id = payload.get("id").cloned();

    // 1. Notifications: no response returned over SSE
    if method == "notifications/initialized" {
        return Ok((StatusCode::OK, "OK").into_response());
    }

    // 2. Methods
    let mut burn_delivery: Option<BurnDelivery> = None;
    let rpc_response = match method {
        "initialize" => {
            let result = serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "disposable-service",
                    "version": "1.2.0"
                }
            });
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            })
        }
        "ping" => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            })
        }
        "tools/list" => {
            let result = serde_json::json!({
                "tools": [
                    {
                        "name": "convert_markdown_to_pdf",
                        "description": "Convert Markdown text to PDF using Pandoc with LaTeX formatting. Returns a one-time ephemeral download link.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "markdown": {
                                    "type": "string",
                                    "description": "Markdown text to convert to PDF"
                                },
                                "filename": {
                                    "type": "string",
                                    "description": "Optional output filename for the PDF (default: document.pdf)"
                                }
                            },
                            "required": ["markdown"]
                        }
                    },
                    {
                        "name": "get_host_metrics",
                        "description": "Get real-time read-only host telemetry snapshot including CPU cores, RAM, swap, disks, and top processes.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "redact_secrets",
                        "description": "Mask secrets (API keys, tokens, passwords, emails, IPs, etc.) in text, .env or log content. Processed in memory only; returns the redacted text and a report of categories and counts.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "text": {
                                    "type": "string",
                                    "minLength": 1,
                                    "maxLength": 1048576,
                                    "description": "Text to redact (1 to 1,048,576 characters)"
                                },
                                "categories": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Optional list of secret categories to redact (default: all)"
                                },
                                "custom_words": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Optional extra words or phrases to redact"
                                },
                                "format": {
                                    "type": "string",
                                    "description": "Optional input format hint (e.g. text, env, log)"
                                }
                            },
                            "required": ["text"]
                        }
                    },
                    {
                        "name": "inspect_jwt",
                        "description": "Decode a JWT locally without sending it to any third party. Returns header, payload, timestamps, status, warnings and optional HMAC verification.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "token": {
                                    "type": "string",
                                    "minLength": 1,
                                    "maxLength": 8192,
                                    "description": "JWT to inspect (1 to 8,192 characters)"
                                },
                                "secret": {
                                    "type": "string",
                                    "description": "Optional HMAC secret used to verify the signature"
                                },
                                "redact": {
                                    "type": "boolean",
                                    "description": "Optionally redact sensitive claims in the output"
                                }
                            },
                            "required": ["token"]
                        }
                    },
                    {
                        "name": "scrub_metadata",
                        "description": "Strip sensitive metadata (EXIF, XMP, document info, etc.) from a JPEG, PNG or PDF file, in memory. Returns a one-time ephemeral download link (5 minutes) and a scrub report.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "file_base64": {
                                    "type": "string",
                                    "description": "Base64-encoded file content (decoded size 1 to 26,214,400 bytes)"
                                },
                                "filename": {
                                    "type": "string",
                                    "description": "Optional output filename"
                                }
                            },
                            "required": ["file_base64"]
                        }
                    },
                    {
                        "name": "burn_workspace_now",
                        "title": "Burn Workspace Now",
                        "description": "PERMANENT and IRREVERSIBLE: wipes all sessions, ephemeral downloads, MCP sessions and temporary files from this server, returns a zero-byte audit report, then shuts the server process down. This cannot be undone.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "confirm": {
                                    "type": "boolean",
                                    "const": true,
                                    "description": "Must be true to confirm the permanent, irreversible burn"
                                }
                            },
                            "required": ["confirm"]
                        }
                    }
                ]
            });
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            })
        }
        "tools/call" => {
            let call_params = payload.get("params");
            let tool_name = call_params
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let tool_args = call_params.and_then(|p| p.get("arguments"));

            let outcome: ToolOutcome = match tool_name {
                "redact_secrets" => call_redact_secrets(&state, tool_args).await,
                "inspect_jwt" => call_inspect_jwt(&state, tool_args).await,
                "scrub_metadata" => call_scrub_metadata(&state, tool_args).await,
                "burn_workspace_now" => {
                    let (o, d) =
                        call_burn_workspace_now(&state, tool_args, params.session_id.as_deref()).await;
                    burn_delivery = d;
                    o
                }
                _ => {
            let (content_items, is_error) = match tool_name {
                "convert_markdown_to_pdf" => {
                    let markdown = tool_args
                        .and_then(|a| a.get("markdown"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("");

                    if markdown.trim().is_empty() {
                        (
                            vec![serde_json::json!({
                                "type": "text",
                                "text": "Error: 'markdown' argument is required and cannot be empty"
                            })],
                            true,
                        )
                    } else {
                        let filename = tool_args
                            .and_then(|a| a.get("filename"))
                            .and_then(|f| f.as_str())
                            .unwrap_or("document.pdf");
                        let pdf_filename = if filename.ends_with(".pdf") {
                            filename.to_string()
                        } else {
                            format!("{}.pdf", filename.trim_end_matches(".md"))
                        };

                        match crate::routes::pandoc::generate_pandoc_pdf_bytes(markdown, &pdf_filename).await {
                            Ok(pdf_bytes) => {
                                use base64::prelude::BASE64_STANDARD;
                                use base64::Engine;
                                let base64_str = BASE64_STANDARD.encode(&pdf_bytes);
                                let byte_len = pdf_bytes.len();

                                let token = state
                                    .store_ephemeral_download(
                                        pdf_bytes,
                                        pdf_filename.clone(),
                                        "application/pdf".to_string(),
                                        300,
                                    )
                                    .await;
                                let download_url = format!("/api/apps/pandoc/download/{}", token);
                                let msg = format!(
                                    "Successfully generated PDF.\nFilename: {}\nSize: {} bytes\nDownload URL: {}\nNote: Embedded resource blob contains base64-encoded PDF. This is also available as a one-time, burn-after-reading download link expiring in 5 minutes.",
                                    pdf_filename, byte_len, download_url
                                );

                                let items = vec![
                                    serde_json::json!({
                                        "type": "text",
                                        "text": msg
                                    }),
                                    serde_json::json!({
                                        "type": "resource",
                                        "mimeType": "application/pdf",
                                        "blob": base64_str,
                                        "resource": {
                                            "uri": format!("file:///{}", pdf_filename),
                                            "mimeType": "application/pdf",
                                            "blob": base64_str
                                        }
                                    }),
                                ];
                                (items, false)
                            }
                            Err((_status, err_msg)) => {
                                (
                                    vec![serde_json::json!({
                                        "type": "text",
                                        "text": format!("Pandoc conversion failed: {}", err_msg)
                                    })],
                                    true,
                                )
                            }
                        }
                    }
                }
                "get_host_metrics" => {
                    let mut collector = crate::routes::monitor::MetricCollector::new();
                    let metrics = collector.collect();
                    let json_str = serde_json::to_string_pretty(&metrics)
                        .unwrap_or_else(|_| "{}".to_string());
                    (
                        vec![serde_json::json!({
                            "type": "text",
                            "text": json_str
                        })],
                        false,
                    )
                }
                unknown => (
                    vec![serde_json::json!({
                        "type": "text",
                        "text": format!("Unknown tool: {}", unknown)
                    })],
                    true,
                ),
            };
            ToolOutcome::Content(content_items, is_error)
                }
            };

            match outcome {
                ToolOutcome::Content(content_items, is_error) => {
                    let result = serde_json::json!({
                        "content": content_items,
                        "isError": is_error
                    });
                    serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result
                    })
                }
                ToolOutcome::RpcError { code, message } => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": code,
                        "message": message
                    }
                }),
            }
        }
        unknown_method => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", unknown_method)
                }
            })
        }
    };

    // Successful burn: the session map was cleared, so deliver through the
    // sender cloned before the burn, then shut down once delivered.
    if let Some(delivery) = burn_delivery {
        if delivery.sender.is_some() {
            let response_str = serde_json::to_string(&rpc_response).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to serialize JSON-RPC response: {}", e),
                )
            })?;
            let event = Event::default().event("message").data(response_str);
            delivery.finish(state.clone(), Some(event));
            return Ok((StatusCode::ACCEPTED, "Accepted").into_response());
        }
        delivery.finish(state.clone(), None);
        return Ok((StatusCode::OK, Json(rpc_response)).into_response());
    }

    // If an active SSE session exists, send the response through SSE event channel
    if let Some(ref sid) = params.session_id {
        let has_session = {
            let lock = state.mcp_sessions.read().await;
            lock.contains_key(sid)
        };
        if has_session {
            let response_str = serde_json::to_string(&rpc_response).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to serialize JSON-RPC response: {}", e),
                )
            })?;

            let sse_event = Event::default().event("message").data(response_str);

            state
                .send_mcp_message(sid, sse_event)
                .await
                .map_err(|e| (StatusCode::NOT_FOUND, e))?;

            return Ok((StatusCode::ACCEPTED, "Accepted").into_response());
        }
    }

    // Standalone single-flight HTTP POST: return JSON-RPC 2.0 response directly in HTTP response body
    Ok((StatusCode::OK, Json(rpc_response)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use disposable_common::SessionInfo;

    #[tokio::test]
    async fn test_mcp_initialize_and_ping() {
        let state = AppState::new();
        let session_id = "test-mcp-session-xyz".to_string();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        state.add_mcp_session(session_id.clone(), tx).await;

        let now = Utc::now().timestamp();
        let user_session = SessionInfo {
            session_id: "auth-session-1".to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: now,
            expires_at: now + 3600,
            token_type: None,
            label: None,
        };

        // 1. Test initialize
        let query = McpQuery {
            session_id: Some(session_id.clone()),
        };
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        let res = mcp_message_handler(
            AuthenticatedSession(user_session.clone()),
            State(state.clone()),
            Query(query),
            Json(init_req),
        )
        .await;

        assert!(res.is_ok());
        assert_eq!(res.unwrap().status(), StatusCode::ACCEPTED);

        // Receive from SSE channel
        let sse_msg = rx.recv().await.expect("Should receive SSE event");
        // Axum event debug string contains the data
        let debug_str = format!("{:?}", sse_msg);
        assert!(debug_str.contains("disposable-service"));
        assert!(debug_str.contains("2024-11-05"));

        // 2. Test ping
        let ping_query = McpQuery {
            session_id: Some(session_id.clone()),
        };
        let ping_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "ping"
        });

        let res_ping = mcp_message_handler(
            AuthenticatedSession(user_session.clone()),
            State(state.clone()),
            Query(ping_query),
            Json(ping_req),
        )
        .await;

        assert!(res_ping.is_ok());
        let ping_event = rx.recv().await.expect("Should receive ping SSE event");
        let ping_str = format!("{:?}", ping_event);
        assert!(ping_str.contains(r#"\"result\":{}"#));

        // 3. Test notifications/initialized (no SSE message, 200 OK)
        let notif_query = McpQuery {
            session_id: Some(session_id.clone()),
        };
        let notif_req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        let res_notif = mcp_message_handler(
            AuthenticatedSession(user_session.clone()),
            State(state.clone()),
            Query(notif_query),
            Json(notif_req),
        )
        .await;

        assert!(res_notif.is_ok());
        assert_eq!(res_notif.unwrap().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_mcp_tools_list_and_host_metrics() {
        let state = AppState::new();
        let session_id = "test-mcp-session-tools".to_string();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        state.add_mcp_session(session_id.clone(), tx).await;

        let now = Utc::now().timestamp();
        let user_session = SessionInfo {
            session_id: "auth-session-2".to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: now,
            expires_at: now + 3600,
            token_type: None,
            label: None,
        };

        // 1. Test tools/list
        let query = McpQuery {
            session_id: Some(session_id.clone()),
        };
        let list_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/list"
        });

        let res = mcp_message_handler(
            AuthenticatedSession(user_session.clone()),
            State(state.clone()),
            Query(query),
            Json(list_req),
        )
        .await;

        assert!(res.is_ok());
        let event = rx.recv().await.expect("Should receive tools/list event");
        let event_str = format!("{:?}", event);
        assert!(event_str.contains("convert_markdown_to_pdf"));
        assert!(event_str.contains("get_host_metrics"));

        // 2. Test tools/call get_host_metrics
        let call_query = McpQuery {
            session_id: Some(session_id.clone()),
        };
        let call_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {
                "name": "get_host_metrics",
                "arguments": {}
            }
        });

        let res_call = mcp_message_handler(
            AuthenticatedSession(user_session.clone()),
            State(state.clone()),
            Query(call_query),
            Json(call_req),
        )
        .await;

        assert!(res_call.is_ok());
        let call_event = rx.recv().await.expect("Should receive metrics event");
        let call_str = format!("{:?}", call_event);
        assert!(call_str.contains("cpu_usage_pct"));
        assert!(call_str.contains("memory_used_bytes"));
    }

    #[tokio::test]
    async fn test_mcp_tool_call_pandoc_and_ephemeral_download() {
        let state = AppState::new();
        let session_id = "test-mcp-session-pandoc".to_string();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        state.add_mcp_session(session_id.clone(), tx).await;

        let now = Utc::now().timestamp();
        let user_session = SessionInfo {
            session_id: "auth-session-3".to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: now,
            expires_at: now + 3600,
            token_type: None,
            label: None,
        };

        let call_query = McpQuery {
            session_id: Some(session_id.clone()),
        };
        let call_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "tools/call",
            "params": {
                "name": "convert_markdown_to_pdf",
                "arguments": {
                    "markdown": "# Title\n\nThis is a test document from MCP.",
                    "filename": "mcp_test.pdf"
                }
            }
        });

        let res = mcp_message_handler(
            AuthenticatedSession(user_session.clone()),
            State(state.clone()),
            Query(call_query),
            Json(call_req),
        )
        .await;

        assert!(res.is_ok());
        let event = rx.recv().await.expect("Should receive conversion event");
        let event_str = format!("{:?}", event);

        // If pandoc succeeds or environment has LaTeX
        if event_str.contains("Successfully generated PDF") {
            assert!(event_str.contains("/api/apps/pandoc/download/"));
            let prefix = "/api/apps/pandoc/download/";
            let start_idx = event_str.find(prefix).unwrap();
            let token_slice = &event_str[start_idx + prefix.len()..];
            let token: String = token_slice
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-')
                .collect();
            assert!(!token.is_empty());

            // Download using the token
            let dl_res = crate::routes::pandoc::download_pandoc_ephemeral(
                axum::extract::Path(token.clone()),
                State(state.clone()),
            )
            .await;
            assert!(dl_res.is_ok());

            // Second download fails (burn-after-reading)
            let dl_res2 = crate::routes::pandoc::download_pandoc_ephemeral(
                axum::extract::Path(token),
                State(state.clone()),
            )
            .await;
            assert!(dl_res2.is_err());
        }
    }

    #[tokio::test]
    async fn test_mcp_sse_handler_and_session_cleanup() {
        let state = AppState::new();
        let now = Utc::now().timestamp();
        let user_session = SessionInfo {
            session_id: "auth-session-sse".to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: now,
            expires_at: now + 3600,
            token_type: None,
            label: None,
        };

        let sse = mcp_sse_handler(AuthenticatedSession(user_session), State(state.clone())).await;

        let res = sse.into_response();
        use tokio_stream::StreamExt;
        let mut stream = res.into_body().into_data_stream();

        // Read initial endpoint event
        let item = stream.next().await;
        assert!(item.is_some());
        let chunk = item.unwrap().expect("Chunk should be Ok");
        let chunk_str = String::from_utf8_lossy(&chunk);
        assert!(chunk_str.contains("event: endpoint"));
        assert!(chunk_str.contains("/api/mcp/message?sessionId="));

        // Verify session was added to state
        let count = state.mcp_sessions.read().await.len();
        assert_eq!(count, 1);

        // Drop the stream
        drop(stream);

        // Small sleep to allow drop guard tokio task to run
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let count_after = state.mcp_sessions.read().await.len();
        assert_eq!(count_after, 0);
    }

    #[tokio::test]
    async fn test_mcp_standalone_single_flight_http_post() {
        let state = AppState::new();
        let now = Utc::now().timestamp();
        let user_session = SessionInfo {
            session_id: "auth-session-single-flight".to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: now,
            expires_at: now + 3600,
            token_type: Some("mcp_agent".to_string()),
            label: Some("CLI Tool".to_string()),
        };

        // Standalone call without any sessionId
        let query = McpQuery { session_id: None };
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 100,
            "method": "initialize",
            "params": {}
        });

        let res = mcp_message_handler(
            AuthenticatedSession(user_session.clone()),
            State(state.clone()),
            Query(query),
            Json(init_req),
        )
        .await;

        assert!(res.is_ok());
        let response = res.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Convert body to bytes to verify direct JSON-RPC payload
        use axum::body::to_bytes;
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body_json["jsonrpc"], "2.0");
        assert_eq!(body_json["id"], 100);
        assert_eq!(body_json["result"]["serverInfo"]["name"], "disposable-service");
    }

    #[tokio::test]
    async fn test_mcp_single_flight_tool_call_pandoc_base64() {
        let state = AppState::new();
        let now = Utc::now().timestamp();
        let user_session = SessionInfo {
            session_id: "auth-session-base64".to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: now,
            expires_at: now + 3600,
            token_type: Some("mcp_agent".to_string()),
            label: Some("Agent".to_string()),
        };

        let query = McpQuery { session_id: None };
        let call_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 101,
            "method": "tools/call",
            "params": {
                "name": "convert_markdown_to_pdf",
                "arguments": {
                    "markdown": "# Header\n\nContent for single flight",
                    "filename": "out.pdf"
                }
            }
        });

        let res = mcp_message_handler(
            AuthenticatedSession(user_session),
            State(state),
            Query(query),
            Json(call_req),
        )
        .await;

        assert!(res.is_ok());
        let response = res.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        use axum::body::to_bytes;
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body_json["jsonrpc"], "2.0");
        assert_eq!(body_json["id"], 101);

        let content = body_json["result"]["content"].as_array();
        assert!(content.is_some());
        let items = content.unwrap();
        // If conversion succeeded (environment has LaTeX or Typst), verify resource blob
        if items.len() >= 2 {
            assert_eq!(items[0]["type"], "text");
            assert_eq!(items[1]["type"], "resource");
            assert_eq!(items[1]["mimeType"], "application/pdf");
            assert!(items[1]["blob"].is_string());
            assert!(!items[1]["blob"].as_str().unwrap().is_empty());
        }
    }

    fn burn_test_session() -> SessionInfo {
        let now = Utc::now().timestamp();
        SessionInfo {
            session_id: "auth-burn".to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: "TestAgent".to_string(),
            login_time: now,
            expires_at: now + 3600,
            token_type: None,
            label: None,
        }
    }

    async fn call_burn(
        state: &AppState,
        sid: Option<&str>,
        args: serde_json::Value,
    ) -> Response {
        let req = serde_json::json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": { "name": "burn_workspace_now", "arguments": args }
        });
        mcp_message_handler(
            AuthenticatedSession(burn_test_session()),
            State(state.clone()),
            Query(McpQuery { session_id: sid.map(str::to_string) }),
            Json(req),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn burn_tool_rejects_missing_or_non_boolean_confirm() {
        let state = AppState::new();
        for args in [
            serde_json::json!({}),
            serde_json::json!({ "confirm": "true" }),
            serde_json::json!({ "confirm": false }),
        ] {
            let res = call_burn(&state, None, args).await;
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v["error"]["code"], -32602);
        }
        assert!(!state.burn.is_burning());
    }

    #[tokio::test]
    async fn burn_tool_sse_delivers_full_event_then_ends_stream() {
        let state = AppState::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        state.add_mcp_session("burn-sid".to_string(), tx).await;

        let res = call_burn(&state, Some("burn-sid"), serde_json::json!({ "confirm": true })).await;
        assert_eq!(res.status(), StatusCode::ACCEPTED);

        let ev = rx.recv().await.expect("result event");
        let s = format!("{:?}", ev);
        assert!(s.contains("verified"));
        // Stream ends after the event.
        assert!(tokio::time::timeout(std::time::Duration::from_secs(6), rx.recv())
            .await
            .expect("stream should end")
            .is_none());
        assert!(state.mcp_sessions.read().await.is_empty());

        // A second call reports a burn in progress.
        let res2 = call_burn(&state, None, serde_json::json!({ "confirm": true })).await;
        let bytes = axum::body::to_bytes(res2.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["error"]["message"], "penghancuran sedang berlangsung");
    }
}
