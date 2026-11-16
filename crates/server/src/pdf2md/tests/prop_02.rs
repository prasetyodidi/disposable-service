// Feature: pdf-to-markdown, Property 2: Opsi tidak valid ditolak dengan menyebut field
// **Validates: Requirements 1.4**
use proptest::prelude::*;

use super::gen_options::arb_invalid_options;
use crate::pdf2md::options::parse_options;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn invalid_options_rejected_naming_field(i in arb_invalid_options()) {
        let e = parse_options(Some(&i.raw)).expect_err("invalid options must be rejected");
        prop_assert_eq!(e.field, i.field);
    }
}
