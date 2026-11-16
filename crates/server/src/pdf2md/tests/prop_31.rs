// Feature: pdf-to-markdown, Property 31: Argumen MCP tidak valid ditolak tanpa efek samping
// **Validates: Requirements 11.5**
use axum::http::StatusCode;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use proptest::prelude::*;
use serde_json::{json, Value};

use super::helpers::{authed_pdf_app, pdf_bytes, test_runtime, tmp_listing_of};
use super::mock_engine::{MockEngine, MockOutcome};
use crate::pdf2md::model::IntermediateDoc;
use crate::test_support::http::{post_json, send};

const MAX_BYTES: u64 = 1024;

#[derive(Debug)]
struct Case {
    args: Value,
    named: String,
}

fn valid_file() -> Value {
    json!(BASE64_STANDARD.encode(pdf_bytes(50)))
}

fn bad_int() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(json!(0)),
        (-1000i64..=-1).prop_map(|n| json!(n)),
        (0.1f64..100.0).prop_map(|f| json!(f + 0.5)),
        "[a-z0-9]{0,5}".prop_map(|s| json!(s)),
        any::<bool>().prop_map(|b| json!(b)),
        Just(json!([1])),
        Just(json!({"a": 1})),
    ]
}

fn bad_bool() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<i64>().prop_map(|n| json!(n)),
        "[a-z]{0,5}".prop_map(|s| json!(s)),
        Just(json!([true])),
    ]
}

fn arb_case() -> impl Strategy<Value = Case> {
    fn file(name: &str, v: Value) -> serde_json::Map<String, Value> {
        let mut m = serde_json::Map::new();
        m.insert(name.to_string(), v);
        m
    }
    let missing = Just(()).prop_map(|_| Case { args: json!({}), named: "file_base64".into() });
    let missing_with_opts = (1u32..50).prop_map(|n| Case {
        args: json!({ "page_from": n }),
        named: "file_base64".into(),
    });
    let non_string = prop_oneof![
        any::<i64>().prop_map(|n| json!(n)),
        any::<bool>().prop_map(|b| json!(b)),
        Just(json!([1])),
        Just(json!({"a": 1})),
    ]
    .prop_map(|v| Case { args: json!({ "file_base64": v }), named: "file_base64".into() });
    let bad_b64 = "[!@#$%^&*()]{1,40}".prop_map(|s| Case {
        args: json!({ "file_base64": s }),
        named: "file_base64".into(),
    });
    let empty = Just(()).prop_map(|_| Case {
        args: json!({ "file_base64": "" }),
        named: "file_base64".into(),
    });
    let oversize = (MAX_BYTES as usize + 1..MAX_BYTES as usize * 3).prop_map(|n| Case {
        args: json!({ "file_base64": BASE64_STANDARD.encode(pdf_bytes(n)) }),
        named: "file_base64".into(),
    });
    let bad_page = (prop_oneof![Just("page_from"), Just("page_to")], bad_int()).prop_map(
        move |(name, v)| {
            let mut m = file("file_base64", valid_file());
            m.insert(name.to_string(), v);
            Case { args: Value::Object(m), named: name.into() }
        },
    );
    let reversed = (2u32..100_000, 1u32..100_000).prop_map(|(a, b)| {
        let from = a.max(b) + 1;
        let to = a.min(b);
        Case {
            args: json!({ "file_base64": valid_file(), "page_from": from, "page_to": to }),
            named: "page_from".into(),
        }
    });
    let bad_flag = (prop_oneof![Just("page_breaks"), Just("strip_headers_footers")], bad_bool())
        .prop_map(|(name, v)| {
            let mut m = file("file_base64", valid_file());
            m.insert(name.to_string(), v);
            Case { args: Value::Object(m), named: name.into() }
        });
    let unknown = "x_[a-z]{1,8}".prop_map(|name| {
        let mut m = file("file_base64", valid_file());
        m.insert(name.clone(), json!(1));
        Case { args: Value::Object(m), named: name }
    });
    prop_oneof![
        1 => missing,
        1 => missing_with_opts,
        2 => non_string,
        2 => bad_b64,
        1 => empty,
        2 => oversize,
        3 => bad_page,
        2 => reversed,
        2 => bad_flag,
        2 => unknown,
    ]
}

async fn assert_rejected(case: &Case) -> Result<(), TestCaseError> {
    let job_root = tempfile::tempdir().unwrap();
    let engine = MockEngine::arc(MockOutcome::Doc(IntermediateDoc::default()));
    let mut runtime = test_runtime(engine.clone(), job_root.path());
    runtime.config.max_bytes = MAX_BYTES;
    let (app, state, bearer) = authed_pdf_app(runtime).await;
    let before = tmp_listing_of(job_root.path());

    let rpc = json!({
        "jsonrpc": "2.0", "id": 9, "method": "tools/call",
        "params": { "name": "pdf_to_markdown", "arguments": case.args }
    });
    let (status, _h, body) = send(&app, post_json("/api/mcp/message", &bearer, &rpc)).await;
    prop_assert_eq!(status, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body).expect("json-rpc body");
    prop_assert_eq!(&v["error"]["code"], &json!(-32602), "args {}", case.args);
    let msg = v["error"]["message"].as_str().unwrap_or_default();
    prop_assert!(msg.contains(&case.named), "message {:?} must name {:?}", msg, case.named);
    prop_assert!(v.get("result").is_none());

    prop_assert_eq!(engine.calls(), 0, "engine must not be called");
    prop_assert!(state.downloads.read().await.is_empty(), "nothing may be stored");
    prop_assert!(tmp_listing_of(job_root.path()).is_empty(), "job_root must stay empty");
    prop_assert_eq!(tmp_listing_of(job_root.path()), before);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(120))]

    #[test]
    fn invalid_mcp_pdf_args_are_rejected_without_side_effects(case in arb_case()) {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(assert_rejected(&case))?;
    }
}
