// Feature: pdf-to-markdown, Property 28: Token tidak valid dijawab seragam
// **Validates: Requirements 10.5**
use chrono::{Duration, Utc};
use proptest::prelude::*;

use super::helpers::{authed_pdf_app, test_runtime};
use super::mock_engine::{MockEngine, MockOutcome};
use crate::pdf2md::model::IntermediateDoc;
use crate::state::EphemeralDownload;
use crate::test_support::http::{get, send};

const BASE: &str = "/api/apps/pdf-to-markdown/download";

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

#[derive(Debug, Clone)]
enum Category {
    RandomString(String),
    UnknownUuid(u128),
    Used(Vec<u8>),
    Expired(Vec<u8>, i64),
    Foreign(Vec<u8>, bool),
}

fn arb_category() -> impl Strategy<Value = Category> {
    let data = proptest::collection::vec(any::<u8>(), 0..64);
    prop_oneof![
        "[A-Za-z0-9_-]{1,64}".prop_map(Category::RandomString),
        any::<u128>().prop_map(Category::UnknownUuid),
        data.clone().prop_map(Category::Used),
        (data.clone(), 1i64..100_000).prop_map(|(d, s)| Category::Expired(d, s)),
        (data, any::<bool>()).prop_map(|(d, img)| Category::Foreign(d, img)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn invalid_tokens_get_identical_404(cats in proptest::collection::vec(arb_category(), 1..6)) {
        let dir = tempfile::tempdir().unwrap();
        rt().block_on(async {
            let engine = MockEngine::arc(MockOutcome::Doc(IntermediateDoc::default()));
            let (app, state, bearer) =
                authed_pdf_app(test_runtime(engine, dir.path())).await;

            // Reference body: a UUID that was never created.
            let ref_uri = format!("{BASE}/{}", uuid::Uuid::new_v4());
            let (ref_status, _, ref_body) = send(&app, get(&ref_uri, Some(&bearer))).await;
            prop_assert_eq!(ref_status.as_u16(), 404);

            let mut foreign: Vec<(String, Vec<u8>)> = Vec::new();
            for cat in cats {
                let token = match cat {
                    Category::RandomString(s) => s,
                    Category::UnknownUuid(n) => uuid::Uuid::from_u128(n).to_string(),
                    Category::Used(d) => {
                        let t = state
                            .store_ephemeral_download(d, "document.md".into(), "text/markdown; charset=utf-8".into(), 300)
                            .await;
                        let (s1, _, _) = send(&app, get(&format!("{BASE}/{t}"), Some(&bearer))).await;
                        prop_assert_eq!(s1.as_u16(), 200);
                        t
                    }
                    Category::Expired(d, secs) => {
                        let t = uuid::Uuid::new_v4().to_string();
                        state.downloads.write().await.insert(t.clone(), EphemeralDownload {
                            data: d,
                            filename: "document.md".into(),
                            content_type: "text/markdown; charset=utf-8".into(),
                            expires_at: Utc::now() - Duration::seconds(secs),
                        });
                        t
                    }
                    Category::Foreign(d, img) => {
                        let ct = if img { "image/png" } else { "application/pdf" };
                        let t = state
                            .store_ephemeral_download(d.clone(), "x.bin".into(), ct.into(), 300)
                            .await;
                        foreign.push((t.clone(), d));
                        t
                    }
                };
                let (status, _, body) = send(&app, get(&format!("{BASE}/{token}"), Some(&bearer))).await;
                prop_assert_eq!(status.as_u16(), 404);
                prop_assert_eq!(&body, &ref_body);
            }

            // Entries of other features survive every request, untouched.
            let lock = state.downloads.read().await;
            for (t, d) in foreign {
                let e = lock.get(&t);
                prop_assert!(e.is_some(), "foreign entry removed");
                prop_assert_eq!(&e.unwrap().data, &d);
            }
            Ok(())
        })?;
    }
}
