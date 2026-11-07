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
}
