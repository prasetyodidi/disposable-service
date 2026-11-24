//! Property 25: Parsing konfigurasi `PDF_MD_MAX_BYTES`.
//! **Validates: Requirements 9.1, 9.2, 9.3, 9.4**
use crate::pdf2md::config::PdfConfig;
use proptest::prelude::*;

const CAP: u64 = 26_214_400;
const LAMBDA: u64 = 5_242_880;

fn run(v: Option<&str>, lambda: bool) -> Result<PdfConfig, crate::pdf2md::config::ConfigError> {
    let v = v.map(str::to_string);
    PdfConfig::from_env_with(&move |k| match k {
        "PDF_MD_MAX_BYTES" => v.clone(),
        "AWS_LAMBDA_FUNCTION_NAME" if lambda => Some("fn".to_string()),
        _ => None,
    })
}

/// Reference model of the acceptance rule.
fn expected(v: &str) -> Option<u64> {
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    v.parse::<u64>().ok().filter(|n| *n >= 1)
}

fn arb_value() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<u64>().prop_map(|n| n.to_string()),
        (1u64..=CAP * 2).prop_map(|n| n.to_string()),
        Just(CAP.to_string()),
        Just((CAP + 1).to_string()),
        Just("0".to_string()),
        Just("00".to_string()),
        Just("18446744073709551616".to_string()),
        "[0-9]{20,30}",
        "[+-]?[0-9]{1,8}",
        "[0-9]{1,6}\\.[0-9]{1,3}",
        "[0-9]{1,8}[a-zA-Z]{1,3}",
        "[ \t]{0,2}[0-9]{1,8}[ \t]{0,2}",
        "\\PC{0,12}",
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: pdf-to-markdown, Property 25: Parsing konfigurasi PDF_MD_MAX_BYTES
    #[test]
    fn max_bytes_parsing(v in arb_value(), lambda in any::<bool>()) {
        let res = run(Some(&v), lambda);
        if v.is_empty() {
            let c = res.unwrap();
            prop_assert_eq!(c.max_bytes, if lambda { LAMBDA } else { CAP });
            prop_assert!(c.warnings.is_empty());
        } else {
            match expected(&v) {
                Some(n) => {
                    let c = res.unwrap();
                    prop_assert_eq!(c.max_bytes, n.min(CAP));
                    if n > CAP {
                        prop_assert_eq!(c.warnings.len(), 1);
                        prop_assert!(c.warnings[0].contains("PDF_MD_MAX_BYTES"));
                    } else {
                        prop_assert!(c.warnings.is_empty());
                    }
                }
                None => {
                    let e = res.unwrap_err();
                    prop_assert!(e.to_string().contains("PDF_MD_MAX_BYTES"));
                }
            }
        }
    }

    #[test]
    fn unset_uses_default(lambda in any::<bool>()) {
        let c = run(None, lambda).unwrap();
        prop_assert_eq!(c.max_bytes, if lambda { LAMBDA } else { CAP });
        prop_assert!(c.warnings.is_empty());
    }
}
