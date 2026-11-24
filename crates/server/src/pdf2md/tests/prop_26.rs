// Feature: pdf-to-markdown, Property 26: Pemilihan mesin
// **Validates: Requirements 7.2, 7.3, 7.4**
use proptest::prelude::*;
use std::sync::Arc;

use super::mock_engine::{MockEngine, MockOutcome};
use crate::pdf2md::engine::{EngineRegistry, ExtractionEngine, DEFAULT_ENGINE, ENV_ENGINE};
use crate::pdf2md::model::IntermediateDoc;

fn same(a: &Arc<dyn ExtractionEngine>, b: &Arc<dyn ExtractionEngine>) -> bool {
    Arc::as_ptr(a) as *const () == Arc::as_ptr(b) as *const ()
}

fn registry_with(extra: &[String]) -> (EngineRegistry, Arc<dyn ExtractionEngine>, Vec<Arc<dyn ExtractionEngine>>) {
    let mut reg = EngineRegistry::with_builtin();
    let poppler = reg.select(None).unwrap();
    let mut others = Vec::new();
    for name in extra {
        let e: Arc<dyn ExtractionEngine> =
            MockEngine::arc(MockOutcome::Doc(IntermediateDoc::default()));
        reg.register(name, e.clone());
        others.push(e);
    }
    (reg, poppler, others)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Arbitrary strings: blank -> poppler; registered (trim+lowercase) -> that engine;
    /// otherwise an error naming PDF_MD_ENGINE and every valid name.
    #[test]
    fn select_arbitrary_string(v in ".{0,24}", extra in proptest::collection::btree_set("[a-z][a-z0-9_-]{0,8}", 0..3)) {
        let extra: Vec<String> = extra.into_iter().filter(|n| n != DEFAULT_ENGINE).collect();
        let (reg, poppler, _others) = registry_with(&extra);
        let key = v.trim().to_lowercase();
        let res = reg.select(Some(&v));
        if v.trim().is_empty() || key == DEFAULT_ENGINE {
            prop_assert!(same(&res.unwrap(), &poppler));
        } else if extra.contains(&key) {
            let got = res.unwrap();
            prop_assert!(!same(&got, &poppler));
        } else {
            let err = res.err().expect("must be error");
            let msg = err.to_string();
            prop_assert!(msg.contains(ENV_ENGINE));
            for n in reg.valid_names() {
                prop_assert!(msg.contains(&n), "missing {} in {}", n, msg);
            }
        }
    }

    /// Case and surrounding whitespace variants of registered names select that engine.
    #[test]
    fn select_registered_variants(
        name in "[a-z][a-z0-9_-]{0,8}",
        pre in "[ \t\n]{0,3}",
        post in "[ \t\n]{0,3}",
        upper in any::<bool>(),
    ) {
        prop_assume!(name != DEFAULT_ENGINE);
        let (reg, poppler, others) = registry_with(&[name.clone()]);
        let body = if upper { name.to_uppercase() } else { name.clone() };
        let got = reg.select(Some(&format!("{pre}{body}{post}"))).unwrap();
        prop_assert!(same(&got, &others[0]));
        let pv = format!("{pre}{}{post}", if upper { "POPPLER" } else { "poppler" });
        prop_assert!(same(&reg.select(Some(&pv)).unwrap(), &poppler));
    }

    /// Whitespace-only and None select poppler.
    #[test]
    fn select_blank_is_poppler(ws in "[ \t\r\n]{0,6}") {
        let (reg, poppler, _o) = registry_with(&[]);
        prop_assert!(same(&reg.select(Some(&ws)).unwrap(), &poppler));
        prop_assert!(same(&reg.select(None).unwrap(), &poppler));
        prop_assert_eq!(poppler.name(), DEFAULT_ENGINE);
    }
}
