// Feature: privacy-micro-apps-and-burn-workspace, Property 21: Verifikasi tanda tangan HMAC
//
// **Validates: Requirements 6.7, 6.8**
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use disposable_common::SignatureCheck;
use proptest::prelude::*;
use serde_json::Value;

use super::gen_jwt::{b64_json, build_signed_token, header_for, hmac_alg, json_object, secret_bytes, HmacAlg};
use crate::privacy::jwt_inspector::{inspect, InspectOptions};

/// HMAC zero-pads short keys, so secrets differing only by trailing zero
/// bytes are cryptographically equivalent; compare without them.
fn effective(s: &[u8]) -> &[u8] {
    let end = s.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
    &s[..end]
}

fn unsupported_alg() -> impl Strategy<Value = String> {
    prop_oneof![
        prop::sample::select(vec![
            "RS256", "RS512", "ES256", "ES384", "PS256", "EdDSA", "none", "None", "hs256", "HS000",
            "",
        ])
        .prop_map(String::from),
        "[A-Za-z0-9]{1,8}".prop_filter("not an HMAC alg", |a| {
            !matches!(a.as_str(), "HS256" | "HS384" | "HS512")
        }),
    ]
}

fn run(token: &str, secret: &[u8]) -> disposable_common::JwtInspectResult {
    let opts = InspectOptions { hmac_secret: Some(secret), redact: false };
    inspect(token, &opts, 0).expect("well-formed token")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn valid_with_same_secret_invalid_with_other_or_tampered(
        alg in hmac_alg(),
        secret in secret_bytes(),
        other in secret_bytes(),
        payload in json_object(),
        extra in "[a-z]{1,8}",
        header in hmac_alg().prop_flat_map(header_for),
    ) {
        // Header must carry the chosen alg.
        let mut header = header;
        header["alg"] = Value::String(alg.name().into());
        let payload = Value::Object(payload);
        let token = build_signed_token(&header, &payload, alg, &secret);

        // Same secret -> valid.
        prop_assert_eq!(run(&token, &secret).signature, SignatureCheck::Valid);

        // Different secret -> invalid.
        prop_assume!(effective(&secret) != effective(&other));
        prop_assert_eq!(run(&token, &other).signature, SignatureCheck::Invalid);

        // Altered payload (signature kept) -> invalid.
        let mut altered = payload.clone();
        altered.as_object_mut().unwrap().insert("k_tamper".into(), Value::String(extra));
        prop_assume!(altered != payload);
        let parts: Vec<&str> = token.split('.').collect();
        let tampered = format!("{}.{}.{}", parts[0], b64_json(&altered), parts[2]);
        prop_assert_eq!(run(&tampered, &secret).signature, SignatureCheck::Invalid);
    }

    #[test]
    fn unsupported_alg_reports_unsupported_with_decoded_result(
        alg in unsupported_alg(),
        secret in secret_bytes(),
        payload in json_object(),
        sig in prop::collection::vec(any::<u8>(), 0..64),
    ) {
        let header = serde_json::json!({ "alg": alg, "typ": "JWT" });
        let payload = Value::Object(payload);
        let token = format!(
            "{}.{}.{}",
            b64_json(&header),
            b64_json(&payload),
            URL_SAFE_NO_PAD.encode(sig)
        );
        let r = run(&token, &secret);
        prop_assert_eq!(r.signature, SignatureCheck::Unsupported);
        prop_assert_eq!(r.header, header);
        prop_assert_eq!(r.payload, payload);
    }
}

#[allow(dead_code)]
fn _uses(_: HmacAlg) {}
