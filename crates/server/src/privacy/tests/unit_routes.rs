//! Unit tests for the scrubber, redactor and JWT HTTP routes.
//!
//! **Validates: Requirements 1.4, 1.7, 2.6, 2.7, 3.2, 3.3, 5.6, 5.8, 6.6, 7.10, 14.2, 14.4**
use super::gen_scrub::{PdfPageSpec, PdfSpec, PngItem, PngSpec};
use super::helpers_http::{authed_app, get, post_json, post_multipart_file, send};
use crate::routes::pandoc::TempDirGuard;
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use base64::prelude::BASE64_URL_SAFE_NO_PAD;
use base64::Engine;
use disposable_common::{
    JwtInspectResult, RedactResponse, ScrubEphemeralResponse, ScrubReport, SignatureCheck,
    TokenStatus,
};
use serde_json::json;

const SCRUB_LIMIT: usize = 26_214_400;
const REDACT_LIMIT: usize = 5_242_880;
const TOKEN_LIMIT: usize = 8_192;

fn unauth_post(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap()
}

/// Valid PNG of exactly `total` bytes (padding lives in IDAT).
fn png_of_size(total: usize) -> Vec<u8> {
    let base = PngSpec { width: 1, height: 1, items: vec![], idat: vec![] }.build().len();
    PngSpec { width: 1, height: 1, items: vec![], idat: vec![7u8; total - base] }.build()
}

fn png_with_text() -> Vec<u8> {
    PngSpec {
        width: 2,
        height: 2,
        items: vec![PngItem::Text { keyword: "Author".into(), text: b"SENSITIVE".to_vec() }],
        idat: vec![1, 2, 3, 4],
    }
    .build()
}

/// Unsigned-style JWT with exactly `target` characters.
fn token_of_len(target: usize) -> String {
    let h = BASE64_URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let b64len = |bytes: usize| (bytes * 4).div_ceil(3);
    for n in 0..target {
        for sig_len in [4usize, 6, 7] {
            if h.len() + 1 + b64len(8 + n) + 1 + sig_len == target {
                let p = BASE64_URL_SAFE_NO_PAD
                    .encode(format!(r#"{{"p":"{}"}}"#, "a".repeat(n)).as_bytes());
                let t = format!("{h}.{p}.{}", "A".repeat(sig_len));
                assert_eq!(t.chars().count(), target);
                return t;
            }
        }
    }
    unreachable!()
}

// ------------------------------- 401 --------------------------------

#[tokio::test]
async fn protected_routes_return_401_without_auth_req_9_5_14_2() {
    let (app, state, _) = authed_app().await;
    for uri in ["/api/apps/scrubber", "/api/apps/redactor", "/api/apps/jwt", "/api/burn"] {
        let (status, _, _) = send(&app, unauth_post(uri)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
    // Invalid bearer is also rejected, and nothing was burned or stored.
    let (status, _, _) =
        send(&app, post_json("/api/apps/jwt", "not-a-session", &json!({"token": "a.b.c"}))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!state.burn.is_burning());
    assert!(state.downloads.read().await.is_empty());
}

// ----------------------------- scrubber -----------------------------

#[tokio::test]
async fn scrubber_accepts_exact_limit_req_1_7_2_6() {
    let (app, _, t) = authed_app().await;
    let png = png_of_size(SCRUB_LIMIT);
    assert_eq!(png.len(), SCRUB_LIMIT);
    let (status, _, body) =
        send(&app, post_multipart_file("/api/apps/scrubber", &t, "big.png", &png)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.len(), SCRUB_LIMIT);
}

#[tokio::test]
async fn scrubber_rejects_one_byte_over_limit_with_413_req_1_7_2_6() {
    let (app, state, t) = authed_app().await;
    let png = png_of_size(SCRUB_LIMIT + 1);
    let (status, _, _) =
        send(&app, post_multipart_file("/api/apps/scrubber", &t, "big.png", &png)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(state.downloads.read().await.is_empty());
}

#[tokio::test]
async fn scrubber_direct_sets_download_and_cache_headers_req_3_3_1_4() {
    let (app, state, t) = authed_app().await;
    let (status, h, body) = send(
        &app,
        post_multipart_file("/api/apps/scrubber", &t, "../My Photo.png", &png_with_text()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h[header::CACHE_CONTROL], "no-store");
    assert_eq!(h[header::CONTENT_TYPE], "image/png");
    let cd = h[header::CONTENT_DISPOSITION].to_str().unwrap();
    assert!(cd.starts_with("attachment;"), "{cd}");
    assert!(cd.contains("scrubbed-My Photo.png"), "{cd}");
    assert!(!body.windows(9).any(|w| w == b"SENSITIVE"));

    let raw = BASE64_URL_SAFE_NO_PAD
        .decode(h["x-scrub-report"].to_str().unwrap())
        .unwrap();
    let report: ScrubReport = serde_json::from_slice(&raw).unwrap();
    assert_eq!(report.file_kind, "png");
    assert_eq!(report.removed.len(), 1);
    // Direct mode keeps nothing server side (Req 3.2).
    assert!(state.downloads.read().await.is_empty());
}

#[tokio::test]
async fn scrubber_ephemeral_is_single_use_with_no_store_req_3_3() {
    let (app, state, t) = authed_app().await;
    let (status, h, body) = send(
        &app,
        post_multipart_file("/api/apps/scrubber?mode=ephemeral", &t, "a.png", &png_with_text()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h[header::CACHE_CONTROL], "no-store");
    let resp: ScrubEphemeralResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.expires_in_secs, 300);

    let (s1, h1, b1) = send(&app, get(&resp.download_url, Some(&t))).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(h1[header::CACHE_CONTROL], "no-store");
    assert!(!b1.windows(9).any(|w| w == b"SENSITIVE"));
    let (s2, _, _) = send(&app, get(&resp.download_url, Some(&t))).await;
    assert_eq!(s2, StatusCode::NOT_FOUND);
    assert!(state.downloads.read().await.is_empty());
}

#[tokio::test]
async fn scrubber_pdf_success_removes_info_req_2_7() {
    let (app, _, t) = authed_app().await;
    let pdf = PdfSpec {
        pages: vec![PdfPageSpec { text: "hello".into(), xmp: false }],
        info: vec![("Author", "SENSAUTHOR123".into())],
        catalog_xmp: false,
    }
    .build();
    let (status, h, body) =
        send(&app, post_multipart_file("/api/apps/scrubber", &t, "d.pdf", &pdf)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h[header::CONTENT_TYPE], "application/pdf");
    assert!(!body.windows(13).any(|w| w == b"SENSAUTHOR123"));
}

#[tokio::test]
async fn scrubber_failure_paths_store_nothing() {
    let (app, state, t) = authed_app().await;
    // Unsupported type -> 415
    let (s, _, _) = send(
        &app,
        post_multipart_file("/api/apps/scrubber?mode=ephemeral", &t, "x.txt", b"plain text"),
    )
    .await;
    assert_eq!(s, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    // Known type but corrupt -> 422
    let png = png_with_text();
    let (s, _, _) = send(
        &app,
        post_multipart_file("/api/apps/scrubber?mode=ephemeral", &t, "x.png", &png[..png.len() / 2]),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    // Missing `file` field -> 400
    let mut req = post_multipart_file("/api/apps/scrubber", &t, "x.png", b"");
    *req.body_mut() = Body::from("------testboundary7f3a--\r\n");
    let (s, _, _) = send(&app, req).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(state.downloads.read().await.is_empty());
}

// ----------------------------- redactor -----------------------------

#[tokio::test]
async fn redactor_boundary_5_mib_req_5_6_5_8() {
    let (app, _, t) = authed_app().await;
    let ok = json!({"text": "a".repeat(REDACT_LIMIT), "categories": ["email"]});
    let (s, _, body) = send(&app, post_json("/api/apps/redactor", &t, &ok)).await;
    assert_eq!(s, StatusCode::OK);
    let resp: RedactResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.text.len(), REDACT_LIMIT);

    let over = json!({"text": "a".repeat(REDACT_LIMIT + 1), "categories": ["email"]});
    let (s, _, _) = send(&app, post_json("/api/apps/redactor", &t, &over)).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn redactor_success_and_failure_paths() {
    let (app, _, t) = authed_app().await;
    let req = json!({"text": "mail bob@example.com now", "categories": ["email"], "custom_words": ["now"]});
    let (s, _, body) = send(&app, post_json("/api/apps/redactor", &t, &req)).await;
    assert_eq!(s, StatusCode::OK);
    let resp: RedactResponse = serde_json::from_slice(&body).unwrap();
    assert!(!resp.text.contains("bob@example.com"));
    assert_eq!(resp.counts.get("email"), Some(&1));

    let bad_cat = json!({"text": "x", "categories": ["nope"]});
    let (s, _, _) = send(&app, post_json("/api/apps/redactor", &t, &bad_cat)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let words: Vec<String> = (0..101).map(|i| format!("w{i}")).collect();
    let too_many = json!({"text": "x", "custom_words": words});
    let (s, _, _) = send(&app, post_json("/api/apps/redactor", &t, &too_many)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

// ------------------------------- JWT --------------------------------

#[tokio::test]
async fn jwt_boundary_8192_chars_req_6_6_7_10() {
    let (app, _, t) = authed_app().await;
    let (s, _, body) = send(
        &app,
        post_json("/api/apps/jwt", &t, &json!({"token": token_of_len(TOKEN_LIMIT)})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let r: JwtInspectResult = serde_json::from_slice(&body).unwrap();
    assert_eq!(r.status, TokenStatus::Active);

    let (s, _, _) = send(
        &app,
        post_json("/api/apps/jwt", &t, &json!({"token": token_of_len(TOKEN_LIMIT + 1)})),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn jwt_success_with_hmac_and_failure_names_segment() {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let (app, _, t) = authed_app().await;
    let h = BASE64_URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let p = BASE64_URL_SAFE_NO_PAD.encode(br#"{"email":"a@b.co","exp":1}"#);
    let mut mac = Hmac::<Sha256>::new_from_slice(b"s3cret").unwrap();
    mac.update(format!("{h}.{p}").as_bytes());
    let sig = BASE64_URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    let token = format!("{h}.{p}.{sig}");

    let (s, _, body) = send(
        &app,
        post_json("/api/apps/jwt", &t, &json!({"token": token, "secret": "s3cret", "redact": true})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let r: JwtInspectResult = serde_json::from_slice(&body).unwrap();
    assert_eq!(r.signature, SignatureCheck::Valid);
    assert_eq!(r.status, TokenStatus::Expired);
    assert!(!r.payload_pretty.contains("a@b.co"));

    let bad = format!("!!!.{p}.{sig}");
    let (s, _, body) = send(&app, post_json("/api/apps/jwt", &t, &json!({"token": bad}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let text = String::from_utf8(body).unwrap();
    assert!(text.contains("header"), "{text}");
    assert!(!text.contains(&p));
}

// ---------------------------- TempDirGuard ---------------------------

async fn job(base: &std::path::Path, fail: bool) -> Result<(), &'static str> {
    let dir = base.join(format!("job-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let _guard = TempDirGuard { path: dir.clone() };
    std::fs::write(dir.join("out.bin"), b"secret").unwrap();
    tokio::task::yield_now().await;
    if fail { Err("failed") } else { Ok(()) }
}

fn is_empty(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir).unwrap().next().is_none()
}

#[tokio::test]
async fn temp_dir_guard_cleans_on_success_failure_and_dropped_future_req_14_4() {
    let base = tempfile::tempdir().unwrap();
    assert!(job(base.path(), false).await.is_ok());
    assert!(is_empty(base.path()));
    assert!(job(base.path(), true).await.is_err());
    assert!(is_empty(base.path()));

    // Future dropped mid-flight (client disconnect): guard still cleans up.
    let dir = base.path().join("job-dropped");
    let d2 = dir.clone();
    let fut = async move {
        std::fs::create_dir_all(&d2).unwrap();
        let _guard = TempDirGuard { path: d2.clone() };
        std::fs::write(d2.join("out.bin"), b"secret").unwrap();
        std::future::pending::<()>().await;
    };
    let r = tokio::time::timeout(std::time::Duration::from_millis(50), fut).await;
    assert!(r.is_err());
    assert!(!dir.exists());
    assert!(is_empty(base.path()));
}
