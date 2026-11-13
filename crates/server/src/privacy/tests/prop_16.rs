// Feature: privacy-micro-apps-and-burn-workspace, Property 16: Redaksi mempertahankan jumlah baris
//
// **Validates: Requirements 5.10**

use proptest::prelude::*;

use super::gen_redact::{arbitrary_text, custom_words, redact_options};
use crate::privacy::redactor::{redact, RedactOptions};

fn newlines(s: &str) -> usize {
    s.chars().filter(|c| *c == '\n').count()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Any text and any options: output has the same number of `\n` as input.
    #[test]
    fn redaction_preserves_line_count(text in arbitrary_text(), opts in redact_options()) {
        let out = redact(&text, &opts);
        prop_assert_eq!(newlines(&out.text), newlines(&text));
    }

    /// Custom words (often containing `\n`) embedded in the text, all categories on.
    #[test]
    fn custom_words_with_newlines_preserve_line_count(
        words in custom_words(10),
        base in arbitrary_text(),
        picks in proptest::collection::vec(any::<prop::sample::Index>(), 0..=6),
        base_opts in redact_options(),
    ) {
        let mut text = base;
        for p in &picks {
            if words.is_empty() { break; }
            let w = &words[p.index(words.len())];
            text.push_str(" ");
            text.push_str(w);
            text.push('\n');
        }
        let opts = RedactOptions { custom_words: words, ..base_opts };
        let out = redact(&text, &opts);
        prop_assert_eq!(newlines(&out.text), newlines(&text));
    }
}
