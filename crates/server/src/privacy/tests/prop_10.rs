// Feature: privacy-micro-apps-and-burn-workspace, Property 10: Redaksi mengganti semua rahasia kategori aktif dan menghitungnya
//
// **Validates: Requirements 5.1, 5.3**
use std::collections::BTreeSet;

use proptest::collection::btree_set;
use proptest::prelude::*;

use super::gen_redact::{sentence_with_secrets, Secret, TEXT_CATEGORIES};
use crate::privacy::redactor::{redact, Category, Format, RedactOptions};

/// The part of a secret that must disappear from the output (Bearer keeps its keyword).
fn secret_value(s: &Secret) -> &str {
    match s.category {
        Category::Bearer => s.text.strip_prefix("Bearer ").unwrap_or(&s.text),
        _ => &s.text,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn redact_replaces_active_secrets_and_counts_them(
        sentence in sentence_with_secrets(6),
        active in btree_set(prop::sample::select(TEXT_CATEGORIES.to_vec()), 0..=TEXT_CATEGORIES.len()),
    ) {
        let opts = RedactOptions {
            categories: active.iter().copied().collect::<BTreeSet<_>>(),
            custom_words: Vec::new(),
            format: Format::Text,
        };
        let out = redact(&sentence.text, &opts);

        // counts has exactly the active categories as keys.
        let keys: BTreeSet<Category> = out.counts.keys().copied().collect();
        prop_assert_eq!(&keys, &active);

        for cat in &active {
            let inserted: Vec<&Secret> =
                sentence.secrets.iter().filter(|s| s.category == *cat).collect();

            // Count equals the number of inserted secrets (0 when none).
            prop_assert_eq!(out.counts[cat], inserted.len(), "count for {:?}", cat);

            // Placeholder is present when at least one secret was inserted.
            if !inserted.is_empty() {
                prop_assert!(
                    out.text.contains(&cat.placeholder()),
                    "missing {} in {:?}", cat.placeholder(), out.text
                );
            }

            // No active-category secret value remains.
            for s in inserted {
                prop_assert!(
                    !out.text.contains(secret_value(s)),
                    "secret {:?} leaked in {:?}", s.text, out.text
                );
            }
        }
    }
}
