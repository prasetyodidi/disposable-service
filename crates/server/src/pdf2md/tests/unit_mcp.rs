//! Unit tests for the `pdf_to_markdown` MCP tool (Requirements 11.1, 11.3, 11.4,
//! 11.6 to 11.9, 16.1). Uses `MockEngine` and an isolated `job_root`, so no
//! poppler is needed.
use super::helpers::{authed_pdf_app, capture_logs, pdf_bytes, test_runtime, tmp_listing_of};
use super::mock_engine::MockEngine;
use crate::pdf2md::error::EngineError;
use crate::pdf2md::model::{IntermediateDoc, Page, Span, TextLine};
use crate::privacy::burn::RealBackend;
use crate::test_support::http::{get, post_json, send};
use axum::http::StatusCode;
use axum::Router;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use std::sync::Arc;

const MSG: &str = "/api/mcp/message";

fn doc_with_text(text: &str) -> IntermediateDoc {
    let span = Span { text: text.into(), bold: false, italic: false, mono: false, size: 12 };
    let line = TextLine::new(vec![span], 12, 10, 150, 100, 16);
    IntermediateDoc {
        total_pages: 1,
        first_page: 1,
        last_page: 1,
        pages: vec![Page { number: 1, width: 600, height: 800, lines: vec![line] }],
    }
}

fn rpc(method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
}

fn call_req(args: serde_json::Value) -> serde_json::Value {
    rpc("tools/call", serde_json::json!({ "name": "pdf_to_markdown", "arguments": args }))
}

fn b64(bytes: &[u8]) -> String {
    BASE64_STANDARD.encode(bytes)
}

async fn rpc_call(app: &Router, token: &str, body: &serde_json::Value) -> serde_json::Value {
    let (status, _, bytes) = send(app, post_json(MSG, token, body)).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    serde_json::from_slice(&bytes).unwrap()
}

/// Parses the JSON payload carried in `result.content[0].text`.
fn tool_payload(v: &serde_json::Value) -> serde_json::Value {
    assert!(v.get("error").is_none(), "unexpected error: {v}");
    assert_eq!(v["result"]["isError"], false);
    serde_json::from_str(v["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn tools_list_has_pdf_to_markdown_once_and_six_old_tools_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let (app, _state, token) =
        authed_pdf_app(test_runtime(MockEngine::doc(doc_with_text("x")), root.path())).await;
    let v = rpc_call(&app, &token, &rpc("tools/list", serde_json::json!({}))).await;
    let tools = v["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(names.iter().filter(|n| **n == "pdf_to_markdown").count(), 1);
    for n in [
        "convert_markdown_to_pdf",
        "get_host_metrics",
        "redact_secrets",
        "inspect_jwt",
        "scrub_metadata",
        "burn_workspace_now",
    ] {
        assert_eq!(names.iter().filter(|x| **x == n).count(), 1, "tool {n}");
    }
    assert_eq!(names.len(), 7);
    // New tool is appended after the burn tool; old schemas are unchanged.
    assert_eq!(names.last(), Some(&"pdf_to_markdown"));
    let by = |n: &str| tools.iter().find(|t| t["name"] == n).unwrap();
    assert_eq!(by("convert_markdown_to_pdf")["inputSchema"]["required"], serde_json::json!(["markdown"]));
    assert_eq!(by("get_host_metrics")["inputSchema"]["properties"], serde_json::json!({}));
    assert_eq!(by("burn_workspace_now")["inputSchema"]["required"], serde_json::json!(["confirm"]));
    assert_eq!(by("scrub_metadata")["inputSchema"]["required"], serde_json::json!(["file_base64"]));
    let schema = &by("pdf_to_markdown")["inputSchema"];
    assert_eq!(schema["required"], serde_json::json!(["file_base64"]));
    for p in ["file_base64", "page_from", "page_to", "page_breaks", "strip_headers_footers"] {
        assert!(schema["properties"][p].is_object(), "missing property {p}");
    }
}

#[tokio::test]
async fn success_returns_inline_markdown_without_download() {
    let root = tempfile::tempdir().unwrap();
    let (app, state, token) =
        authed_pdf_app(test_runtime(MockEngine::doc(doc_with_text("Halo dunia")), root.path())).await;
    let v = rpc_call(&app, &token, &call_req(serde_json::json!({ "file_base64": b64(&pdf_bytes(200)) }))).await;
    let p = tool_payload(&v);
    assert!(p["markdown"].as_str().unwrap().contains("Halo dunia"));
    assert_eq!(p["truncated"], false);
    assert_eq!(p["pages"], 1);
    assert!(p.get("download_url").map_or(true, |u| u.is_null()));
    assert!(state.downloads.read().await.is_empty());
    assert!(tmp_listing_of(root.path()).is_empty());
}

#[tokio::test]
async fn long_result_is_truncated_and_download_url_works() {
    let root = tempfile::tempdir().unwrap();
    let text = "abcdefghi ".repeat(11_000); // > 100,000 characters
    let (app, state, token) =
        authed_pdf_app(test_runtime(MockEngine::doc(doc_with_text(&text)), root.path())).await;
    let v = rpc_call(&app, &token, &call_req(serde_json::json!({ "file_base64": b64(&pdf_bytes(200)) }))).await;
    let p = tool_payload(&v);
    assert_eq!(p["truncated"], true);
    assert!(p.get("markdown").map_or(true, |m| m.is_null()));
    let preview = p["preview"].as_str().unwrap();
    assert_eq!(preview.chars().count(), 2_000);
    let url = p["download_url"].as_str().unwrap();
    assert!(url.starts_with("/api/apps/pdf-to-markdown/download/"), "{url}");
    assert_eq!(state.downloads.read().await.len(), 1);

    let (status, _, body) = send(&app, get(url, Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    let full = String::from_utf8(body).unwrap();
    assert!(full.chars().count() > 100_000);
    assert!(full.starts_with(preview));
    // Burn after reading.
    let (status, _, _) = send(&app, get(url, Some(&token))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(tmp_listing_of(root.path()).is_empty());
}

#[tokio::test]
async fn document_errors_map_to_32000_with_data_code() {
    let cases: Vec<(EngineError, &str)> = vec![
        (EngineError::Corrupt, "corrupt_pdf"),
        (EngineError::Encrypted, "encrypted_pdf"),
        (EngineError::NoTextLayer, "no_text_layer"),
        (EngineError::TooManyPages, "too_many_pages"),
        (EngineError::OutputTooLarge, "output_too_large"),
        (EngineError::Timeout, "timeout"),
        (EngineError::Unavailable, "engine_unavailable"),
        (EngineError::PageFromOutOfRange { total_pages: 3 }, "page_from_out_of_range"),
    ];
    for (err, code) in cases {
        let root = tempfile::tempdir().unwrap();
        let (app, _state, token) = authed_pdf_app(test_runtime(MockEngine::err(err), root.path())).await;
        let v = rpc_call(&app, &token, &call_req(serde_json::json!({ "file_base64": b64(&pdf_bytes(100)) }))).await;
        assert_eq!(v["error"]["code"], -32000, "{code}: {v}");
        assert_eq!(v["error"]["data"]["code"], code, "{v}");
        assert!(v["error"]["message"].as_str().is_some_and(|m| !m.is_empty()));
        assert!(v.get("result").is_none());
        assert!(tmp_listing_of(root.path()).is_empty(), "{code}: job dir left behind");
    }
}

#[tokio::test]
async fn not_a_pdf_and_empty_text_and_oversized_output_map_to_32000() {
    let root = tempfile::tempdir().unwrap();
    let engine = MockEngine::doc(doc_with_text("Halo"));
    let (app, state, token) = authed_pdf_app(test_runtime(engine.clone(), root.path())).await;

    let v = rpc_call(&app, &token, &call_req(serde_json::json!({ "file_base64": b64(b"bukan pdf") }))).await;
    assert_eq!(v["error"]["code"], -32000);
    assert_eq!(v["error"]["data"]["code"], "not_a_pdf");
    assert_eq!(engine.calls(), 0);

    // Whitespace-only document: no text layer.
    let blank = Arc::new(MockEngine::new(super::mock_engine::MockOutcome::Doc(doc_with_text("   "))));
    let (app2, _s, token2) = authed_pdf_app(test_runtime(blank, root.path())).await;
    let v = rpc_call(&app2, &token2, &call_req(serde_json::json!({ "file_base64": b64(&pdf_bytes(100)) }))).await;
    assert_eq!(v["error"]["data"]["code"], "no_text_layer");

    // Response larger than max_response_bytes.
    let mut rt = test_runtime(MockEngine::doc(doc_with_text("Halo dunia")), root.path());
    rt.config.max_response_bytes = Some(10);
    let (app3, _s, token3) = authed_pdf_app(rt).await;
    let v = rpc_call(&app3, &token3, &call_req(serde_json::json!({ "file_base64": b64(&pdf_bytes(100)) }))).await;
    assert_eq!(v["error"]["code"], -32000);
    assert_eq!(v["error"]["data"]["code"], "output_too_large");

    assert!(state.downloads.read().await.is_empty());
    assert!(tmp_listing_of(root.path()).is_empty());
}

#[tokio::test]
async fn invalid_arguments_are_32602_without_calling_engine() {
    let root = tempfile::tempdir().unwrap();
    let engine = MockEngine::doc(doc_with_text("Halo"));
    let (app, _state, token) = authed_pdf_app(test_runtime(engine.clone(), root.path())).await;
    let ok = b64(&pdf_bytes(100));
    for (args, name) in [
        (serde_json::json!({}), "file_base64"),
        (serde_json::json!({ "file_base64": ok, "bogus": 1 }), "bogus"),
        (serde_json::json!({ "file_base64": ok, "page_from": 5, "page_to": 2 }), "page_from"),
        (serde_json::json!({ "file_base64": "!!!" }), "file_base64"),
        (serde_json::json!({ "file_base64": "" }), "file_base64"),
    ] {
        let v = rpc_call(&app, &token, &call_req(args)).await;
        assert_eq!(v["error"]["code"], -32602, "{v}");
        assert!(v["error"]["message"].as_str().unwrap().contains(name), "{v}");
    }
    assert_eq!(engine.calls(), 0);
    assert!(tmp_listing_of(root.path()).is_empty());
}

#[tokio::test]
async fn internal_failure_is_32603() {
    let dir = tempfile::tempdir().unwrap();
    // job_root is a regular file, so the job directory cannot be created.
    let bogus_root = dir.path().join("not-a-dir");
    std::fs::write(&bogus_root, b"x").unwrap();
    let (app, _state, token) =
        authed_pdf_app(test_runtime(MockEngine::doc(doc_with_text("Halo")), &bogus_root)).await;
    let v = rpc_call(&app, &token, &call_req(serde_json::json!({ "file_base64": b64(&pdf_bytes(100)) }))).await;
    assert_eq!(v["error"]["code"], -32603, "{v}");
}

#[tokio::test]
async fn unauthenticated_call_is_401_and_does_no_work() {
    let root = tempfile::tempdir().unwrap();
    let engine = MockEngine::doc(doc_with_text("Halo"));
    let (app, _state, _token) = authed_pdf_app(test_runtime(engine.clone(), root.path())).await;
    let body = call_req(serde_json::json!({ "file_base64": b64(&pdf_bytes(100)) }));
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(MSG)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let (status, _, _) = send(&app, req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = send(&app, post_json(MSG, "bukan-token", &body)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(engine.calls(), 0);
    assert!(tmp_listing_of(root.path()).is_empty());
}

#[tokio::test]
async fn call_during_burn_is_rejected_with_burn_in_progress() {
    let root = tempfile::tempdir().unwrap();
    let engine = MockEngine::doc(doc_with_text("Halo"));
    let (app, state, token) = authed_pdf_app(test_runtime(engine.clone(), root.path())).await;
    // `run_with` never schedules a process shutdown.
    state.burn.run_with(&RealBackend::new(&state), None, None).await;

    let body = call_req(serde_json::json!({ "file_base64": b64(&pdf_bytes(100)) }));
    let (status, _, bytes) = send(&app, post_json(MSG, &token, &body)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["code"], "burn_in_progress");
    assert_eq!(engine.calls(), 0);
    assert!(state.downloads.read().await.is_empty());
    assert!(tmp_listing_of(root.path()).is_empty());
}

#[tokio::test]
async fn logs_never_contain_arguments_content_or_paths() {
    let logs = capture_logs();
    let root = tempfile::tempdir().unwrap();
    let secret_text = "RAHASIA-KONTEN-UNIK-42";
    let mut pdf = pdf_bytes(300);
    pdf.extend_from_slice(b"PAYLOAD-BIN-UNIK");
    let payload = b64(&pdf);

    // Success path.
    let (app, _s, token) =
        authed_pdf_app(test_runtime(MockEngine::doc(doc_with_text(secret_text)), root.path())).await;
    let v = rpc_call(&app, &token, &call_req(serde_json::json!({ "file_base64": payload }))).await;
    assert!(tool_payload(&v)["markdown"].as_str().unwrap().contains(secret_text));

    // Error paths.
    for err in [EngineError::Corrupt, EngineError::Encrypted, EngineError::Unavailable] {
        let (app, _s, token) = authed_pdf_app(test_runtime(MockEngine::err(err), root.path())).await;
        let _ = rpc_call(&app, &token, &call_req(serde_json::json!({ "file_base64": payload }))).await;
    }
    let _ = rpc_call(&app, &token, &call_req(serde_json::json!({ "file_base64": "!!!tidak-valid" }))).await;

    let out = logs.contents();
    for needle in [
        payload.as_str(),
        "PAYLOAD-BIN-UNIK",
        secret_text,
        "tidak-valid",
        root.path().to_str().unwrap(),
        "pdf2md-job-",
        "input.pdf",
    ] {
        assert!(!out.contains(needle), "log leaks {needle:?}: {out}");
    }
}

/// The pdf_to_markdown description and schema report this server's real limits,
/// so agents on Lambda see 5 MB instead of the 25 MB default.
#[tokio::test]
async fn pdf_to_markdown_schema_reports_configured_limits() {
    let root = tempfile::tempdir().unwrap();
    let mut rt = test_runtime(MockEngine::doc(doc_with_text("x")), root.path());
    rt.config.max_bytes = 5_242_880;
    let (app, _state, token) = authed_pdf_app(rt).await;
    let v = rpc_call(&app, &token, &rpc("tools/list", serde_json::json!({}))).await;
    let tool = v["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "pdf_to_markdown")
        .unwrap()
        .clone();
    let desc = tool["description"].as_str().unwrap();
    assert!(desc.contains("5242880 bytes") && desc.contains("5 MB"), "{desc}");
    assert!(desc.contains("300 pages"), "{desc}");
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert!(tool["inputSchema"]["properties"]["file_base64"]["description"]
        .as_str()
        .unwrap()
        .contains("5242880"));
}
