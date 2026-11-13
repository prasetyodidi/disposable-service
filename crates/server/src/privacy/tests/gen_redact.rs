//! proptest strategies for the secret redactor (`privacy/redactor.rs`).
//!
//! Helpers are `pub(crate)` and consumed by `prop_10` .. `prop_16`.
#![allow(dead_code)]

use std::collections::BTreeSet;

use proptest::collection::{btree_set, vec};
use proptest::prelude::*;

use crate::privacy::redactor::{
    Category, Format, RedactOptions, MAX_CUSTOM_WORDS, MAX_CUSTOM_WORD_CHARS,
};

/// Categories that have a built-in, text-detectable secret generator.
pub(crate) const TEXT_CATEGORIES: [Category; 8] = [
    Category::Jwt,
    Category::Bearer,
    Category::ApiKey,
    Category::Email,
    Category::Phone,
    Category::Ipv4,
    Category::Ipv6,
    Category::CreditCard,
];

/// Neutral filler words: cannot trigger any built-in rule, even next to a secret.
const FILLER: [&str; 14] = [
    "alpha", "server", "started", "user", "logged", "in", "from", "request", "done", "value",
    "ok", "retry", "after", "seconds",
];

/// A generated secret and the category expected to redact it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Secret {
    pub category: Category,
    pub text: String,
}

/// A sentence with secrets inserted at random positions, separated by spaces.
#[derive(Debug, Clone)]
pub(crate) struct Sentence {
    pub text: String,
    pub secrets: Vec<Secret>,
}

/// A generated `.env` file.
#[derive(Debug, Clone)]
pub(crate) struct EnvFile {
    pub text: String,
    /// Variable names in order of appearance.
    pub names: Vec<String>,
    /// Number of assignments with a non-empty value.
    pub non_empty_values: usize,
}

// ---------------------------------------------------------------------------
// Luhn
// ---------------------------------------------------------------------------

/// Appends the Luhn check digit to `body` (digits 0..=9) and returns the full number.
pub(crate) fn luhn_complete(body: &[u8]) -> Vec<u8> {
    // The check digit sits at position 0 from the right, so body digits start at index 1.
    let mut sum = 0u32;
    for (i, d) in body.iter().rev().enumerate() {
        let mut v = *d as u32;
        if i % 2 == 0 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    let check = ((10 - sum % 10) % 10) as u8;
    let mut out = body.to_vec();
    out.push(check);
    out
}

pub(crate) fn luhn_valid(digits: &[u8]) -> bool {
    let mut sum = 0u32;
    for (i, d) in digits.iter().rev().enumerate() {
        let mut v = *d as u32;
        if i % 2 == 1 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    sum % 10 == 0
}

/// Luhn-valid card number of 13..=19 digits, optionally grouped by space or dash.
pub(crate) fn credit_card() -> impl Strategy<Value = String> {
    (
        // First digit non-zero keeps the number from looking like padding.
        1u8..=9,
        vec(0u8..=9, 11..=17),
        prop_oneof![Just(""), Just(" "), Just("-")],
    )
        .prop_map(|(first, rest, sep)| {
            let mut body = vec![first];
            body.extend(rest);
            let digits = luhn_complete(&body);
            let mut s = String::new();
            for (i, d) in digits.iter().enumerate() {
                if i > 0 && i % 4 == 0 {
                    s.push_str(sep);
                }
                s.push(char::from(b'0' + d));
            }
            s
        })
}

// ---------------------------------------------------------------------------
// Individual secrets
// ---------------------------------------------------------------------------

fn b64url(min: usize, max: usize) -> impl Strategy<Value = String> {
    proptest::string::string_regex(&format!("[A-Za-z0-9_-]{{{min},{max}}}")).unwrap()
}

pub(crate) fn jwt_secret() -> impl Strategy<Value = String> {
    (b64url(4, 30), b64url(4, 40), b64url(0, 43))
        .prop_map(|(h, p, s)| format!("eyJ{h}.{p}.{s}"))
}

pub(crate) fn bearer_secret() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[A-Za-z0-9._~+/-]{15,39}[A-Za-z]")
        .unwrap()
        .prop_map(|t| format!("Bearer {t}"))
}

pub(crate) fn api_key_secret() -> impl Strategy<Value = String> {
    prop_oneof![
        proptest::string::string_regex("sk-[A-Za-z0-9]{20,40}").unwrap(),
        proptest::string::string_regex("AKIA[0-9A-Z]{16}").unwrap(),
        proptest::string::string_regex("ghp_[A-Za-z0-9]{24,40}").unwrap(),
    ]
}

pub(crate) fn email_secret() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-z][a-z0-9._]{1,10}@[a-z][a-z0-9]{1,10}\\.(com|org|io|net)")
        .unwrap()
}

pub(crate) fn ipv4_secret() -> impl Strategy<Value = String> {
    (0u8..=255, 0u8..=255, 0u8..=255, 0u8..=255)
        .prop_map(|(a, b, c, d)| format!("{a}.{b}.{c}.{d}"))
}

/// Fully expanded IPv6 (8 groups), which `Ipv6Addr` always accepts.
pub(crate) fn ipv6_secret() -> impl Strategy<Value = String> {
    vec(proptest::string::string_regex("[0-9a-f]{1,4}").unwrap(), 8)
        .prop_map(|groups| groups.join(":"))
}

/// International phone number in compact `+<digits>` form (11..=15 digits).
/// Kept at 12 digits or fewer when it could be mistaken for a card is not needed:
/// cards require >= 13 digits *and* Luhn, so we cap at 12 digits.
pub(crate) fn phone_secret() -> impl Strategy<Value = String> {
    proptest::string::string_regex("\\+[1-9][0-9]{9,11}").unwrap()
}

/// A secret of the given text-detectable category.
pub(crate) fn secret_of(cat: Category) -> BoxedStrategy<String> {
    match cat {
        Category::Jwt => jwt_secret().boxed(),
        Category::Bearer => bearer_secret().boxed(),
        Category::ApiKey => api_key_secret().boxed(),
        Category::Email => email_secret().boxed(),
        Category::Phone => phone_secret().boxed(),
        Category::Ipv4 => ipv4_secret().boxed(),
        Category::Ipv6 => ipv6_secret().boxed(),
        Category::CreditCard => credit_card().boxed(),
        // No standalone text secret exists for these; callers handle them separately.
        Category::EnvValue | Category::Custom => Just(String::new()).boxed(),
    }
}

/// Any text-detectable secret with its category.
pub(crate) fn secret() -> impl Strategy<Value = Secret> {
    prop::sample::select(TEXT_CATEGORIES.to_vec()).prop_flat_map(|category| {
        secret_of(category).prop_map(move |text| Secret { category, text })
    })
}

// ---------------------------------------------------------------------------
// Sentences
// ---------------------------------------------------------------------------

fn filler_words(max: usize) -> impl Strategy<Value = Vec<String>> {
    vec(prop::sample::select(FILLER.to_vec()).prop_map(String::from), 0..=max)
}

/// Random sentence with 0..=`max_secrets` secrets inserted at random positions.
/// Every secret is surrounded by spaces (or string ends), on a single line.
pub(crate) fn sentence_with_secrets(max_secrets: usize) -> impl Strategy<Value = Sentence> {
    (
        filler_words(8),
        vec((secret(), any::<prop::sample::Index>()), 0..=max_secrets),
    )
        .prop_map(|(words, inserts)| {
            let mut tokens: Vec<(String, Option<Secret>)> =
                words.into_iter().map(|w| (w, None)).collect();
            let mut secrets = Vec::new();
            for (s, idx) in inserts {
                let pos = idx.index(tokens.len() + 1);
                tokens.insert(pos, (s.text.clone(), Some(s.clone())));
            }
            // Report secrets in textual order.
            for (_, s) in &tokens {
                if let Some(s) = s {
                    secrets.push(s.clone());
                }
            }
            let text = tokens
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            Sentence { text, secrets }
        })
}

/// Multi-line text: several sentences joined by `\n`. Secrets keep their order.
pub(crate) fn multiline_text(max_lines: usize) -> impl Strategy<Value = Sentence> {
    vec(sentence_with_secrets(3), 1..=max_lines).prop_map(|lines| {
        let text = lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let secrets = lines.into_iter().flat_map(|l| l.secrets).collect();
        Sentence { text, secrets }
    })
}

/// Arbitrary printable-ish text (no secret guarantees), for properties that
/// only need structure such as line count or idempotence.
pub(crate) fn arbitrary_text() -> impl Strategy<Value = String> {
    prop_oneof![
        multiline_text(5).prop_map(|s| s.text),
        proptest::string::string_regex("[ -~\n\t]{0,200}").unwrap(),
    ]
}

// ---------------------------------------------------------------------------
// .env files
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum EnvLine {
    Assign(String, String),
    Comment(String),
    Blank,
}

fn env_line() -> impl Strategy<Value = EnvLine> {
    prop_oneof![
        6 => (
            proptest::string::string_regex("[A-Z_][A-Z0-9_]{0,10}").unwrap(),
            proptest::string::string_regex("[A-Za-z0-9_./:@-]{0,24}").unwrap(),
        )
            .prop_map(|(n, v)| EnvLine::Assign(n, v)),
        2 => proptest::string::string_regex("# [a-z ]{0,20}")
            .unwrap()
            .prop_map(EnvLine::Comment),
        1 => Just(EnvLine::Blank),
    ]
}

/// Random `.env` file with at least one assignment.
pub(crate) fn env_file() -> impl Strategy<Value = EnvFile> {
    (
        proptest::string::string_regex("[A-Z_][A-Z0-9_]{0,10}").unwrap(),
        proptest::string::string_regex("[A-Za-z0-9_./-]{1,24}").unwrap(),
        vec(env_line(), 0..=12),
        any::<bool>(),
    )
        .prop_map(|(n0, v0, lines, trailing_nl)| {
            let mut rendered = vec![format!("{n0}={v0}")];
            let mut names = vec![n0];
            let mut non_empty = 1usize;
            for l in lines {
                match l {
                    EnvLine::Assign(n, v) => {
                        if !v.is_empty() {
                            non_empty += 1;
                        }
                        rendered.push(format!("{n}={v}"));
                        names.push(n);
                    }
                    EnvLine::Comment(c) => rendered.push(c),
                    EnvLine::Blank => rendered.push(String::new()),
                }
            }
            let mut text = rendered.join("\n");
            if trailing_nl {
                text.push('\n');
            }
            EnvFile { text, names, non_empty_values: non_empty }
        })
}

// ---------------------------------------------------------------------------
// Custom words and options
// ---------------------------------------------------------------------------

fn word_piece() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => proptest::string::string_regex("[A-Za-z0-9]{1,6}").unwrap(),
        3 => prop::sample::select(vec![
            ".", "*", "+", "?", "(", ")", "[", "]", "{", "}", "|", "^", "$", "\\", "\\d", ".*",
            "[a-z]", "(a|b)",
        ])
        .prop_map(String::from),
        1 => Just("REDACTED".to_string()),
        1 => Just("\n".to_string()),
        1 => Just(" ".to_string()),
        1 => Just("é".to_string()),
    ]
}

/// One valid custom word: non-empty, <= 256 chars, may hold regex-meta chars,
/// `REDACTED`, or `\n`.
pub(crate) fn custom_word() -> impl Strategy<Value = String> {
    vec(word_piece(), 1..=6).prop_map(|p| p.concat())
}

/// 0..=`max` valid custom words (duplicates allowed).
pub(crate) fn custom_words(max: usize) -> impl Strategy<Value = Vec<String>> {
    vec(custom_word(), 0..=max.min(MAX_CUSTOM_WORDS))
}

/// Custom word lists that `validate` must reject (Property 14).
pub(crate) fn invalid_custom_words() -> impl Strategy<Value = Vec<String>> {
    prop_oneof![
        // More than 100 words.
        (MAX_CUSTOM_WORDS + 1..=MAX_CUSTOM_WORDS + 5)
            .prop_map(|n| (0..n).map(|i| format!("w{i}")).collect::<Vec<_>>()),
        // Contains an empty word.
        (custom_words(5), any::<prop::sample::Index>()).prop_map(|(mut ws, idx)| {
            let at = idx.index(ws.len() + 1);
            ws.insert(at, String::new());
            ws
        }),
        // Contains a word over 256 chars.
        (MAX_CUSTOM_WORD_CHARS + 1..=MAX_CUSTOM_WORD_CHARS + 40, custom_words(3)).prop_map(
            |(len, mut ws)| {
                ws.push("x".repeat(len));
                ws
            }
        ),
    ]
}

pub(crate) fn format_strategy() -> impl Strategy<Value = Format> {
    prop::sample::select(vec![Format::Auto, Format::Env, Format::Text])
}

/// Random subset of all categories (possibly empty).
pub(crate) fn category_set() -> impl Strategy<Value = BTreeSet<Category>> {
    btree_set(prop::sample::select(Category::ALL.to_vec()), 0..=Category::ALL.len())
}

/// Random options with valid custom words.
pub(crate) fn redact_options() -> impl Strategy<Value = RedactOptions> {
    (category_set(), custom_words(10), format_strategy()).prop_map(
        |(categories, custom_words, format)| RedactOptions { categories, custom_words, format },
    )
}

/// Options with every category enabled, the given words, and `Format::Text`.
pub(crate) fn all_categories_text_options(words: Vec<String>) -> RedactOptions {
    RedactOptions {
        categories: Category::ALL.iter().copied().collect(),
        custom_words: words,
        format: Format::Text,
    }
}

#[cfg(test)]
mod self_checks {
    use super::*;
    use crate::privacy::redactor::validate;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn generated_cards_pass_luhn(card in credit_card()) {
            let digits: Vec<u8> = card.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
            prop_assert!((13..=19).contains(&digits.len()));
            prop_assert!(luhn_valid(&digits));
        }

        #[test]
        fn generated_words_are_valid(words in custom_words(20)) {
            let o = all_categories_text_options(words);
            prop_assert!(validate(&o).is_ok());
        }

        #[test]
        fn invalid_words_are_rejected(words in invalid_custom_words()) {
            let o = all_categories_text_options(words);
            prop_assert!(validate(&o).is_err());
        }

        #[test]
        fn env_files_have_assignments(env in env_file()) {
            prop_assert!(!env.names.is_empty());
            prop_assert!(env.non_empty_values >= 1);
        }
    }
}
