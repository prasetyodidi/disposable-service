// Feature: pdf-to-markdown, Property 9: Dekode entitas tepat satu kali
// **Validates: Requirements 6.4, 6.5**
use proptest::prelude::*;

use super::gen_xml::escape_xml;
use crate::pdf2md::xml::decode_entities;

fn tricky_text() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<String>(),
        proptest::collection::vec(
            prop_oneof![
                Just("&lt;".to_string()),
                Just("&amp;lt;".to_string()),
                Just("&#38;".to_string()),
                Just("&#x26;".to_string()),
                Just("&".to_string()),
                Just("<>\"'".to_string()),
                "[a-zA-Zéж漢 ;#x0-9]{0,6}",
            ],
            0..8
        )
        .prop_map(|v| v.concat()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn decode_inverts_escape(s in tricky_text()) {
        prop_assert_eq!(decode_entities(&escape_xml(&s)), s);
    }

    #[test]
    fn out_of_range_numeric_refs_become_replacement_char(
        cp in prop_oneof![0x110000u32..=u32::MAX, 0xD800u32..=0xDFFF],
        hex in any::<bool>(),
        upper_x in any::<bool>(),
    ) {
        let r = if hex {
            format!("&#{}{:x};", if upper_x { 'X' } else { 'x' }, cp)
        } else {
            format!("&#{};", cp)
        };
        prop_assert_eq!(decode_entities(&r), "\u{FFFD}");
    }

    #[test]
    fn arbitrary_input_never_panics(s in "[&#xX;0-9a-fA-Fa-z]{0,40}") {
        let _ = decode_entities(&s);
    }
}
