// Feature: privacy-micro-apps-and-burn-workspace, Property 27: Penimpaan buffer dengan nol
//
// **Validates: Requirements 10.2**
use proptest::prelude::*;

use crate::privacy::burn::wipe_buffer;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn wipe_buffer_zeroes_every_byte_and_keeps_length(
        mut buf in proptest::collection::vec(any::<u8>(), 0..4096)
    ) {
        let original_len = buf.len();
        wipe_buffer(&mut buf);
        prop_assert_eq!(buf.len(), original_len);
        prop_assert!(buf.iter().all(|&b| b == 0), "non-zero byte remained after wipe");
    }
}
