use ed25519_dalek::Keypair;
use icn_types::identity::{CredentialProof, CredentialSubject, VerifiableCredential};
use rand::rngs::OsRng;
use std::collections::HashMap;
use base64::Engine;

#[test]
fn test_credential_canonical_serialization() {
    // Create a test credential
    let mut csprng = OsRng {};
    let keypair = Keypair::generate(&mut csprng);
    let verification_method = format!(
        "did:key:test#{}",
        base64::engine::general_purpose::STANDARD.encode(&keypair.public.to_bytes()[..8])
    );

    // Create a sample credential
    let mut claims = HashMap::new();
    claims.insert("name".to_string(), serde_json::json!("Test Subject"));
    claims.insert("role".to_string(), serde_json::json!("Tester"));

    let subject = CredentialSubject {
        id: "did:key:test".to_string(),
        claims,
    };

    let proof = CredentialProof {
        type_: "Ed25519Signature2020".to_string(),
        created: "2023-01-01T00:00:00Z".to_string(),
        verification_method: verification_method.clone(),
        proof_purpose: "assertionMethod".to_string(),
        jws: "".to_string(), // Empty signature for now
    };

    let credential = VerifiableCredential {
        context: vec!["https://www.w3.org/2018/credentials/v1".to_string()],
        id: "urn:uuid:test-credential".to_string(),
        type_: vec!["VerifiableCredential".to_string()],
        issuer: "did:key:issuer".to_string(),
        issuance_date: "2023-01-01T00:00:00Z".to_string(),
        expiration_date: Some("2024-01-01T00:00:00Z".to_string()),
        credential_subject: subject,
        proof,
    };

    // Test canonicalization
    let canonical_bytes = credential
        .canonical_bytes()
        .expect("Canonicalization failed");

    // The proof should be excluded from the canonical bytes
    let canonical_json: serde_json::Value =
        serde_json::from_slice(&canonical_bytes).expect("Failed to parse canonical JSON");

    assert!(
        canonical_json.get("proof").is_none(),
        "Proof should be excluded from canonical bytes"
    );

    // Check if the canonical bytes are deterministic
    let canonical_bytes2 = credential
        .canonical_bytes()
        .expect("Canonicalization failed");
    assert_eq!(
        canonical_bytes, canonical_bytes2,
        "Canonical bytes should be deterministic"
    );

    println!(
        "Canonical JSON: {}",
        String::from_utf8_lossy(&canonical_bytes)
    );
}

#[test]
fn test_credential_signing_and_verification() {
    // Create a test credential
    let mut csprng = OsRng {};
    let keypair = Keypair::generate(&mut csprng);
    let verification_method = format!(
        "did:key:test#{}",
        base64::engine::general_purpose::STANDARD.encode(&keypair.public.to_bytes()[..8])
    );

    // Create a sample credential
    let mut claims = HashMap::new();
    claims.insert("name".to_string(), serde_json::json!("Test Subject"));

    let subject = CredentialSubject {
        id: "did:key:test".to_string(),
        claims,
    };

    let proof = CredentialProof {
        type_: "Ed25519Signature2020".to_string(),
        created: "2023-01-01T00:00:00Z".to_string(),
        verification_method: verification_method.clone(),
        proof_purpose: "assertionMethod".to_string(),
        jws: "".to_string(), // Empty signature for now
    };

    let mut credential = VerifiableCredential {
        context: vec!["https://www.w3.org/2018/credentials/v1".to_string()],
        id: "urn:uuid:test-credential".to_string(),
        type_: vec!["VerifiableCredential".to_string()],
        issuer: "did:key:issuer".to_string(),
        issuance_date: "2023-01-01T00:00:00Z".to_string(),
        expiration_date: None,
        credential_subject: subject,
        proof,
    };

    // Sign the credential
    let jws = credential.sign(&keypair).expect("Signing failed");
    println!("Generated JWS: {}", jws);

    // Update the credential with the signature
    credential.proof.jws = jws;

    // Verify the credential
    let result = credential.verify(&keypair.public);
    assert!(result.is_ok(), "Verification failed: {:?}", result);

    // Test with full signature workflow
    let unsigned_credential = VerifiableCredential {
        context: vec!["https://www.w3.org/2018/credentials/v1".to_string()],
        id: "urn:uuid:test-credential-2".to_string(),
        type_: vec!["VerifiableCredential".to_string()],
        issuer: "did:key:issuer".to_string(),
        issuance_date: "2023-01-01T00:00:00Z".to_string(),
        expiration_date: None,
        credential_subject: CredentialSubject {
            id: "did:key:subject".to_string(),
            claims: HashMap::new(),
        },
        proof: CredentialProof {
            type_: "".to_string(),
            created: "".to_string(),
            verification_method: "".to_string(),
            proof_purpose: "".to_string(),
            jws: "".to_string(),
        },
    };

    let signed_credential = unsigned_credential
        .with_signature(&keypair, &verification_method)
        .expect("Signing with proof failed");

    // Verify the signed credential
    let result = signed_credential.verify(&keypair.public);
    assert!(
        result.is_ok(),
        "Verification of signed credential failed: {:?}",
        result
    );

    println!("✓ Credential successfully signed and verified");
    println!("Proof: {:?}", signed_credential.proof);
}
