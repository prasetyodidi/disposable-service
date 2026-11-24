// Feature: privacy-micro-apps-and-burn-workspace, Property 30: Burn tahan kegagalan sebagian dan tidak pernah memalsukan verifikasi
//! **Validates: Requirements 10.8, 11.5, 11.6**
use super::gen_burn::*;
use crate::privacy::burn::{BurnCoordinator, StorageBackend, CATEGORIES};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

#[test]
fn prop_30_partial_failure_never_fakes_verification() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut runner = TestRunner::new(Config { cases: 100, ..Config::default() });
    runner
        .run(&(burn_spec(), fault_plan(), residue()), |(spec, faults, residue)| {
            rt.block_on(async {
                let (state, dir, _keep) = build_world(&spec).await;
                let backend = FakeBackend::new(&state, dir.path())
                    .with_faults(faults)
                    .with_residue(residue.clone());
                let before = expected_before(&spec);
                let report = BurnCoordinator::new().run_with(&backend, None, None).await;

                prop_assert_eq!(&report.before, &before);

                // Expected "after": failed steps keep their data, healthy ones
                // are emptied; residue is added on top.
                let failed_cats = faults.failed_categories();
                let mut expected_after: Vec<(u64, u64)> = CATEGORIES
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        if failed_cats.contains(c) {
                            (before[i].entries, before[i].bytes)
                        } else {
                            (0, 0)
                        }
                    })
                    .collect();
                if let Some(r) = &residue {
                    expected_after[r.category].0 += r.entries;
                    expected_after[r.category].1 += r.bytes;
                }
                let actual_after: Vec<(u64, u64)> =
                    report.after.iter().map(|c| (c.entries, c.bytes)).collect();
                prop_assert_eq!(actual_after, expected_after.clone());

                // Other steps still ran: healthy categories hold no spec data.
                if !faults.fail_sessions {
                    prop_assert!(state.sessions.read().await.is_empty());
                }
                if !faults.fail_downloads {
                    prop_assert!(state.downloads.read().await.is_empty());
                }
                if !faults.fail_mcp {
                    prop_assert!(state.mcp_sessions.read().await.is_empty());
                }
                if !faults.fail_temp {
                    prop_assert_eq!(dir_stats(dir.path()), (0, 0));
                }

                // failed_items lists exactly the failed steps.
                let mut got: Vec<String> =
                    report.failed_items.iter().map(|f| f.category.clone()).collect();
                got.sort();
                let mut want: Vec<String> =
                    failed_cats.iter().map(|c| c.to_string()).collect();
                want.sort();
                prop_assert_eq!(got, want);

                // remaining_bytes equals the residual bytes.
                let remaining: u64 = expected_after.iter().map(|x| x.1).sum();
                prop_assert_eq!(report.remaining_bytes, remaining);

                let any_left = expected_after.iter().any(|x| x.0 > 0 || x.1 > 0);
                if faults.any() || any_left {
                    prop_assert!(report.incomplete);
                    prop_assert!(!report.verified);
                } else {
                    prop_assert!(report.verified);
                    prop_assert!(!report.incomplete);
                }
                // Backend measure is still consistent afterwards.
                prop_assert_eq!(backend.measure().await.len(), 4);
                Ok(())
            })
        })
        .unwrap();
}
