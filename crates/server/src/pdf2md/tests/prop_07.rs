// Feature: pdf-to-markdown, Property 7: Zero-footprint untuk setiap hasil
// **Validates: Requirements 11.9, 12.1, 12.2, 12.3, 12.6, 16.7**
use proptest::prelude::*;
use std::time::Duration;

use super::gen_doc::arb_intermediate_doc;
use super::helpers::{authed_pdf_app, pdf_bytes, test_runtime, tmp_listing_of};
use super::mock_engine::{MockEngine, MockOutcome};
use crate::pdf2md::error::EngineError;
use crate::test_support::http::{post_multipart_file, send};

const URI: &str = "/api/apps/pdf-to-markdown";

fn arb_engine_error() -> impl Strategy<Value = EngineError> {
    prop_oneof![
        Just(EngineError::Corrupt),
        Just(EngineError::Encrypted),
        Just(EngineError::NoTextLayer),
        Just(EngineError::Timeout),
        Just(EngineError::TooManyPages),
        Just(EngineError::Unavailable),
        Just(EngineError::OutputTooLarge),
        (1u32..500).prop_map(|total_pages| EngineError::PageFromOutOfRange { total_pages }),
    ]
}

fn arb_outcome() -> impl Strategy<Value = MockOutcome> {
    prop_oneof![
        arb_intermediate_doc().prop_map(MockOutcome::Doc),
        arb_engine_error().prop_map(MockOutcome::Err),
    ]
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

#[derive(Debug, Clone, Copy)]
enum Cancel {
    /// Future dropped before it is ever polled.
    BeforeExtract,
    /// Future dropped while the engine is sleeping inside `extract`.
    DuringExtract,
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Every engine result leaves the job root exactly as it was (empty).
    #[test]
    fn engine_results_leave_no_files(outcome in arb_outcome(), size in 5usize..2048) {
        let dir = tempfile::tempdir().unwrap();
        rt().block_on(async {
            let engine = MockEngine::arc(outcome);
            let (app, _state, token) =
                authed_pdf_app(test_runtime(engine.clone(), dir.path())).await;
            let before = tmp_listing_of(dir.path());
            let (status, _, _) =
                send(&app, post_multipart_file(URI, &token, "x.pdf", &pdf_bytes(size))).await;
            prop_assert!(status.as_u16() != 500, "unexpected 500");
            prop_assert_eq!(engine.calls(), 1);
            prop_assert_eq!(tmp_listing_of(dir.path()), before);
            Ok(())
        })?;
    }

    /// I/O failure while creating/writing the job directory -> 500 internal_error,
    /// engine not run, nothing left behind.
    #[test]
    fn io_failure_gives_500_without_running_engine(
        outcome in arb_outcome(),
        size in 5usize..512,
        readonly in any::<bool>(),
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = if readonly {
            let r = dir.path().join("ro");
            std::fs::create_dir(&r).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&r, std::fs::Permissions::from_mode(0o500)).unwrap();
            }
            r
        } else {
            // Parent is a regular file, so create_dir_all must fail.
            let f = dir.path().join("not-a-dir");
            std::fs::write(&f, b"x").unwrap();
            f.join("jobs")
        };
        let res = rt().block_on(async {
            let engine = MockEngine::arc(outcome);
            let (app, _state, token) = authed_pdf_app(test_runtime(engine.clone(), &root)).await;
            let (status, _, body) =
                send(&app, post_multipart_file(URI, &token, "x.pdf", &pdf_bytes(size))).await;
            (status, body, engine.calls())
        });
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if readonly {
                let _ = std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700));
            }
        }
        let (status, body, calls) = res;
        if status.as_u16() == 500 {
            let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
            prop_assert_eq!(v["code"].as_str(), Some("internal_error"));
            prop_assert_eq!(calls, 0);
        } else {
            // Only possible when a read-only directory is still writable (e.g. root user).
            prop_assert!(readonly);
        }
        if root.is_dir() {
            prop_assert!(tmp_listing_of(&root).is_empty());
        }
    }

    /// Cancelling the request future before or inside `extract` leaves nothing behind.
    #[test]
    fn cancelled_requests_leave_no_files(
        cancel in prop_oneof![Just(Cancel::BeforeExtract), Just(Cancel::DuringExtract)],
        size in 5usize..512,
    ) {
        let dir = tempfile::tempdir().unwrap();
        rt().block_on(async {
            let engine = MockEngine::sleep(Duration::from_secs(30));
            let (app, _state, token) =
                authed_pdf_app(test_runtime(engine.clone(), dir.path())).await;
            let req = post_multipart_file(URI, &token, "x.pdf", &pdf_bytes(size));
            match cancel {
                Cancel::BeforeExtract => {
                    let fut = send(&app, req);
                    drop(fut);
                    prop_assert_eq!(engine.calls(), 0);
                }
                Cancel::DuringExtract => {
                    // Drop the future only once the engine is inside `extract` (not after a
                    // fixed delay, which is too short on a loaded machine). The engine sleeps
                    // 30 s, so the request can never finish first.
                    let mut fut = Box::pin(send(&app, req));
                    let mut waited = 0;
                    while engine.calls() == 0 {
                        let r = tokio::time::timeout(Duration::from_millis(20), &mut fut).await;
                        prop_assert!(r.is_err(), "request should still be running");
                        waited += 1;
                        prop_assert!(waited < 1500, "engine was never called");
                    }
                    drop(fut);
                    prop_assert_eq!(engine.calls(), 1);
                }
            }
            prop_assert!(tmp_listing_of(dir.path()).is_empty());
            Ok(())
        })?;
    }
}
