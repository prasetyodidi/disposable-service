//! Compliance smoke tests (task 13.3).
//! Validates: Requirements 4.7, 12.2, 14.3, 14.6, 14.7
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};

use super::helpers_http::send;
use crate::routes::create_api_router;
use crate::state::AppState;

const COMPOSE: &str = include_str!("../../../../../deploy/docker-compose.yml");
const README: &str = include_str!("../../../../../README.md");
const AGENTS: &str = include_str!("../../../../../AGENTS.md");

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[tokio::test]
async fn new_routes_are_registered_not_404() {
    let state = AppState::new();
    let app = create_api_router(state.clone()).with_state(state);
    let routes = [
        (Method::POST, "/api/apps/scrubber"),
        (Method::GET, "/api/apps/scrubber/download/some-token"),
        (Method::POST, "/api/apps/redactor"),
        (Method::POST, "/api/apps/jwt"),
        (Method::POST, "/api/burn"),
        (Method::GET, "/api/burn/events"),
    ];
    for (method, uri) in routes {
        let req = Request::builder()
            .method(method.clone())
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let (status, _, body) = send(&app, req).await;
        // The download route legitimately answers 404 (JSON body) for an unknown
        // token; an unregistered route yields an empty 404 body.
        let handler_404 = uri.contains("/download/") && !body.is_empty();
        if !handler_404 {
            assert_ne!(status, StatusCode::NOT_FOUND, "{method} {uri} returned 404");
        }
        assert_ne!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {uri} not registered for method"
        );
    }
}

#[test]
fn compose_has_on_failure_default_and_aligned_env() {
    assert!(COMPOSE.contains("restart: ${RESTART_POLICY:-on-failure}"));
    assert!(COMPOSE.contains("DISPOSABLE_RESTART_POLICY=${RESTART_POLICY:-on-failure}"));
}

#[test]
fn docs_mention_routes_and_tools() {
    let needles = [
        "/api/apps/scrubber",
        "/api/apps/redactor",
        "/api/apps/jwt",
        "/api/burn",
        "/api/burn/events",
        "redact_secrets",
        "inspect_jwt",
        "scrub_metadata",
        "burn_workspace_now",
        "RESTART_POLICY",
    ];
    for n in needles {
        assert!(README.contains(n), "README.md missing {n}");
        assert!(AGENTS.contains(n), "AGENTS.md missing {n}");
    }
}

/// Returns true if `s` contains `#` followed by 3/4/6/8 hex digits and a non-alphanumeric boundary.
fn has_hex_color(s: &str) -> bool {
    let b = s.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c != b'#' {
            continue;
        }
        let run = b[i + 1..].iter().take_while(|x| x.is_ascii_hexdigit()).count();
        let next = b.get(i + 1 + run).copied();
        let boundary = next.map_or(true, |x| !x.is_ascii_alphanumeric() && x != b'_');
        if matches!(run, 3 | 4 | 6 | 8) && boundary {
            return true;
        }
    }
    false
}

#[test]
fn hex_color_detector_sanity() {
    assert!(has_hex_color("color: #fff;"));
    assert!(has_hex_color("'#A1b2C3'"));
    assert!(!has_hex_color("href=\"#section-title\""));
    assert!(!has_hex_color("var(--color-x)"));
}

#[test]
fn new_frontend_components_have_no_hex_colors() {
    let dir = workspace_root().join("frontend/src/components");
    let fixed = [
        "MetadataScrubber.tsx",
        "RedactorPage.tsx",
        "JwtInspectorTab.tsx",
        "LogRedactorTab.tsx",
        "AuditScreen.tsx",
    ];
    let mut files: Vec<PathBuf> = fixed.iter().map(|f| dir.join(f)).collect();
    for entry in std::fs::read_dir(&dir).expect("components dir") {
        let p = entry.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if name.starts_with("Burn") && name.ends_with(".tsx") && !name.contains(".test.") {
            files.push(p);
        }
    }
    assert!(files.iter().any(|f| f.to_string_lossy().contains("Burn")));
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap_or_else(|_| panic!("missing {f:?}"));
        assert!(!has_hex_color(&src), "hex color literal in {f:?}");
    }
}

#[test]
fn cli_teardown_does_not_use_burn_service() {
    let dir = workspace_root().join("crates/cli/src");
    for entry in std::fs::read_dir(&dir).expect("cli src") {
        let p = entry.unwrap().path();
        if p.extension().map_or(false, |e| e == "rs") {
            let src = std::fs::read_to_string(&p).unwrap().to_lowercase();
            assert!(!src.contains("burn"), "{p:?} references burn");
        }
    }
}
