//! Property 5: invalid input is rejected without output or storage.
//!
//! Covers `scrub()` and `POST /api/apps/scrubber` (both modes) with non-format
//! bytes and corrupted files: 415/422, no new entry in `state.downloads`, and no
//! new files in the workspace temp dir.
//!
//! **Validates: Requirements 1.5, 1.6, 2.5, 3.8**

use std::collections::BTreeSet;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use disposable_common::SessionInfo;
use proptest::prelude::*;
use tower::ServiceExt;

use super::gen_scrub::*;
use crate::privacy::scrubber::{scrub, ScrubError};
use crate::routes::create_api_router;
use crate::state::{workspace_tmp_root, AppState};

const BOUNDARY: &str = "----prop05boundary";

fn multipart_body(file: &[u8]) -> Vec<u8> {
    let mut b = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"in.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    b.extend_from_slice(file);
    b.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    b
}

fn temp_listing() -> BTreeSet<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut BTreeSet<std::path::PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                }
                out.insert(p);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(&workspace_tmp_root(), &mut out);
    // Sibling tests (pandoc and pdf2md jobs) legitimately create entries in the shared
    // root concurrently; only the scrubber's own writes are relevant here.
    out.retain(|p| {
        !p.components()
            .any(|c| {
                let n = c.as_os_str().to_string_lossy();
                n.starts_with("pandoc-job-") || n.starts_with("pdf2md-job-")
            })
    });
    out
}

/// Authenticated state: a session registered and its session id used as Bearer token.
async fn authed_state() -> (AppState, String) {
    let state = AppState::new();
    let now = Utc::now().timestamp();
    let id = "prop05-session".to_string();
    state
        .add_session(SessionInfo {
            session_id: id.clone(),
            ip_address: "127.0.0.1".into(),
            user_agent: "prop05".into(),
            login_time: now,
            expires_at: now + 3600,
            token_type: None,
            label: None,
        })
        .await;
    (state, id)
}

/// POST the bytes to the scrubber in both modes; assert `expected` status, no download stored.
async fn post_and_check(bytes: &[u8], expected: StatusCode) -> Result<(), TestCaseError> {
    let (state, token) = authed_state().await;
    let before_tmp = temp_listing();
    for mode in ["direct", "ephemeral"] {
        let app = create_api_router(state.clone()).with_state(state.clone());
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/apps/scrubber?mode={mode}"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", format!("multipart/form-data; boundary={BOUNDARY}"))
            .body(Body::from(multipart_body(bytes)))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        prop_assert_eq!(res.status(), expected, "mode {}", mode);
        prop_assert!(res.headers().get("x-scrub-report").is_none());
        prop_assert!(state.downloads.read().await.is_empty());
    }
    let after_tmp = temp_listing();
    prop_assert!(after_tmp.is_subset(&before_tmp), "new temp files: {:?}", after_tmp.difference(&before_tmp).collect::<Vec<_>>());
    Ok(())
}

fn run<F: std::future::Future<Output = Result<(), TestCaseError>>>(f: F) -> Result<(), TestCaseError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

fn expected_status(c: &Corruption) -> (StatusCode, bool) {
    match c {
        Corruption::Header { .. } => (StatusCode::UNSUPPORTED_MEDIA_TYPE, true),
        _ => (StatusCode::UNPROCESSABLE_ENTITY, false),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: privacy-micro-apps-and-burn-workspace, Property 5: Input tidak valid ditolak tanpa keluaran atau penyimpanan
    #[test]
    fn scrub_rejects_invalid_input(
        j in arb_jpeg_spec(), p in arb_png_spec(), d in arb_pdf_spec(),
        c in arb_corruption(), junk in arb_non_format_bytes(),
    ) {
        let before_tmp = temp_listing();
        prop_assert_eq!(scrub(&junk).err(), Some(ScrubError::UnsupportedType));
        let (_, unsupported) = expected_status(&c);
        for (f, bytes) in [(Format::Jpeg, j.build()), (Format::Png, p.build()), (Format::Pdf, d.build())] {
            let bad = corrupt(f, &bytes, &c);
            match scrub(&bad) {
                Err(ScrubError::UnsupportedType) => prop_assert!(unsupported),
                Err(ScrubError::Unreadable(_)) => prop_assert!(!unsupported),
                other => prop_assert!(false, "unexpected: {:?}", other.map(|o| o.kind)),
            }
        }
        prop_assert!(temp_listing().is_subset(&before_tmp));
    }

    // Feature: privacy-micro-apps-and-burn-workspace, Property 5: Input tidak valid ditolak tanpa keluaran atau penyimpanan
    #[test]
    fn handler_rejects_non_format_bytes(junk in arb_non_format_bytes()) {
        run(post_and_check(&junk, StatusCode::UNSUPPORTED_MEDIA_TYPE))?;
    }

    // Feature: privacy-micro-apps-and-burn-workspace, Property 5: Input tidak valid ditolak tanpa keluaran atau penyimpanan
    #[test]
    fn handler_rejects_corrupted_files(
        j in arb_jpeg_spec(), p in arb_png_spec(), d in arb_pdf_spec(),
        c in arb_corruption(), pick in 0..3usize,
    ) {
        let (f, bytes) = match pick {
            0 => (Format::Jpeg, j.build()),
            1 => (Format::Png, p.build()),
            _ => (Format::Pdf, d.build()),
        };
        let bad = corrupt(f, &bytes, &c);
        run(post_and_check(&bad, expected_status(&c).0))?;
    }
}
