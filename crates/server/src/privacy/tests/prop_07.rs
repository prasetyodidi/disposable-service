//! Property 7: Tidak ada konten pengguna di log.
//!
//! Tracing output is captured (thread-scoped `with_default`, TRACE level, in a
//! buffer) while the scrubber, redactor and JWT handlers process successful
//! and failing requests through the real router (with `TraceLayer`, as in
//! `main.rs`). Unique markers placed in filename, file content, text, token,
//! secret and custom words must never appear in the captured output.
//!
//! Note: `spawn_blocking` work (scrubber core) runs on other threads and is not
//! covered by the thread-scoped subscriber; handler/middleware logs are.

// Feature: privacy-micro-apps-and-burn-workspace, Property 7: Tidak ada konten pengguna di log
// **Validates: Requirements 3.6, 5.11, 6.11**

use std::io;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use proptest::prelude::*;
use tower::ServiceExt;
use tower_http::trace::TraceLayer;
use tracing_subscriber::fmt::MakeWriter;

use crate::privacy::tests::gen_scrub::png_chunk;
use crate::routes::create_api_router;
use crate::state::AppState;
use disposable_common::SessionInfo;

#[derive(Clone, Default)]
struct BufWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for BufWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for BufWriter {
    type Writer = BufWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

const SESSION: &str = "prop07-session";

fn app(state: AppState) -> axum::Router {
    create_api_router(state.clone())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn send(app: &axum::Router, req: Request<Body>) -> StatusCode {
    let resp = app.clone().oneshot(req).await.expect("infallible");
    let status = resp.status();
    let _ = axum::body::to_bytes(resp.into_body(), usize::MAX).await;
    status
}

fn json_req(uri: &str, auth: bool, body: serde_json::Value) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if auth {
        b = b.header(header::AUTHORIZATION, format!("Bearer {SESSION}"));
    }
    b.body(Body::from(body.to_string())).unwrap()
}

fn multipart_req(uri: &str, auth: bool, filename: &str, content: &[u8]) -> Request<Body> {
    let boundary = "prop07boundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let mut b = Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        );
    if auth {
        b = b.header(header::AUTHORIZATION, format!("Bearer {SESSION}"));
    }
    b.body(Body::from(body)).unwrap()
}

/// Minimal valid 1x1 PNG with a tEXt chunk carrying `marker` (user content).
fn png_with_text(marker: &str) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    out.extend(png_chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]));
    let mut text = b"Comment\0".to_vec();
    text.extend_from_slice(marker.as_bytes());
    out.extend(png_chunk(b"tEXt", &text));
    out.extend(png_chunk(
        b"IDAT",
        &[0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00],
    ));
    out.extend(png_chunk(b"IEND", &[]));
    out
}

fn arb_marker(tag: &'static str) -> impl Strategy<Value = String> {
    "[a-z0-9]{10,16}".prop_map(move |s| format!("zq{tag}{s}"))
}

#[derive(Debug, Clone)]
struct Markers {
    filename: String,
    content: String,
    text: String,
    word: String,
    token_claim: String,
    secret: String,
    junk: String,
}

fn arb_markers() -> impl Strategy<Value = Markers> {
    (
        arb_marker("fn"),
        arb_marker("ct"),
        arb_marker("tx"),
        arb_marker("cw"),
        arb_marker("tk"),
        arb_marker("sc"),
        arb_marker("jk"),
    )
        .prop_map(|(filename, content, text, word, token_claim, secret, junk)| Markers {
            filename,
            content,
            text,
            word,
            token_claim,
            secret,
            junk,
        })
}

fn run_case(m: &Markers) -> (Vec<String>, Vec<StatusCode>) {
    let state = AppState::new();
    let writer = BufWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(writer.clone())
        .finish();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // JWT material: markers inside payload; token is built from them.
    let header_b64 = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload_json = serde_json::json!({
        "sub": m.token_claim,
        "email": format!("{}@example.com", m.token_claim),
    })
    .to_string();
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(m.junk.as_bytes());
    let token = format!("{header_b64}.{payload_b64}.{sig_b64}");

    let statuses = tracing::subscriber::with_default(subscriber, || {
        rt.block_on(async {
            state
                .add_session(SessionInfo {
                    session_id: SESSION.to_string(),
                    ip_address: "127.0.0.1".into(),
                    user_agent: "test".into(),
                    login_time: 0,
                    expires_at: chrono::Utc::now().timestamp() + 3600,
                    token_type: None,
                    label: None,
                })
                .await;
            let app = app(state.clone());
            let png = png_with_text(&m.content);
            let fname = format!("{}.png", m.filename);
            let mut st = Vec::new();

            // Scrubber: success (direct + ephemeral), unsupported type, corrupt PNG, unauth.
            st.push(send(&app, multipart_req("/api/apps/scrubber", true, &fname, &png)).await);
            st.push(
                send(
                    &app,
                    multipart_req("/api/apps/scrubber?mode=ephemeral", true, &fname, &png),
                )
                .await,
            );
            st.push(
                send(
                    &app,
                    multipart_req("/api/apps/scrubber", true, &fname, m.content.as_bytes()),
                )
                .await,
            );
            let mut broken = png.clone();
            broken.truncate(broken.len() - 20);
            broken.extend_from_slice(m.content.as_bytes());
            st.push(send(&app, multipart_req("/api/apps/scrubber", true, &fname, &broken)).await);
            st.push(send(&app, multipart_req("/api/apps/scrubber", false, &fname, &png)).await);

            // Redactor: success, custom words, 400 (bad category / too many words), unauth.
            let text = format!("line {} mail a@b.co\nnext {}\n", m.text, m.word);
            st.push(
                send(
                    &app,
                    json_req(
                        "/api/apps/redactor",
                        true,
                        serde_json::json!({"text": text, "custom_words": [m.word]}),
                    ),
                )
                .await,
            );
            st.push(
                send(
                    &app,
                    json_req(
                        "/api/apps/redactor",
                        true,
                        serde_json::json!({"text": text, "categories": [m.junk]}),
                    ),
                )
                .await,
            );
            let many: Vec<String> = (0..101).map(|i| format!("{}{i}", m.word)).collect();
            st.push(
                send(
                    &app,
                    json_req(
                        "/api/apps/redactor",
                        true,
                        serde_json::json!({"text": text, "custom_words": many}),
                    ),
                )
                .await,
            );
            st.push(
                send(
                    &app,
                    json_req("/api/apps/redactor", false, serde_json::json!({"text": text})),
                )
                .await,
            );

            // JWT: success (with secret, redact), malformed token, oversize secret, unauth.
            st.push(
                send(
                    &app,
                    json_req(
                        "/api/apps/jwt",
                        true,
                        serde_json::json!({"token": token, "secret": m.secret, "redact": true}),
                    ),
                )
                .await,
            );
            st.push(
                send(
                    &app,
                    json_req(
                        "/api/apps/jwt",
                        true,
                        serde_json::json!({"token": format!("{}.{}", m.token_claim, m.junk)}),
                    ),
                )
                .await,
            );
            st.push(
                send(
                    &app,
                    json_req(
                        "/api/apps/jwt",
                        true,
                        serde_json::json!({"token": token, "secret": m.secret.repeat(100)}),
                    ),
                )
                .await,
            );
            st.push(
                send(
                    &app,
                    json_req("/api/apps/jwt", false, serde_json::json!({"token": token})),
                )
                .await,
            );
            st
        })
    });

    let captured = String::from_utf8_lossy(&writer.0.lock().unwrap()).into_owned();
    let mut leaks = Vec::new();
    let needles = [
        ("filename", m.filename.clone()),
        ("content", m.content.clone()),
        ("text", m.text.clone()),
        ("custom_word", m.word.clone()),
        ("token_claim", m.token_claim.clone()),
        ("secret", m.secret.clone()),
        ("junk", m.junk.clone()),
        ("token", token.clone()),
        ("token_payload", payload_b64.clone()),
        ("token_signature", sig_b64.clone()),
        ("content_b64", URL_SAFE_NO_PAD.encode(m.content.as_bytes())),
    ];
    for (name, needle) in needles {
        if captured.contains(&needle) {
            leaks.push(name.to_string());
        }
    }
    // Sanity: capturing works (TraceLayer logs at least request start lines).
    if captured.is_empty() {
        leaks.push("capture-empty".to_string());
    }
    (leaks, statuses)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 100, ..ProptestConfig::default() })]

    #[test]
    fn prop_no_user_content_in_logs(m in arb_markers()) {
        let (leaks, statuses) = run_case(&m);
        prop_assert!(leaks.is_empty(), "leaked into logs: {:?}", leaks);
        // Success and failure paths were both exercised.
        prop_assert_eq!(statuses[0], StatusCode::OK);
        prop_assert_eq!(statuses[2], StatusCode::UNSUPPORTED_MEDIA_TYPE);
        prop_assert_eq!(statuses[4], StatusCode::UNAUTHORIZED);
        prop_assert_eq!(statuses[5], StatusCode::OK);
        prop_assert_eq!(statuses[6], StatusCode::BAD_REQUEST);
        prop_assert_eq!(statuses[9], StatusCode::OK);
        prop_assert_eq!(statuses[10], StatusCode::UNPROCESSABLE_ENTITY);
    }
}
