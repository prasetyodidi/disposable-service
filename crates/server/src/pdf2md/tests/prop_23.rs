// Feature: pdf-to-markdown, Property 23: Klasifikasi stderr
// **Validates: Requirements 8.7**
use proptest::collection::vec;
use proptest::prelude::*;

use crate::pdf2md::error::EngineError;
use crate::pdf2md::poppler::classify_stderr;

/// Case-insensitive pattern check, written independently of the implementation.
fn expects_encrypted(bytes: &[u8]) -> bool {
    let s = String::from_utf8_lossy(bytes).to_lowercase();
    s.contains("incorrect password") || (s.contains("password") && s.contains("encrypted"))
}

fn randomize_case(s: &str, flips: &[bool]) -> String {
    s.chars()
        .enumerate()
        .map(|(i, c)| {
            if flips.get(i).copied().unwrap_or(false) {
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// Fragments mixing password patterns, structural-error patterns, and noise.
fn arb_stderr() -> impl Strategy<Value = Vec<u8>> {
    let frag = prop_oneof![
        Just("Incorrect password".to_string()),
        Just("incorrect  password".to_string()),
        Just("password".to_string()),
        Just("encrypted".to_string()),
        Just("Command Line Error: ".to_string()),
        Just("Syntax Error: Couldn't read xref".to_string()),
        Just("May not be a PDF file".to_string()),
        Just("Couldn't find trailer dictionary".to_string()),
        "[ -~\\n]{0,20}",
    ];
    (vec((frag, vec(any::<bool>(), 0..40)), 0..6), prop::bool::ANY).prop_map(|(parts, case)| {
        let mut out = Vec::new();
        for (f, flips) in parts {
            let f = if case { randomize_case(&f, &flips) } else { f };
            out.extend_from_slice(f.as_bytes());
            out.push(b' ');
        }
        out
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn classification_matches_password_pattern(bytes in arb_stderr()) {
        let got = classify_stderr(&bytes);
        let want = if expects_encrypted(&bytes) { EngineError::Encrypted } else { EngineError::Corrupt };
        prop_assert_eq!(got, want);
    }

    #[test]
    fn arbitrary_bytes_never_panic_and_follow_rule(bytes in vec(any::<u8>(), 0..512)) {
        let got = classify_stderr(&bytes);
        let want = if expects_encrypted(&bytes) { EngineError::Encrypted } else { EngineError::Corrupt };
        prop_assert_eq!(got, want);
    }

    #[test]
    fn password_without_encrypted_is_corrupt(
        pre in "[a-z ]{0,10}", post in "[a-z ]{0,10}", flips in vec(any::<bool>(), 0..10)
    ) {
        prop_assume!(!post.contains("encrypted") && !pre.contains("encrypted"));
        let s = format!("{pre}{}{post}", randomize_case("password required", &flips));
        prop_assume!(!s.to_lowercase().contains("incorrect password"));
        prop_assert_eq!(classify_stderr(s.as_bytes()), EngineError::Corrupt);
    }

    #[test]
    fn incorrect_password_any_case_is_encrypted(
        pre in "[ -~]{0,20}", post in "[ -~]{0,20}", flips in vec(any::<bool>(), 0..20)
    ) {
        let s = format!("{pre}{}{post}", randomize_case("incorrect password", &flips));
        prop_assert_eq!(classify_stderr(s.as_bytes()), EngineError::Encrypted);
    }
}
