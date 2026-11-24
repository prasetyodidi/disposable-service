// Feature: privacy-micro-apps-and-burn-workspace, Property 11: Redaksi `.env` mempertahankan struktur
//! **Validates: Requirements 5.2**

use proptest::prelude::*;

use super::gen_redact::{env_file, format_strategy};
use crate::privacy::redactor::{redact, Category, Format, RedactOptions};

const PH: &str = "[REDACTED:env_value]";

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn env_redaction_preserves_structure(
        env in env_file(),
        fmt in format_strategy().prop_filter("env-capable", |f| *f != Format::Text),
    ) {
        let opts = RedactOptions {
            categories: Category::ALL.iter().copied().collect(),
            custom_words: Vec::new(),
            format: fmt,
        };
        let out = redact(&env.text, &opts);

        let in_lines: Vec<&str> = env.text.split('\n').collect();
        let out_lines: Vec<&str> = out.text.split('\n').collect();
        prop_assert_eq!(in_lines.len(), out_lines.len());

        let mut names = Vec::new();
        for (i, o) in in_lines.iter().zip(out_lines.iter()) {
            if i.is_empty() || i.starts_with('#') {
                // Blank and comment lines are identical.
                prop_assert_eq!(i, o);
            } else {
                let (name, value) = i.split_once('=').expect("assignment");
                names.push(name.to_string());
                let expected = if value.is_empty() {
                    format!("{name}=")
                } else {
                    format!("{name}={PH}")
                };
                prop_assert_eq!(&expected, o);
            }
        }
        prop_assert_eq!(&names, &env.names);
        prop_assert_eq!(out.counts[&Category::EnvValue], env.non_empty_values);
    }
}
