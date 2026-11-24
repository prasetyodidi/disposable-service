//! Unit tests for the Engine_Registry. Requirements: 7.2, 7.3, 7.4, 7.6, 16.11.
//! None of these tests need poppler: the built-in engine is only selected, never run.
use super::mock_engine::MockEngine;
use crate::pdf2md::engine::{EngineRegistry, ExtractRequest, DEFAULT_ENGINE, ENV_ENGINE};
use crate::pdf2md::model::{IntermediateDoc, Page};
use crate::pdf2md::proc::Deadline;
use std::path::Path;
use std::time::Duration;

#[test]
fn unset_empty_or_blank_selects_poppler() {
    let reg = EngineRegistry::with_builtin();
    for raw in [None, Some(""), Some("   "), Some("\t\n")] {
        let e = reg.select(raw).expect("default engine");
        assert_eq!(e.name(), "poppler", "raw={raw:?}");
    }
    assert_eq!(DEFAULT_ENGINE, "poppler");
}

#[test]
fn unknown_value_is_rejected_with_env_name_and_valid_list() {
    let mut reg = EngineRegistry::with_builtin();
    reg.register("mock", MockEngine::arc(super::mock_engine::MockOutcome::Sleep(Duration::ZERO)));
    let err = match reg.select(Some("nope")) {
        Ok(_) => panic!("unknown engine must be rejected"),
        Err(e) => e,
    };
    assert_eq!(err.given, "nope");
    assert_eq!(err.valid, vec!["mock".to_string(), "poppler".to_string()]);
    let msg = err.to_string();
    assert!(msg.contains(ENV_ENGINE), "{msg}");
    assert!(msg.contains("PDF_MD_ENGINE"), "{msg}");
    assert!(msg.contains("poppler") && msg.contains("mock"), "{msg}");
}

#[tokio::test]
async fn registered_mock_engine_is_selected_and_converts_without_poppler() {
    let doc = IntermediateDoc {
        total_pages: 1,
        first_page: 1,
        last_page: 1,
        pages: vec![Page { number: 1, width: 600, height: 800, lines: vec![] }],
    };
    let mock = MockEngine::doc(doc.clone());
    let mut reg = EngineRegistry::with_builtin();
    reg.register("mock", mock.clone());

    // Name matching is trimmed and case-insensitive.
    let engine = reg.select(Some("  MOCK ")).expect("mock registered");
    assert_eq!(engine.name(), "mock");

    let req = ExtractRequest {
        pdf_path: Path::new("/nonexistent/in.pdf"),
        work_dir: Path::new("/nonexistent"),
        page_from: 1,
        page_to: None,
        max_pages: 10,
        deadline: Deadline::new(Duration::from_secs(5)),
        max_output_bytes: 1024,
    };
    let got = engine.extract(&req).await.expect("mock result");
    assert_eq!(got, doc);
    assert_eq!(mock.calls(), 1);
}
