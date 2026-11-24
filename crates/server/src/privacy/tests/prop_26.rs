// Feature: privacy-micro-apps-and-burn-workspace, Property 26: Burn mengosongkan seluruh Managed_Storage dan membuktikannya
//! **Validates: Requirements 10.1, 10.2, 10.3, 10.4, 10.5, 11.1, 11.2, 12.5**

use proptest::prelude::*;

use super::gen_burn::{any_marker_left, build_world, burn_spec, dir_stats, expected_before, FakeBackend};
use crate::privacy::burn::{BurnCoordinator, CATEGORIES};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn burn_empties_all_storage_and_proves_it(spec in burn_spec()) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let (state, dir, _keep) = build_world(&spec).await;
            let backend = FakeBackend::new(&state, dir.path());
            let expected = expected_before(&spec);
            let coordinator = BurnCoordinator::new();

            // "no" restart policy: no warning, and nothing real is shut down.
            let report = coordinator.run_with(&backend, Some("no"), None).await;

            // `before` equals the independent computation.
            prop_assert_eq!(&report.before, &expected);
            prop_assert_eq!(
                report.total_bytes_before,
                expected.iter().map(|c| c.bytes).sum::<u64>()
            );

            // Verified and fully empty.
            prop_assert!(report.verified);
            prop_assert!(!report.incomplete);
            prop_assert_eq!(report.remaining_bytes, 0);
            prop_assert!(report.remaining_categories.is_empty());
            prop_assert!(report.failed_items.is_empty());
            let names: Vec<_> = report.after.iter().map(|c| c.category.as_str()).collect();
            prop_assert_eq!(names, CATEGORIES.to_vec());
            prop_assert!(report.after.iter().all(|c| c.entries == 0 && c.bytes == 0));

            // Independent check of actual storage.
            prop_assert!(state.sessions.read().await.is_empty());
            prop_assert!(state.downloads.read().await.is_empty());
            prop_assert!(state.mcp_sessions.read().await.is_empty());
            prop_assert_eq!(dir_stats(dir.path()), (0, 0));
            prop_assert!(!any_marker_left(&state, dir.path(), &spec).await);

            // Removed items cover sessions, downloads, mcp sessions and files.
            let min_removed = (spec.sessions.len() + spec.downloads.len()
                + spec.mcp_ids.len()) as u64 + spec.file_count();
            prop_assert!(report.removed_items >= min_removed);
            Ok(())
        })?;
    }
}
