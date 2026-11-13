// Feature: privacy-micro-apps-and-burn-workspace, Property 23: Paritas MCP dan REST
//
// **Validates: Requirements 8.2, 8.3**
//
// For the same input, the MCP tools `redact_secrets` / `inspect_jwt` return a
// result identical to the REST endpoints `/api/apps/redactor` / `/api/apps/jwt`.
use super::gen_jwt::*;
use super::gen_redact::*;
use super::helpers_http::*;
use disposable_common::{JwtInspectResult, RedactResponse};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use serde_json::{json, Value};

/// Calls an MCP tool over the standalone single-flight HTTP endpoint.
async fn mcp_call(app: &axum::Router, token: &str, name: &str, args: Value) -> Value {
    let req = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": name, "arguments": args }
    });
    let (status, _, body) = send(app, post_json("/api/mcp/message", token, &req)).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    serde_json::from_slice(&body).expect("json-rpc body")
}

fn mcp_text(rpc: &Value) -> (&str, bool) {
    let r = &rpc["result"];
    (
        r["content"][0]["text"].as_str().expect("text content"),
        r["isError"].as_bool().expect("isError"),
    )
}

#[test]
fn redact_secrets_mcp_equals_rest() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (app, _state, token) = rt.block_on(authed_app());
    let mut runner = TestRunner::new(Config { cases: 100, ..Config::default() });
    let strat = (multiline_text(4), category_set(), custom_words(5), format_strategy());
    runner
        .run(&strat, |(sentence, cats, words, format)| {
            // MCP requires text of 1..=1_048_576 chars (design: redact_secrets);
            // the generator can emit an empty string, which is outside parity scope.
            prop_assume!(!sentence.text.is_empty());
            let cat_names: Vec<&str> = cats.iter().map(|c| c.as_str()).collect();
            let fmt = match format {
                crate::privacy::redactor::Format::Auto => "auto",
                crate::privacy::redactor::Format::Env => "env",
                crate::privacy::redactor::Format::Text => "text",
            };
            let args = json!({
                "text": sentence.text, "categories": cat_names,
                "custom_words": words, "format": fmt
            });
            rt.block_on(async {
                let (st, _, body) =
                    send(&app, post_json("/api/apps/redactor", &token, &args)).await;
                prop_assert_eq!(st, axum::http::StatusCode::OK);
                let rest: RedactResponse = serde_json::from_slice(&body).unwrap();

                let rpc = mcp_call(&app, &token, "redact_secrets", args).await;
                let (text, is_error) = mcp_text(&rpc);
                prop_assert!(!is_error);
                let mcp: RedactResponse = serde_json::from_str(text).unwrap();
                prop_assert_eq!(mcp, rest);
                Ok(())
            })
        })
        .unwrap();
}

#[test]
fn inspect_jwt_mcp_equals_rest() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (app, _state, token) = rt.block_on(authed_app());
    let mut runner = TestRunner::new(Config { cases: 100, ..Config::default() });
    // Valid tokens (with/without secret, redact on/off) and corrupted tokens.
    let token_strat = prop_oneof![
        3 => jwt_case().prop_map(|c| (c.token, Some(String::from_utf8_lossy(&c.secret).into_owned()))),
        1 => jwt_case().prop_map(|c| (c.token, None)),
        1 => corrupted_token().prop_map(|c| (c.token, None)),
    ];
    let strat = (token_strat, any::<bool>());
    runner
        .run(&strat, |((jwt, secret), redact)| {
            let args = json!({ "token": jwt, "secret": secret, "redact": redact });
            rt.block_on(async {
                let (st, _, body) = send(&app, post_json("/api/apps/jwt", &token, &args)).await;
                let rpc = mcp_call(&app, &token, "inspect_jwt", args).await;
                if let Some(err) = rpc.get("error") {
                    // Invalid params (e.g. secret length): REST rejects too, never 200.
                    prop_assert_ne!(st, axum::http::StatusCode::OK);
                    prop_assert_eq!(err["code"].as_i64(), Some(-32602));
                    return Ok(());
                }
                let (text, is_error) = mcp_text(&rpc);
                if st == axum::http::StatusCode::OK {
                    prop_assert!(!is_error);
                    let rest: JwtInspectResult = serde_json::from_slice(&body).unwrap();
                    let mcp: JwtInspectResult = serde_json::from_str(text).unwrap();
                    prop_assert_eq!(mcp, rest);
                } else {
                    prop_assert!(is_error);
                    let rest: Value = serde_json::from_slice(&body).unwrap();
                    prop_assert_eq!(rest["message"].as_str(), Some(text));
                }
                Ok(())
            })
        })
        .unwrap();
}
