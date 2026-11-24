//! Property 12: Kategori nonaktif tidak disentuh.
//! **Validates: Requirements 5.4**
use std::collections::BTreeSet;

use proptest::prelude::*;

use super::gen_redact::{
    arbitrary_text, custom_words, format_strategy, multiline_text, TEXT_CATEGORIES,
};
use crate::privacy::redactor::{redact, Category, Format, RedactOptions};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: privacy-micro-apps-and-burn-workspace, Property 12: Kategori nonaktif tidak disentuh
    #[test]
    fn inactive_category_secrets_remain_verbatim(
        s in multiline_text(4),
        active in prop::collection::btree_set(prop::sample::select(TEXT_CATEGORIES.to_vec()), 0..=TEXT_CATEGORIES.len()),
    ) {
        let opts = RedactOptions {
            categories: active.iter().copied().collect::<BTreeSet<_>>(),
            custom_words: Vec::new(),
            format: Format::Text,
        };
        let out = redact(&s.text, &opts);
        for sec in s.secrets.iter().filter(|x| !active.contains(&x.category)) {
            prop_assert!(
                out.text.contains(&sec.text),
                "inactive {:?} secret {:?} was altered in {:?}", sec.category, sec.text, out.text
            );
        }
        for c in Category::ALL {
            if !active.contains(&c) {
                prop_assert!(!out.counts.contains_key(&c));
                prop_assert!(!out.text.contains(&c.placeholder()));
            }
        }
    }

    // Feature: privacy-micro-apps-and-burn-workspace, Property 12: Kategori nonaktif tidak disentuh
    #[test]
    fn empty_category_set_returns_original(
        text in arbitrary_text(),
        format in format_strategy(),
    ) {
        let opts = RedactOptions {
            categories: BTreeSet::new(),
            custom_words: Vec::new(),
            format,
        };
        let out = redact(&text, &opts);
        prop_assert_eq!(out.text, text);
        prop_assert!(out.counts.is_empty());
    }

    // Feature: privacy-micro-apps-and-burn-workspace, Property 12: Kategori nonaktif tidak disentuh
    #[test]
    fn inactive_custom_category_ignores_words(
        text in arbitrary_text(),
        words in custom_words(10),
        format in format_strategy(),
    ) {
        // Isolate Custom: with no other category active the text must be untouched.
        let opts = RedactOptions { categories: BTreeSet::new(), custom_words: words, format };
        let out = redact(&text, &opts);
        prop_assert_eq!(out.text, text);
    }
}
