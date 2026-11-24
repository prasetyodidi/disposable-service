//! Compliance smoke tests for the PDF to Markdown mini app (task 10.4).
//! **Validates: Requirements 7.5, 8.9, 12.5, 13.17, 15.1, 15.3, 15.4, 15.5, 15.6, 15.7**
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};

use crate::routes::create_api_router;
use crate::state::AppState;
use crate::test_support::http::send;

const COMPOSE: &str = include_str!("../../../../../deploy/docker-compose.yml");
const DOCKERFILE: &str = include_str!("../../../../../deploy/Dockerfile");
const DOCKERFILE_LAMBDA: &str = include_str!("../../../../../deploy/Dockerfile.lambda");
const README: &str = include_str!("../../../../../README.md");
const AGENTS: &str = include_str!("../../../../../AGENTS.md");
const SERVER_CARGO: &str = include_str!("../../../Cargo.toml");

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let p = entry.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().map_or(false, |e| e == "rs") {
            out.push(p);
        }
    }
}

#[tokio::test]
async fn pdf_routes_registered_and_other_methods_405() {
    let state = AppState::new();
    let app = create_api_router(state.clone()).with_state(state);
    let base = "/api/apps/pdf-to-markdown";
    let supported = [
        (Method::POST, base.to_string(), vec![Method::GET, Method::PUT, Method::DELETE]),
        (
            Method::GET,
            format!("{base}/download/some-token"),
            vec![Method::POST, Method::PUT, Method::DELETE],
        ),
        (
            Method::GET,
            format!("{base}/limits"),
            vec![Method::POST, Method::PUT, Method::DELETE],
        ),
    ];
    for (method, uri, others) in supported {
        let req = Request::builder()
            .method(method.clone())
            .uri(&uri)
            .body(Body::empty())
            .unwrap();
        let (status, _, _) = send(&app, req).await;
        assert_ne!(status, StatusCode::NOT_FOUND, "{method} {uri} not registered");
        assert_ne!(status, StatusCode::METHOD_NOT_ALLOWED, "{method} {uri}");
        for other in others {
            let req = Request::builder()
                .method(other.clone())
                .uri(&uri)
                .body(Body::empty())
                .unwrap();
            let (status, _, _) = send(&app, req).await;
            assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{other} {uri}");
        }
    }
}

#[test]
fn server_cargo_has_no_poppler_or_database_dependency() {
    let deps = SERVER_CARGO.to_lowercase();
    for banned in [
        "poppler", "sqlite", "rusqlite", "sqlx", "diesel", "postgres", "mysql", "mongodb",
        "sea-orm", "redis", "sled", "rocksdb",
    ] {
        assert!(!deps.contains(banned), "crates/server/Cargo.toml mentions {banned}");
    }
}

#[test]
fn structure_module_does_not_import_poppler_xml_or_proc() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pdf2md/structure");
    let mut files = Vec::new();
    rs_files(&dir, &mut files);
    assert!(!files.is_empty());
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap();
        for (n, line) in src.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            let uses_path = t.starts_with("use ")
                || t.starts_with("pub use ")
                || t.contains("crate::pdf2md::")
                || t.contains("super::super::");
            if !uses_path {
                continue;
            }
            for banned in ["poppler", "xml", "proc"] {
                let hit = t
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .any(|seg| seg == banned || seg == "quick_xml");
                assert!(!hit, "{f:?}:{} imports {banned}: {t}", n + 1);
            }
        }
    }
}

#[test]
fn dockerfiles_install_poppler_utils() {
    assert!(DOCKERFILE.contains("poppler-utils"));
    assert!(DOCKERFILE_LAMBDA.contains("poppler-utils"));
}

#[test]
fn compose_has_pdf_md_variables() {
    for v in [
        "PDF_MD_ENGINE=${PDF_MD_ENGINE:-poppler}",
        "PDF_MD_MAX_BYTES=${PDF_MD_MAX_BYTES:-}",
        "PDF_MD_MAX_RESPONSE_BYTES=${PDF_MD_MAX_RESPONSE_BYTES:-}",
    ] {
        assert!(COMPOSE.contains(v), "compose missing {v}");
    }
}

#[test]
fn docs_cover_all_pdf_items() {
    let needles = [
        "PDF to Markdown",
        "/api/apps/pdf-to-markdown",
        "/api/apps/pdf-to-markdown/download/",
        "/api/apps/pdf-to-markdown/limits",
        "pdf_to_markdown",
        "PDF_MD_ENGINE",
        "PDF_MD_MAX_BYTES",
        "PDF_MD_MAX_RESPONSE_BYTES",
        "25 MB",
        "5 MB",
        "300",
        "GPL",
        "subprocess",
        "OCR",
    ];
    for (name, doc) in [("README.md", README), ("AGENTS.md", AGENTS)] {
        for n in needles {
            assert!(doc.contains(n), "{name} missing {n}");
        }
        assert!(
            doc.contains("30 s") || doc.contains("30 detik") || doc.contains("30 seconds"),
            "{name} missing 30 second limit"
        );
        let lower = doc.to_lowercase();
        assert!(lower.contains("table"), "{name} missing Phase 1 'tables'");
        assert!(
            lower.contains("multi-column") || lower.contains("multi-kolom"),
            "{name} missing Phase 1 'multi-column'"
        );
        assert!(lower.contains("image"), "{name} missing Phase 1 'image extraction'");
    }
}

#[test]
fn docs_env_table_rows_have_defaults_and_descriptions() {
    let rows = [
        ("PDF_MD_ENGINE", "poppler"),
        ("PDF_MD_MAX_BYTES", "26214400"),
        ("PDF_MD_MAX_RESPONSE_BYTES", "5767168"),
    ];
    for (name, doc) in [("README.md", README), ("AGENTS.md", AGENTS)] {
        for (var, default) in rows {
            let row = doc
                .lines()
                .find(|l| l.trim_start().starts_with('|') && l.contains(&format!("`{var}`")))
                .unwrap_or_else(|| panic!("{name}: no table row for {var}"));
            let cells: Vec<&str> = row
                .trim()
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            assert!(cells.len() >= 3, "{name}: {var} row has fewer than 3 cells");
            assert!(cells.iter().all(|c| !c.is_empty()), "{name}: {var} has empty cell");
            assert!(row.contains(default), "{name}: {var} row lacks default {default}");
        }
    }
}

/// True if `s` contains `#` followed by 3/4/6/8 hex digits and a non-alphanumeric boundary.
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
fn pdf_component_has_no_hex_colors() {
    let f = workspace_root().join("frontend/src/components/PdfToMarkdown.tsx");
    let src = std::fs::read_to_string(&f).expect("PdfToMarkdown.tsx");
    assert!(!has_hex_color(&src), "hex color literal in PdfToMarkdown.tsx");
}

#[test]
fn cli_does_not_reference_pdf_converter_or_burn() {
    let dir = workspace_root().join("crates/cli/src");
    let mut files = Vec::new();
    rs_files(&dir, &mut files);
    assert!(!files.is_empty());
    for p in files {
        let src = std::fs::read_to_string(&p).unwrap().to_lowercase();
        for banned in ["pdf2md", "pdf_to_markdown", "pdf-to-markdown", "pdftohtml", "pdfconverter", "burn"] {
            assert!(!src.contains(banned), "{p:?} references {banned}");
        }
    }
}
