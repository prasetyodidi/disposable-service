// Feature: privacy-micro-apps-and-burn-workspace, Property 20: Token cacat ditolak dengan menyebut segmen
//! **Validates: Requirements 6.5, 6.6**

use proptest::prelude::*;

use super::gen_jwt::{corrupted_token, malformed_structure_token};
use crate::privacy::jwt_inspector::{inspect, InspectOptions, JwtError, MAX_TOKEN_CHARS};

fn opts() -> InspectOptions<'static> {
    InspectOptions { hmac_secret: None, redact: false }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// One corrupted segment => error names exactly that segment, no partial result.
    #[test]
    fn corrupted_segment_is_named(c in corrupted_token()) {
        let err = inspect(&c.token, &opts(), 0).unwrap_err();
        prop_assert_eq!(&err, &JwtError::Segment(c.segment));
        prop_assert!(err.to_string().contains(c.segment.name()));
    }

    /// Segment count != 3 => structure error.
    #[test]
    fn wrong_segment_count_rejected(t in malformed_structure_token()) {
        prop_assume!(!t.is_empty());
        let err = inspect(&t, &opts(), 0).unwrap_err();
        prop_assert_eq!(&err, &JwtError::Structure);
        prop_assert!(err.to_string().contains("tiga segmen"));
    }

    /// Tokens longer than the limit => length error mentioning the limit.
    #[test]
    fn overlong_token_rejected(extra in 1usize..2000, ch in prop::sample::select(vec!['a', 'Z', '.', '-', 'é'])) {
        let t: String = std::iter::repeat(ch).take(MAX_TOKEN_CHARS + extra).collect();
        let err = inspect(&t, &opts(), 0).unwrap_err();
        prop_assert_eq!(&err, &JwtError::TokenLength);
        prop_assert!(err.to_string().contains(&MAX_TOKEN_CHARS.to_string()));
    }
}

#[test]
fn empty_token_rejected_with_length_limit() {
    let err = inspect("", &opts(), 0).unwrap_err();
    assert_eq!(err, JwtError::TokenLength);
    assert!(err.to_string().contains("8192"));
}
