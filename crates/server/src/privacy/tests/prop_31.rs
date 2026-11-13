// Feature: privacy-micro-apps-and-burn-workspace, Property 31: Invarian dan privasi AuditReport
//
// **Validates: Requirements 11.3, 11.4**
use super::gen_burn::*;
use crate::privacy::burn::{BurnCoordinator, CATEGORIES};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

#[test]
fn audit_report_invariants_and_privacy() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut runner = TestRunner::new(Config { cases: 100, ..Config::default() });
    let strat = (burn_spec(), fault_plan(), residue());
    runner
        .run(&strat, |(spec, faults, residue)| {
            rt.block_on(async {
                let (state, dir, _keep) = build_world(&spec).await;
                let backend = FakeBackend::new(&state, dir.path())
                    .with_faults(faults)
                    .with_residue(residue);
                let report = BurnCoordinator::new().run_with(&backend, Some("always"), None).await;

                // before/after: exactly the fixed categories, in order.
                for list in [&report.before, &report.after] {
                    let names: Vec<&str> = list.iter().map(|c| c.category.as_str()).collect();
                    prop_assert_eq!(names, CATEGORIES.to_vec());
                }
                for c in report.remaining_categories.iter() {
                    prop_assert!(CATEGORIES.contains(&c.category.as_str()));
                }
                for f in report.failed_items.iter() {
                    prop_assert!(CATEGORIES.contains(&f.category.as_str()));
                }

                // duration >= 0 (unsigned) and ISO 8601 UTC timestamp.
                let ts = &report.timestamp_utc;
                prop_assert!(ts.ends_with('Z'), "timestamp not UTC: {}", ts);
                let parsed = chrono::DateTime::parse_from_rfc3339(ts);
                prop_assert!(parsed.is_ok(), "timestamp not ISO 8601: {}", ts);
                prop_assert_eq!(parsed.unwrap().offset().local_minus_utc(), 0);

                // Privacy: no marker of file names, tokens, or contents.
                let json = serde_json::to_string(&*report).unwrap();
                prop_assert!(!json.contains(&spec.marker), "marker leaked in report");
                for m in spec.markers() {
                    prop_assert!(!json.contains(&m), "marker leaked in report");
                }
                Ok(())
            })
        })
        .unwrap();
}
