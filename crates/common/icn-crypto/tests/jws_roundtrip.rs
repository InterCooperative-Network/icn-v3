use assert_matches::assert_matches;
use ed25519_dalek::{SigningKey, VerifyingKey};
use icn_crypto::{sign_detached_jws, verify_detached_jws};
use rand::rngs::OsRng;

#[test]
fn test_jws_sign_verify_roundtrip() {
    let mut csprng = OsRng;
    let signing_key: SigningKey = SigningKey::generate(&mut csprng);
    let public_key: VerifyingKey = VerifyingKey::from(&signing_key);

    // Test payload
    let payload = b"test payload for JWS signing";

    // Sign the payload
    let detached_jws = sign_detached_jws(payload, &signing_key).expect("Failed to sign payload");

    // Verify the signature
    let result = verify_detached_jws(payload, &detached_jws, &public_key);
    assert_matches!(result, Ok(()));

    // Verify the format: header..signature
    let parts: Vec<&str> = detached_jws.split('.').collect();
    assert_eq!(parts.len(), 3);
    assert!(!parts[0].is_empty(), "Header part should not be empty");
    assert!(
        parts[1].is_empty(),
        "Middle part should be empty in detached JWS"
    );
    assert!(!parts[2].is_empty(), "Signature part should not be empty");

    // Tamper with the payload and verify it fails
    let tampered_payload = b"tampered payload";
    let tamper_result = verify_detached_jws(tampered_payload, &detached_jws, &public_key);
    assert!(
        tamper_result.is_err(),
        "Verification should fail with tampered payload"
    );

    // Tamper with the signature and verify it fails
    let tampered_jws = detached_jws.replace(detached_jws.chars().last().unwrap(), "X");
    let tamper_sig_result = verify_detached_jws(payload, &tampered_jws, &public_key);
    assert!(
        tamper_sig_result.is_err(),
        "Verification should fail with tampered signature"
    );
}
