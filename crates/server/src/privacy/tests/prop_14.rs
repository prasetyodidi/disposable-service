//! Property 14: custom word list validation.
//! **Validates: Requirements 5.7**

use proptest::prelude::*;

use super::gen_redact::{all_categories_text_options, custom_words};
use crate::privacy::redactor::{
    validate, RedactError, MAX_CUSTOM_WORDS, MAX_CUSTOM_WORD_CHARS,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: privacy-micro-apps-and-burn-workspace, Property 14: Validasi daftar kata kustom
    #[test]
    fn too_many_words_rejected(extra in 1usize..=20, base in custom_words(5)) {
        let mut words = base;
        words.resize(MAX_CUSTOM_WORDS + extra, "w".to_string());
        let err = validate(&all_categories_text_options(words)).unwrap_err();
        prop_assert_eq!(err, RedactError::TooManyWords);
        prop_assert!(err.to_string().contains(&MAX_CUSTOM_WORDS.to_string()));
    }

    // Feature: privacy-micro-apps-and-burn-workspace, Property 14: Validasi daftar kata kustom
    #[test]
    fn empty_word_rejected(mut words in custom_words(20), idx in any::<prop::sample::Index>()) {
        let at = idx.index(words.len() + 1);
        words.insert(at, String::new());
        let err = validate(&all_categories_text_options(words)).unwrap_err();
        prop_assert_eq!(err, RedactError::EmptyWord);
        prop_assert!(err.to_string().contains("kosong"));
    }

    // Feature: privacy-micro-apps-and-burn-workspace, Property 14: Validasi daftar kata kustom
    #[test]
    fn too_long_word_rejected(
        extra in 1usize..=50,
        mut words in custom_words(20),
        idx in any::<prop::sample::Index>(),
    ) {
        let at = idx.index(words.len() + 1);
        words.insert(at, "é".repeat(MAX_CUSTOM_WORD_CHARS + extra));
        words.truncate(MAX_CUSTOM_WORDS);
        if !words.iter().any(|w| w.chars().count() > MAX_CUSTOM_WORD_CHARS) {
            return Ok(());
        }
        let err = validate(&all_categories_text_options(words)).unwrap_err();
        prop_assert_eq!(err, RedactError::WordTooLong);
        prop_assert!(err.to_string().contains(&MAX_CUSTOM_WORD_CHARS.to_string()));
    }

    // Feature: privacy-micro-apps-and-burn-workspace, Property 14: Validasi daftar kata kustom
    #[test]
    fn valid_lists_accepted(words in custom_words(MAX_CUSTOM_WORDS)) {
        prop_assert!(validate(&all_categories_text_options(words)).is_ok());
    }

    // Feature: privacy-micro-apps-and-burn-workspace, Property 14: Validasi daftar kata kustom
    #[test]
    fn boundary_lists_accepted(n in 1usize..=MAX_CUSTOM_WORDS, len in 1usize..=MAX_CUSTOM_WORD_CHARS) {
        let words = vec!["é".repeat(len); n];
        prop_assert!(validate(&all_categories_text_options(words)).is_ok());
    }
}
