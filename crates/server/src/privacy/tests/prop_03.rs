// Feature: privacy-micro-apps-and-burn-workspace, Property 3: Idempotensi scrub
//
// **Validates: Requirements 1.8, 2.4**
use proptest::prelude::*;

use super::gen_scrub::{arb_jpeg_spec, arb_pdf_spec, arb_png_spec};
use crate::privacy::scrubber::scrub;

fn check_idempotent(input: &[u8]) -> Result<(), TestCaseError> {
    let first = scrub(input).expect("first scrub of a valid file");
    let second = scrub(&first.bytes).expect("second scrub of a scrubbed file");
    prop_assert_eq!(&second.bytes, &first.bytes, "scrub(scrub(x)) must be byte identical");
    prop_assert!(
        second.report.removed.is_empty(),
        "second report must be empty, got {:?}",
        second.report.removed
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn scrub_jpeg_is_idempotent(spec in arb_jpeg_spec()) {
        check_idempotent(&spec.build())?;
    }

    #[test]
    fn scrub_png_is_idempotent(spec in arb_png_spec()) {
        check_idempotent(&spec.build())?;
    }

    #[test]
    fn scrub_pdf_is_idempotent(spec in arb_pdf_spec()) {
        check_idempotent(&spec.build())?;
    }
}
