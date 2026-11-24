//! Property 15: Idempotensi redaksi.
//! **Validates: Requirements 5.9**
use proptest::prelude::*;

use super::gen_redact::{arbitrary_text, env_file, redact_options};
use crate::privacy::redactor::redact;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: privacy-micro-apps-and-burn-workspace, Property 15: Idempotensi redaksi
    #[test]
    fn redact_is_idempotent_on_text(
        text in arbitrary_text(),
        opts in redact_options(),
    ) {
        let once = redact(&text, &opts);
        let twice = redact(&once.text, &opts);
        prop_assert_eq!(twice.text, once.text);
    }

    // Feature: privacy-micro-apps-and-burn-workspace, Property 15: Idempotensi redaksi
    #[test]
    fn redact_is_idempotent_on_env_files(
        env in env_file(),
        opts in redact_options(),
    ) {
        let once = redact(&env.text, &opts);
        let twice = redact(&once.text, &opts);
        prop_assert_eq!(twice.text, once.text);
    }
}
