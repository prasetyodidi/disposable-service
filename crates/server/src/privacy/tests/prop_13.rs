//! Property 13: Kata kustom diganti dengan pencocokan sensitif huruf besar/kecil.
//!
//! **Validates: Requirements 5.5**
#![allow(dead_code)]

use std::collections::BTreeSet;

use proptest::collection::vec;
use proptest::prelude::*;

use crate::privacy::redactor::{redact, Category, Format, RedactOptions};

/// Distinct custom words that each contain an uppercase letter and never
/// overlap each other (no word is a substring of another).
fn words() -> impl Strategy<Value = Vec<String>> {
    vec(
        proptest::string::string_regex("[A-Z][A-Za-z0-9]{2,7}[.*+?()|]?").unwrap(),
        1..=5,
    )
    .prop_map(|ws| {
        let mut uniq: Vec<String> = Vec::new();
        for w in ws {
            if uniq.iter().all(|u| !u.contains(&w) && !w.contains(u.as_str())) {
                uniq.push(w);
            }
        }
        uniq
    })
}

/// Each token is either a literal custom word (`true`) or its lowercase variant.
fn tokens() -> impl Strategy<Value = (Vec<String>, Vec<(usize, bool)>)> {
    words().prop_flat_map(|ws| {
        let n = ws.len();
        (Just(ws), vec((0..n, any::<bool>()), 0..=12))
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: privacy-micro-apps-and-burn-workspace, Property 13: Kata kustom diganti dengan pencocokan sensitif huruf besar/kecil
    #[test]
    fn custom_words_are_replaced_case_sensitively((ws, toks) in tokens()) {
        let mut parts: Vec<String> = Vec::new();
        let mut expected: Vec<String> = Vec::new();
        let mut literal = 0usize;
        for (i, exact) in &toks {
            if *exact {
                parts.push(ws[*i].clone());
                expected.push(Category::Custom.placeholder());
                literal += 1;
            } else {
                let v = ws[*i].to_lowercase();
                prop_assert_ne!(&v, &ws[*i]);
                parts.push(v.clone());
                expected.push(v);
            }
        }
        let text = parts.join(" ");
        let opts = RedactOptions {
            categories: BTreeSet::from([Category::Custom]),
            custom_words: ws.clone(),
            format: Format::Text,
        };
        let out = redact(&text, &opts);
        prop_assert_eq!(&out.text, &expected.join(" "));
        prop_assert_eq!(out.counts.get(&Category::Custom).copied().unwrap_or(0), literal);
    }
}
