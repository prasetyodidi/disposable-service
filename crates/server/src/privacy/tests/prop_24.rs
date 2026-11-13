// Feature: privacy-micro-apps-and-burn-workspace, Property 24: Argumen tool MCP tidak valid ditolak tanpa efek samping
//
// **Validates: Requirements 8.6, 13.3**
//
// For every invalid argument set (missing, wrong type, empty, over limits, bad
// base64, `confirm` not boolean `true`) the four new MCP tools answer with a
// JSON-RPC error -32602 whose message names the argument, store nothing in
// `downloads`, and never start a burn.
use axum::http::StatusCode;
use proptest::prelude::*;
use serde_json::{json, Value};

use super::helpers_http::{authed_app, post_json, send};

/// One invalid call: tool name, arguments, and the argument that must be named.
#[derive(Debug, Clone)]
struct Case {
    tool: &'static str,
    args: Value,
    named: &'static str,
}

fn arb_wrong_type() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<i64>().prop_map(|n| json!(n)),
        any::<bool>().prop_map(|b| json!(b)),
        Just(json!([1, 2])),
        Just(json!({"a": 1})),
    ]
}

fn with(mut base: Value, key: &str, v: Value) -> Value {
    base.as_object_mut().unwrap().insert(key.to_string(), v);
    base
}

fn arb_case() -> impl Strategy<Value = Case> {
    let redact = prop_oneof![
        Just(Case { tool: "redact_secrets", args: json!({}), named: "text" }),
        Just(Case { tool: "redact_secrets", args: json!({"text": ""}), named: "text" }),
        Just(Case { tool: "redact_secrets", args: json!({"text": null}), named: "text" }),
        arb_wrong_type().prop_map(|v| Case { tool: "redact_secrets", args: json!({ "text": v }), named: "text" }),
        arb_wrong_type().prop_map(|v| Case {
            tool: "redact_secrets",
            args: with(json!({"text": "hello"}), "categories", v),
            named: "categories",
        }),
        "[a-z]{3,10}x9".prop_map(|s| Case {
            tool: "redact_secrets",
            args: json!({"text": "hello", "categories": [s]}),
            named: "categories",
        }),
        arb_wrong_type().prop_map(|v| Case {
            tool: "redact_secrets",
            args: with(json!({"text": "hello"}), "custom_words", v),
            named: "custom_words",
        }),
        (101usize..130).prop_map(|n| Case {
            tool: "redact_secrets",
            args: json!({"text": "hello", "custom_words": vec!["w"; n]}),
            named: "custom_words",
        }),
        (257usize..400).prop_map(|n| Case {
            tool: "redact_secrets",
            args: json!({"text": "hello", "custom_words": ["a".repeat(n)]}),
            named: "custom_words",
        }),
        Just(Case {
            tool: "redact_secrets",
            args: json!({"text": "hello", "custom_words": [""]}),
            named: "custom_words",
        }),
        "[a-z]{3,10}x9".prop_map(|s| Case {
            tool: "redact_secrets",
            args: json!({"text": "hello", "format": s}),
            named: "format",
        }),
        arb_wrong_type().prop_map(|v| Case {
            tool: "redact_secrets",
            args: with(json!({"text": "hello"}), "format", v),
            named: "format",
        }),
    ];
    let jwt = prop_oneof![
        Just(Case { tool: "inspect_jwt", args: json!({}), named: "token" }),
        Just(Case { tool: "inspect_jwt", args: json!({"token": ""}), named: "token" }),
        arb_wrong_type().prop_map(|v| Case { tool: "inspect_jwt", args: json!({ "token": v }), named: "token" }),
        (8193usize..8400).prop_map(|n| Case {
            tool: "inspect_jwt",
            args: json!({"token": "a".repeat(n)}),
            named: "token",
        }),
        arb_wrong_type().prop_map(|v| Case {
            tool: "inspect_jwt",
            args: with(json!({"token": "a.b.c"}), "secret", v),
            named: "secret",
        }),
        (1025usize..1200).prop_map(|n| Case {
            tool: "inspect_jwt",
            args: json!({"token": "eyJhbGciOiJIUzI1NiJ9.e30.c2ln", "secret": "s".repeat(n)}),
            named: "secret",
        }),
        prop_oneof![
            any::<i64>().prop_map(|n| json!(n)),
            "[a-z]{1,5}".prop_map(|s| json!(s)),
            Just(json!([true])),
        ]
        .prop_map(|v| Case {
            tool: "inspect_jwt",
            args: with(json!({"token": "a.b.c"}), "redact", v),
            named: "redact",
        }),
    ];
    let scrub = prop_oneof![
        Just(Case { tool: "scrub_metadata", args: json!({}), named: "file_base64" }),
        Just(Case { tool: "scrub_metadata", args: json!({"file_base64": ""}), named: "file_base64" }),
        arb_wrong_type().prop_map(|v| Case {
            tool: "scrub_metadata",
            args: json!({ "file_base64": v }),
            named: "file_base64",
        }),
        "[A-Za-z0-9]{0,20}".prop_map(|s| Case {
            tool: "scrub_metadata",
            args: json!({ "file_base64": format!("{s}!!@#") }),
            named: "file_base64",
        }),
        arb_wrong_type().prop_map(|v| Case {
            tool: "scrub_metadata",
            args: with(json!({"file_base64": "AAAA"}), "filename", v),
            named: "filename",
        }),
    ];
    let burn = prop_oneof![
        Just(Case { tool: "burn_workspace_now", args: json!({}), named: "confirm" }),
        Just(Case { tool: "burn_workspace_now", args: json!({"confirm": false}), named: "confirm" }),
        Just(Case { tool: "burn_workspace_now", args: json!({"confirm": null}), named: "confirm" }),
        Just(Case { tool: "burn_workspace_now", args: json!({"confirm": "true"}), named: "confirm" }),
        any::<i64>().prop_map(|n| Case {
            tool: "burn_workspace_now",
            args: json!({ "confirm": n }),
            named: "confirm",
        }),
        Just(Case { tool: "burn_workspace_now", args: json!({"confirm": [true]}), named: "confirm" }),
    ];
    let non_object = prop_oneof![
        Just("redact_secrets"),
        Just("inspect_jwt"),
        Just("scrub_metadata"),
    ]
    .prop_flat_map(|tool| {
        prop_oneof![
            any::<i64>().prop_map(|n| json!(n)),
            "[a-z]{0,6}".prop_map(|s| json!(s)),
            Just(json!([1])),
        ]
        .prop_map(move |args| Case { tool, args, named: "arguments" })
    });
    prop_oneof![
        4 => redact,
        3 => jwt,
        3 => scrub,
        2 => burn,
        1 => non_object,
    ]
}

async fn assert_rejected_without_effects(case: &Case) -> Result<(), TestCaseError> {
    let (app, state, bearer) = authed_app().await;
    let rpc = json!({
        "jsonrpc": "2.0", "id": 7, "method": "tools/call",
        "params": { "name": case.tool, "arguments": case.args }
    });
    let (status, _h, body) = send(&app, post_json("/api/mcp/message", &bearer, &rpc)).await;
    prop_assert_eq!(status, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body).expect("json-rpc body");
    prop_assert_eq!(&v["error"]["code"], &json!(-32602), "tool {} args {}", case.tool, case.args);
    let msg = v["error"]["message"].as_str().unwrap_or_default();
    prop_assert!(
        msg.contains(case.named),
        "message {:?} must name argument {:?}",
        msg,
        case.named
    );
    prop_assert!(v.get("result").is_none());

    prop_assert!(state.downloads.read().await.is_empty(), "nothing may be stored");
    prop_assert!(!state.burn.is_burning(), "no burn may start");
    prop_assert_eq!(state.sessions.read().await.len(), 1, "session must survive");
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn invalid_mcp_args_are_rejected_without_side_effects(case in arb_case()) {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(assert_rejected_without_effects(&case))?;
    }
}

// Over-limit inputs are too large to generate per case; check them once each.
#[tokio::test]
async fn over_limit_inputs_are_rejected_without_side_effects() {
    use base64::prelude::BASE64_STANDARD;
    use base64::Engine;

    let over_file = BASE64_STANDARD.encode(vec![0u8; 26_214_401]);
    let cases = [
        Case {
            tool: "redact_secrets",
            args: json!({"text": "a".repeat(1_048_577)}),
            named: "text",
        },
        Case {
            tool: "scrub_metadata",
            args: json!({ "file_base64": over_file }),
            named: "file_base64",
        },
    ];
    for case in &cases {
        assert_rejected_without_effects(case)
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", case.tool));
    }
}
