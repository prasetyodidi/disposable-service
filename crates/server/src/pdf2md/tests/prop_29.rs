//! Property 29: Paritas MCP dan REST.
//!
//! For any mock-engine document and valid options, the `pdf_to_markdown` MCP tool returns the
//! same `markdown`, `pages`, `headings` and `warnings` as the REST endpoint. Options that are
//! not given use the same defaults on both paths.

// Feature: pdf-to-markdown, Property 29: Paritas MCP dan REST
// **Validates: Requirements 11.2**

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use serde_json::{json, Value};

use crate::pdf2md::tests::gen_doc::arb_intermediate_doc;
use crate::pdf2md::tests::gen_options::arb_valid_options;
use crate::pdf2md::tests::helpers::{authed_pdf_app, pdf_bytes, test_runtime};
use crate::pdf2md::tests::mock_engine::{MockEngine, MockOutcome};
use crate::test_support::http::{post_json, send};

const URI: &str = "/api/apps/pdf-to-markdown";

fn multipart_with_options(token: &str, file: &[u8], options: &[u8]) -> Request<Body> {
    let b = "----parityboundary29";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!("--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.pdf\"\r\nContent-Type: application/pdf\r\n\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(file);
    body.extend_from_slice(
        format!("\r\n--{b}\r\nContent-Disposition: form-data; name=\"options\"\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(options);
    body.extend_from_slice(format!("\r\n--{b}--\r\n").as_bytes());
    Request::builder()
        .method(Method::POST)
        .uri(URI)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={b}"))
        .body(Body::from(body))
        .unwrap()
}

#[test]
fn mcp_pdf_to_markdown_equals_rest() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let job_root = tempfile::tempdir().unwrap();
    let engine = MockEngine::arc(MockOutcome::Err(crate::pdf2md::error::EngineError::Timeout));
    let (app, _state, token) =
        rt.block_on(authed_pdf_app(test_runtime(engine.clone(), job_root.path())));

    let mut runner = TestRunner::new(Config { cases: 100, ..Config::default() });
    runner
        .run(&(arb_intermediate_doc(), arb_valid_options()), |(doc, opts)| {
            engine.set_outcome(MockOutcome::Doc(doc));
            let file = pdf_bytes(64);
            let args_opts: Value = serde_json::from_slice(&opts.raw).unwrap();
            let mut args = args_opts.as_object().unwrap().clone();
            args.insert("file_base64".into(), json!(BASE64_STANDARD.encode(&file)));

            rt.block_on(async {
                let (st, _, body) = send(&app, multipart_with_options(&token, &file, &opts.raw)).await;
                let req = json!({
                    "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": { "name": "pdf_to_markdown", "arguments": Value::Object(args) }
                });
                let (mst, _, mbody) = send(&app, post_json("/api/mcp/message", &token, &req)).await;
                prop_assert_eq!(mst, StatusCode::OK);
                let rpc: Value = serde_json::from_slice(&mbody).unwrap();

                if st != StatusCode::OK {
                    // Same failure on both paths (e.g. no text layer): MCP must not succeed.
                    let failed = rpc.get("error").is_some()
                        || rpc["result"]["isError"].as_bool() == Some(true);
                    prop_assert!(failed, "REST {} but MCP succeeded", st);
                    return Ok(());
                }
                prop_assert!(rpc.get("error").is_none(), "MCP error: {}", rpc);
                prop_assert_eq!(rpc["result"]["isError"].as_bool(), Some(false));
                let text = rpc["result"]["content"][0]["text"].as_str().unwrap();
                let mcp: Value = serde_json::from_str(text).unwrap();
                let rest: Value = serde_json::from_slice(&body).unwrap();

                prop_assert_eq!(mcp["truncated"].as_bool(), Some(false));
                for key in ["markdown", "pages", "headings", "warnings"] {
                    prop_assert_eq!(&mcp[key], &rest[key], "field {}", key);
                }
                Ok(())
            })
        })
        .unwrap();
}
