// Feature: privacy-micro-apps-and-burn-workspace, Property 33: Peringatan restart policy
//
// **Validates: Requirements 12.3**
use super::gen_burn::*;
use crate::privacy::burn::{restart_warning, BurnCoordinator};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

fn policy_strategy() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some("always".to_string())),
        Just(Some("unless-stopped".to_string())),
        Just(Some("no".to_string())),
        Just(Some("on-failure".to_string())),
        "[a-zA-Z0-9:_-]{0,12}".prop_map(Some),
    ]
}

fn expects_warning(p: &Option<String>) -> bool {
    !matches!(
        p.as_deref().map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("no") | Some("on-failure")
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn restart_warning_matches_policy(policy in policy_strategy()) {
        let w = restart_warning(policy.as_deref());
        prop_assert_eq!(w.is_some(), expects_warning(&policy), "policy {:?}", policy);
    }
}

#[test]
fn report_warnings_follow_restart_policy() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut runner = TestRunner::new(Config { cases: 100, ..Config::default() });
    let strat = (burn_spec(), policy_strategy());
    runner
        .run(&strat, |(spec, policy)| {
            rt.block_on(async {
                let (state, dir, _keep) = build_world(&spec).await;
                let backend = FakeBackend::new(&state, dir.path());
                let report = BurnCoordinator::new().run_with(&backend, policy.as_deref(), None).await;
                prop_assert_eq!(
                    !report.warnings.is_empty(),
                    expects_warning(&policy),
                    "policy {:?}",
                    policy
                );
                Ok(())
            })
        })
        .unwrap();
}
