// Feature: privacy-micro-apps-and-burn-workspace, Property 6: Ephemeral_Download scrubber bersifat sekali pakai dan bertuntun
//
// **Validates: Requirements 3.7, 3.9, 8.4**
//
// Covers REST `?mode=ephemeral` and the MCP tool `scrub_metadata`:
// - first download succeeds with clean bytes (same as in-memory `scrub`),
// - second download fails (404),
// - TTL is 300 s, and a download whose stored expiry is in the past fails.
use axum::http::StatusCode;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use chrono::{Duration, Utc};
use proptest::prelude::*;

use super::gen_scrub::{arb_jpeg_spec, arb_pdf_spec, arb_png_spec};
use super::helpers_http::{authed_app, get, post_json, post_multipart_file, send};
use crate::privacy::scrubber::{scan, scrub};
use disposable_common::ScrubEphemeralResponse;

#[derive(Debug, Clone, Copy)]
enum Channel {
    Rest,
    Mcp,
}

#[derive(Debug, Clone, Copy)]
enum Second {
    /// Download twice in a row.
    Repeat,
    /// Make the stored expiry past, then download once.
    Expired,
}

fn arb_input() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        arb_jpeg_spec().prop_map(|s| s.build()),
        arb_png_spec().prop_map(|s| s.build()),
        arb_pdf_spec().prop_map(|s| s.build()),
    ]
}

fn run(input: Vec<u8>, channel: Channel, second: Second) -> Result<(), TestCaseError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let (app, state, bearer) = authed_app().await;
        let expected = scrub(&input).expect("scrub valid input").bytes;

        let before = Utc::now();
        let resp: ScrubEphemeralResponse = match channel {
            Channel::Rest => {
                let req = post_multipart_file(
                    "/api/apps/scrubber?mode=ephemeral",
                    &bearer,
                    "input.bin",
                    &input,
                );
                let (status, _h, body) = send(&app, req).await;
                prop_assert_eq!(status, StatusCode::OK);
                serde_json::from_slice(&body).expect("ephemeral json")
            }
            Channel::Mcp => {
                let rpc = serde_json::json!({
                    "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": { "name": "scrub_metadata",
                                "arguments": { "file_base64": BASE64_STANDARD.encode(&input) } }
                });
                let (status, _h, body) =
                    send(&app, post_json("/api/mcp/message", &bearer, &rpc)).await;
                prop_assert_eq!(status, StatusCode::OK);
                let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
                prop_assert_eq!(v["result"]["isError"].as_bool(), Some(false));
                let text = v["result"]["content"][0]["text"].as_str().expect("text");
                serde_json::from_str(text).expect("scrub result json")
            }
        };

        // TTL is 300 seconds, in the response and in the stored expiry.
        prop_assert_eq!(resp.expires_in_secs, 300);
        prop_assert_eq!(
            &resp.download_url,
            &format!("/api/apps/scrubber/download/{}", resp.token)
        );
        {
            let lock = state.downloads.read().await;
            let stored = lock.get(&resp.token).expect("stored download");
            let after = Utc::now();
            prop_assert!(stored.expires_at >= before + Duration::seconds(300));
            prop_assert!(stored.expires_at <= after + Duration::seconds(300));
        }

        match second {
            Second::Repeat => {
                let (status, _h, bytes) = send(&app, get(&resp.download_url, None)).await;
                prop_assert_eq!(status, StatusCode::OK);
                prop_assert_eq!(&bytes, &expected, "download must be the scrubbed bytes");
                let left = scan(&bytes).expect("scan");
                prop_assert!(left.removed.is_empty(), "metadata left: {:?}", left.removed);

                let (status2, _h, body2) = send(&app, get(&resp.download_url, None)).await;
                prop_assert_eq!(status2, StatusCode::NOT_FOUND);
                prop_assert!(body2.len() < expected.len().max(1) || body2 != expected);
            }
            Second::Expired => {
                {
                    let mut lock = state.downloads.write().await;
                    lock.get_mut(&resp.token).unwrap().expires_at =
                        Utc::now() - Duration::seconds(1);
                }
                let (status, _h, bytes) = send(&app, get(&resp.download_url, None)).await;
                prop_assert_eq!(status, StatusCode::NOT_FOUND);
                prop_assert!(bytes != expected);
                let (status2, _h, _b) = send(&app, get(&resp.download_url, None)).await;
                prop_assert_eq!(status2, StatusCode::NOT_FOUND);
            }
        }
        Ok(())
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn ephemeral_scrub_download_is_single_use_and_expires(
        input in arb_input(),
        mcp in any::<bool>(),
        expired in any::<bool>(),
    ) {
        run(
            input,
            if mcp { Channel::Mcp } else { Channel::Rest },
            if expired { Second::Expired } else { Second::Repeat },
        )?;
    }
}
