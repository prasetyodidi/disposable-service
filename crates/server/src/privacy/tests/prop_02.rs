// Feature: privacy-micro-apps-and-burn-workspace, Property 2: Scrub_Report akurat
//! **Validates: Requirements 1.3, 2.1**
//!
//! For any valid JPEG/PNG/PDF with known inserted metadata categories, the
//! `ScrubReport` lists exactly those categories (count > 0) with the same counts,
//! contains no metadata values, and is empty when nothing was inserted.

use super::gen_scrub::*;
use crate::privacy::scrubber::scrub;
use disposable_common::ScrubReport;
use proptest::prelude::*;

fn check(out: &ScrubReport, expected: &ScrubReport, has_metadata: bool) -> Result<(), TestCaseError> {
    prop_assert_eq!(out, expected);
    prop_assert!(out.removed.iter().all(|c| c.count > 0));
    prop_assert_eq!(out.removed.is_empty(), !has_metadata);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn scrub_report_is_accurate_jpeg(spec in arb_jpeg_spec()) {
        let out = scrub(&spec.build()).unwrap();
        check(&out.report, &spec.expected_report(), spec.has_metadata())?;
    }

    #[test]
    fn scrub_report_is_accurate_png(spec in arb_png_spec()) {
        let out = scrub(&spec.build()).unwrap();
        check(&out.report, &spec.expected_report(), spec.has_metadata())?;
    }

    #[test]
    fn scrub_report_is_accurate_pdf_and_has_no_values(spec in arb_pdf_spec()) {
        let out = scrub(&spec.build()).unwrap();
        check(&out.report, &spec.expected_report(), spec.has_metadata())?;
        // No metadata value (all start with `SENS`) may appear in the serialized report.
        let json = serde_json::to_string(&out.report).unwrap();
        for (_, v) in &spec.info {
            prop_assert!(!json.contains(v.as_str()));
        }
        prop_assert!(!json.contains("SENS"));
    }
}
