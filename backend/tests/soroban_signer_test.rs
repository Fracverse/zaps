use base64::{engine::general_purpose, Engine as _};
use serde_json::Value;

use blinks_backend::service::soroban_service::CustodialSigner;

#[tokio::test]
async fn test_custodial_signer_ed25519_hex_seed() {
    // 32-byte hex seed (0x01 repeated)
    let seed = "01".repeat(32);
    let signer = CustodialSigner::new(seed.clone());

    let payload = "testpayload";
    let payload_b64 = general_purpose::STANDARD.encode(payload.as_bytes());

    let res = signer.sign_transaction(&payload_b64).await;
    assert!(res.is_ok(), "signing should succeed");

    let out_b64 = res.unwrap();
    let out_raw = general_purpose::STANDARD
        .decode(&out_b64)
        .expect("output should be base64");

    let v: Value = serde_json::from_slice(&out_raw).expect("valid json");
    assert_eq!(v.get("signature_type").and_then(|s| s.as_str()), Some("ed25519"));
    assert!(v.get("signature").is_some());
    assert!(v.get("pubkey").is_some());
}

#[tokio::test]
async fn test_custodial_signer_hmac_fallback() {
    // Non-hex secret should use HMAC fallback
    let secret = "not_hex_secret".to_string();
    let signer = CustodialSigner::new(secret);

    let payload = "anotherpayload";
    let payload_b64 = general_purpose::STANDARD.encode(payload.as_bytes());

    let res = signer.sign_transaction(&payload_b64).await;
    assert!(res.is_ok(), "signing should succeed");

    let out_b64 = res.unwrap();
    let out_raw = general_purpose::STANDARD
        .decode(&out_b64)
        .expect("output should be base64");

    let v: Value = serde_json::from_slice(&out_raw).expect("valid json");
    assert_eq!(v.get("signature_type").and_then(|s| s.as_str()), Some("hmac-sha256"));
    assert!(v.get("signature").is_some());
}
