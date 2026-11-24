// Feature: privacy-micro-apps-and-burn-workspace, Property 18: Klaim waktu dan status token
//! **Validates: Requirements 6.2, 6.3**

use chrono::DateTime;
use disposable_common::TokenStatus;
use proptest::prelude::*;

use super::gen_jwt::jwt_case;
use crate::privacy::jwt_inspector::{inspect, InspectOptions};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn time_claims_and_status(case in jwt_case()) {
        let opts = InspectOptions { hmac_secret: None, redact: false };
        let r = inspect(&case.token, &opts, case.now).unwrap();
        let tc = &case.payload.time;

        // One entry per present claim.
        prop_assert_eq!(r.times.len(), tc.present().len());

        for (name, val) in tc.present() {
            let ct = r.times.iter().find(|c| c.claim == name).expect("claim reported");
            match val.seconds() {
                Some(secs) => {
                    prop_assert!(ct.valid);
                    let iso = ct.iso.as_deref().expect("iso present");
                    prop_assert!(iso.ends_with('Z'));
                    let parsed = DateTime::parse_from_rfc3339(iso).unwrap();
                    prop_assert_eq!(parsed.timestamp(), secs);
                }
                None => {
                    prop_assert!(!ct.valid);
                    prop_assert!(ct.iso.is_none());
                    prop_assert_eq!(&ct.raw, &val.to_json());
                }
            }
        }

        // Status from valid exp/nbf only.
        let exp = tc.exp.as_ref().and_then(|v| v.seconds());
        let nbf = tc.nbf.as_ref().and_then(|v| v.seconds());
        let expected = if exp.is_some_and(|e| case.now >= e) {
            TokenStatus::Expired
        } else if nbf.is_some_and(|n| case.now < n) {
            TokenStatus::NotYetValid
        } else {
            TokenStatus::Active
        };
        prop_assert_eq!(r.status, expected);
    }
}
