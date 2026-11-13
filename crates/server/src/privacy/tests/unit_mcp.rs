//! Unit tests for the MCP surface: tools/list, auth, and `burn_workspace_now`.
//! Validates: Requirements 8.1, 8.7, 13.1, 13.2, 13.4, 13.5, 13.6, 13.7.
//!
//! Burn *failure* (-32603) cannot be injected through the HTTP/MCP path: the
//! tool always uses `RealBackend`, and making its wipe fail would require
//! tampering with the shared `workspace_tmp_root()`, which would race with
//! sibling tests. The failure path of the coordinator is covered with a fake
//! `StorageBackend` in `unit_burn.rs`; the mapping to -32603 is exercised in
//! `routes/mcp.rs` (`ToolOutcome::internal`).
//! Basic confirm validation and the SSE/second-call happy path are already in
//! `routes/mcp.rs`; the tests here cover the remaining behaviour.

use axum::http::StatusCode;
use std::time::Duration;

use super::helpers_http::{authed_app, get, post_json, send};

fn rpc(id: i64, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

#[tokio::test]
async fn tools_list_has_new_tools_and_keeps_old_ones() {
    let (app, _state, token) = authed_app().await;
    let (status, _, body) = send(
        &app,
        post_json("/api/mcp/message", &token, &rpc(1, "tools/list", serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let tools = v["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for n in [
        "redact_secrets",
        "inspect_jwt",
        "scrub_metadata",
        "burn_workspace_now",
        "convert_markdown_to_pdf",
        "get_host_metrics",
    ] {
        assert!(names.contains(&n), "missing tool {n}");
    }
    assert_eq!(names.len(), 6);

    // Old tools keep their original schema.
    let pdf = tools.iter().find(|t| t["name"] == "convert_markdown_to_pdf").unwrap();
    assert_eq!(pdf["inputSchema"]["required"], serde_json::json!(["markdown"]));
    assert!(pdf["inputSchema"]["properties"]["filename"].is_object());
    let metrics = tools.iter().find(|t| t["name"] == "get_host_metrics").unwrap();
    assert_eq!(metrics["inputSchema"]["properties"], serde_json::json!({}));

    // Burn tool requires `confirm`.
    let burn = tools.iter().find(|t| t["name"] == "burn_workspace_now").unwrap();
    assert_eq!(burn["inputSchema"]["required"], serde_json::json!(["confirm"]));
}

#[tokio::test]
async fn mcp_endpoints_require_a_session() {
    let (app, _state, _token) = authed_app().await;
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/mcp/message")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_vec(&rpc(1, "tools/list", serde_json::json!({}))).unwrap(),
        ))
        .unwrap();
    let (status, _, _) = send(&app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _, _) = send(&app, get("/api/mcp/sse", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Invalid token is rejected too.
    let (status, _, _) = send(
        &app,
        post_json("/api/mcp/message", "not-a-token", &rpc(1, "tools/list", serde_json::json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn burn_via_sse_delivers_full_report_before_stream_closes() {
    let (app, state, token) = authed_app().await;
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    state.add_mcp_session("unit-mcp-sid".to_string(), tx).await;
    state
        .store_ephemeral_download(vec![9; 32], "f.bin".into(), "application/octet-stream".into(), 60)
        .await;

    let req = rpc(
        5,
        "tools/call",
        serde_json::json!({ "name": "burn_workspace_now", "arguments": { "confirm": true } }),
    );
    let (status, _, _) = send(&app, post_json("/api/mcp/message?sessionId=unit-mcp-sid", &token, &req)).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("event in time")
        .expect("full event before close");
    let s = format!("{:?}", ev);
    // The event carries the whole audit report, not a truncated message.
    for key in ["jsonrpc", "total_bytes_before", "remaining_bytes", "removed_items", "duration_ms", "timestamp_utc"] {
        assert!(s.contains(key), "event missing {key}: {s}");
    }
    // Only then does the stream close.
    let closed = tokio::time::timeout(Duration::from_secs(6), rx.recv())
        .await
        .expect("stream should close");
    assert!(closed.is_none());
    assert!(state.mcp_sessions.read().await.is_empty());
    assert!(state.downloads.read().await.is_empty());
}

#[tokio::test]
async fn burn_publishes_burn_event_to_ui_subscribers() {
    let (app, state, token) = authed_app().await;
    let mut ui_rx = state.ui_events.subscribe();

    let req = rpc(
        6,
        "tools/call",
        serde_json::json!({ "name": "burn_workspace_now", "arguments": { "confirm": true } }),
    );
    let (status, _, body) = send(&app, post_json("/api/mcp/message", &token, &req)).await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("error").is_none(), "unexpected error: {v}");

    let report = tokio::time::timeout(Duration::from_secs(3), ui_rx.recv())
        .await
        .expect("ui event in time")
        .expect("report");
    assert!(state.burn.is_burning());
    assert_eq!(report.before.len(), 4);

    // The tool result text is the same report.
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(parsed["verified"], report.verified);
}

#[tokio::test]
async fn second_burn_while_running_returns_in_progress_error() {
    use crate::auth::AuthenticatedSession;
    use crate::routes::mcp::{mcp_message_handler, McpQuery};
    use axum::extract::{Query, State};
    use axum::Json;

    let (_app, state, _token) = authed_app().await;
    // Burn has been triggered (running or finished): flag is set.
    let _ = state.burn.run_or_join(&state).await;
    assert!(state.burn.is_burning());

    let now = chrono::Utc::now().timestamp();
    let session = disposable_common::SessionInfo {
        session_id: "unit-mcp-second".into(),
        ip_address: "127.0.0.1".into(),
        user_agent: "test".into(),
        login_time: now,
        expires_at: now + 3600,
        token_type: None,
        label: None,
    };
    let req = rpc(
        8,
        "tools/call",
        serde_json::json!({ "name": "burn_workspace_now", "arguments": { "confirm": true } }),
    );
    let res = mcp_message_handler(
        AuthenticatedSession(session),
        State(state.clone()),
        Query(McpQuery { session_id: None }),
        Json(req),
    )
    .await
    .unwrap();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"]["code"], -32603);
    assert_eq!(v["error"]["message"], "penghancuran sedang berlangsung");
}
