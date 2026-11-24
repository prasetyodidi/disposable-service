// Feature: pdf-to-markdown, Property 11: Determinisme dan format keluaran
// **Validates: Requirements 3.10, 6.6, 16.4**
use proptest::prelude::*;
use super::gen_doc::{arb_code_doc, arb_heading_doc, arb_intermediate_doc, arb_list_doc};
use crate::pdf2md::model::IntermediateDoc;
use crate::pdf2md::structure::{build, BuildOptions};

fn arb_any_doc() -> impl Strategy<Value = IntermediateDoc> {
    prop_oneof![arb_intermediate_doc(), arb_heading_doc(), arb_list_doc(), arb_code_doc()]
}

fn arb_build_options() -> impl Strategy<Value = BuildOptions> {
    (any::<bool>(), any::<bool>())
        .prop_map(|(page_breaks, strip_headers_footers)| BuildOptions { page_breaks, strip_headers_footers })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Two builds are byte-identical; output ends with exactly one `\n` and has no `\n\n\n`.
    #[test]
    fn build_is_deterministic_and_well_formed(doc in arb_any_doc(), opts in arb_build_options()) {
        let a = build(&doc, &opts);
        let b = build(&doc, &opts);
        prop_assert_eq!(a.markdown.as_bytes(), b.markdown.as_bytes());
        prop_assert_eq!(&a, &b);
        prop_assert!(a.markdown.ends_with('\n'));
        prop_assert!(!a.markdown.ends_with("\n\n"));
        prop_assert!(!a.markdown.contains("\n\n\n"));
    }
}
