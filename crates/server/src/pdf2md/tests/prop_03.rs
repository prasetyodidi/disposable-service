// Feature: pdf-to-markdown, Property 3: Resolusi rentang halaman
// **Validates: Requirements 1.5, 1.6, 2.7, 5.7**
use proptest::prelude::*;

use crate::pdf2md::options::{resolve_range, RangeError};

const MAX_PAGES: u32 = 300;

/// (from, to, total) with values clustered around the interesting boundaries.
fn arb_range_input() -> impl Strategy<Value = (u32, Option<u32>, u32)> {
    let total = prop_oneof![1..=20u32, 295..=310u32, 1..=1000u32];
    (total, 1..=1200u32, prop::option::of(0..=1300u32), 0..=4u8).prop_map(|(total, from_raw, to_raw, mode)| {
        // Bias `from` toward total boundaries in some cases.
        let from = match mode {
            0 => total,
            1 => total + 1,
            2 => 1,
            _ => from_raw,
        };
        let to = to_raw.map(|t| from.max(1).saturating_add(t));
        (from, to, total)
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn range_resolution_matches_spec((from, to, total) in arb_range_input()) {
        let r = resolve_range(from, to, total, MAX_PAGES);

        if from > total {
            prop_assert_eq!(r, Err(RangeError::FromBeyondTotal(total)));
            return Ok(());
        }

        let last = to.unwrap_or(total).min(total);
        let count = (last - from) as u64 + 1;

        if count > MAX_PAGES as u64 {
            prop_assert_eq!(r, Err(RangeError::TooManyPages));
            return Ok(());
        }

        let (f, l, warn) = r.expect("range should be accepted");
        prop_assert_eq!(f, from);
        prop_assert_eq!(l, last);
        prop_assert_eq!((l - f) as u64 + 1, count);
        prop_assert!(count <= MAX_PAGES as u64);
        let clamped = matches!(to, Some(t) if t > total);
        prop_assert_eq!(warn.is_some(), clamped);
    }

    /// Exactly 300 pages is accepted; 301 is rejected.
    #[test]
    fn max_pages_boundary(from in 1..=500u32, extra in 0..=50u32) {
        let total = from + 300 + extra;
        let ok = resolve_range(from, Some(from + 299), total, MAX_PAGES);
        prop_assert_eq!(ok.map(|(f, l, _)| (f, l)), Ok((from, from + 299)));
        let bad = resolve_range(from, Some(from + 300), total, MAX_PAGES);
        prop_assert_eq!(bad, Err(RangeError::TooManyPages));
    }
}
