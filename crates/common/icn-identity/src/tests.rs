use crate::{Did, KeyPair, VerifiableCredential};
use crate::{FederationMetadata, TrustBundle};
use crate::{QuorumError, QuorumProof, QuorumType};
use std::collections::HashMap;

#[test]
fn did_round_trip_ed25519() {
    let kp = KeyPair::generate();
    let did_str = kp.did.as_str().to_owned();

    // Re-parse DID → public key
    let pk = kp.did.to_ed25519().unwrap();
    assert_eq!(pk.to_bytes(), kp.pk.to_bytes());

    // Manually decode the multibase bytes
    let did2 = Did::new_ed25519(&pk);
    assert_eq!(did2.as_str(), did_str);
}

#[test]
fn sign_and_verify() {
    let kp = KeyPair::generate();
    let msg = b"ICN rocks";
    let sig = kp.sign(msg);
    assert!(kp.verify(msg, &sig));

    // Tamper
    let mut bad = sig.to_bytes();
    bad[0] ^= 0xFF;

    // In ed25519-dalek v2, from_bytes returns a Signature directly, not a Result
    let bad_sig = ed25519_dalek::Signature::from_bytes(&bad);
    assert!(!kp.verify(msg, &bad_sig));
}

#[test]
fn malformed_did_rejected() {
    // Random base58 string, wrong prefix.
    let bad = "did:key:zQ3shBAdummy";
    assert!(bad.parse::<Did>().is_err_or_none());
}

trait ErrOrNone<T, E> {
    fn is_err_or_none(&self) -> bool;
}
impl<T, E> ErrOrNone<T, E> for Result<T, E> {
    fn is_err_or_none(&self) -> bool {
        self.is_err()
    }
}

// VC Tests
#[test]
fn vc_sign_and_verify() {
    let kp = KeyPair::generate();
    let vc = VerifiableCredential {
        context: vec!["https://www.w3.org/2018/credentials/v1".into()],
        types: vec!["VerifiableCredential".into()],
        issuer: kp.did.clone(),
        issuance_date: chrono::Utc::now(),
        credential_subject: serde_json::json!({"hello": "world"}),
        proof: None,
    };

    let signed = vc.sign(&kp).unwrap();
    assert!(signed.verify(&kp.pk).is_ok());

    // Tamper
    let mut tampered = signed.clone();
    tampered.vc.credential_subject = serde_json::json!({"hello": "evil"});
    assert!(tampered.verify(&kp.pk).is_err());
}

#[test]
fn canonical_bytes_stable() {
    let kp = KeyPair::generate();
    let vc1 = VerifiableCredential {
        context: vec!["https://www.w3.org/2018/credentials/v1".into()],
        types: vec!["VC".into()],
        issuer: kp.did.clone(),
        issuance_date: chrono::Utc::now(),
        credential_subject: serde_json::json!({"x": 1, "y": 2}),
        proof: None,
    };
    let vc2 = vc1.clone();

    let b1 = vc1.canonical_bytes().unwrap();
    let b2 = vc2.canonical_bytes().unwrap();
    assert_eq!(b1, b2, "deterministic serialization failed");
}

// QuorumProof Tests
#[test]
fn quorum_proof_majority() {
    // Create 5 keypairs as potential signers
    let keypairs: Vec<KeyPair> = (0..5).map(|_| KeyPair::generate()).collect();

    // Create a message to sign
    let message = b"Federation test message";

    // Create a map of allowed signers
    let mut allowed_signers = HashMap::new();
    for kp in &keypairs {
        allowed_signers.insert(kp.did.clone(), kp.pk);
    }

    // Create signatures from 3 signers (majority of 5)
    let signatures = vec![
        (keypairs[0].did.clone(), keypairs[0].sign(message)),
        (keypairs[1].did.clone(), keypairs[1].sign(message)),
        (keypairs[2].did.clone(), keypairs[2].sign(message)),
    ];

    // Create a majority quorum proof
    let proof = QuorumProof::new(QuorumType::Majority, signatures);

    // Verify should succeed with 3/5 signatures
    assert!(proof.verify(message, &allowed_signers).is_ok());

    // Create a proof with only 2 signatures (not a majority)
    let insufficient_signatures = vec![
        (keypairs[0].did.clone(), keypairs[0].sign(message)),
        (keypairs[1].did.clone(), keypairs[1].sign(message)),
    ];

    let insufficient_proof = QuorumProof::new(QuorumType::Majority, insufficient_signatures);

    // Verify should fail with 2/5 signatures
    assert!(matches!(
        insufficient_proof.verify(message, &allowed_signers),
        Err(QuorumError::InsufficientSigners)
    ));
}

#[test]
fn quorum_proof_threshold() {
    // Create 5 keypairs as potential signers
    let keypairs: Vec<KeyPair> = (0..5).map(|_| KeyPair::generate()).collect();

    // Create a message to sign
    let message = b"Federation threshold test";

    // Create a map of allowed signers
    let mut allowed_signers = HashMap::new();
    for kp in &keypairs {
        allowed_signers.insert(kp.did.clone(), kp.pk);
    }

    // Create signatures from 2 signers
    let signatures = vec![
        (keypairs[0].did.clone(), keypairs[0].sign(message)),
        (keypairs[1].did.clone(), keypairs[1].sign(message)),
    ];

    // Create a threshold quorum proof requiring 2 signers
    let proof = QuorumProof::new(QuorumType::Threshold(2), signatures.clone());

    // Verify should succeed with 2 signatures meeting threshold
    assert!(proof.verify(message, &allowed_signers).is_ok());

    // Test with threshold too high
    let high_threshold_proof = QuorumProof::new(QuorumType::Threshold(6), signatures);

    // Verify should fail with threshold > number of allowed signers
    assert!(matches!(
        high_threshold_proof.verify(message, &allowed_signers),
        Err(QuorumError::ThresholdTooHigh)
    ));
}

#[test]
fn quorum_proof_weighted() {
    // Create 3 keypairs as potential signers
    let keypairs: Vec<KeyPair> = (0..3).map(|_| KeyPair::generate()).collect();

    // Create a message to sign
    let message = b"Federation weighted test";

    // Create a map of allowed signers
    let mut allowed_signers = HashMap::new();
    for kp in &keypairs {
        allowed_signers.insert(kp.did.clone(), kp.pk);
    }

    // Create weight map: kp0 gets 3 votes, kp1 gets 2 votes, kp2 gets 1 vote
    let mut weights = HashMap::new();
    weights.insert(keypairs[0].did.clone(), 3);
    weights.insert(keypairs[1].did.clone(), 2);
    weights.insert(keypairs[2].did.clone(), 1);

    // Case 1: Only kp0 signs (3/6 votes, not enough)
    let signatures1 = vec![(keypairs[0].did.clone(), keypairs[0].sign(message))];

    let proof1 = QuorumProof::new(QuorumType::Weighted(weights.clone()), signatures1);

    // Should fail - need more than 3/6 votes
    assert!(matches!(
        proof1.verify(message, &allowed_signers),
        Err(QuorumError::InsufficientSigners)
    ));

    // Case 2: kp0 and kp1 sign (5/6 votes, sufficient)
    let signatures2 = vec![
        (keypairs[0].did.clone(), keypairs[0].sign(message)),
        (keypairs[1].did.clone(), keypairs[1].sign(message)),
    ];

    let proof2 = QuorumProof::new(QuorumType::Weighted(weights), signatures2);

    // Should succeed with 5/6 votes
    assert!(proof2.verify(message, &allowed_signers).is_ok());
}

#[test]
fn quorum_proof_duplicate_signer() {
    // Create keypair
    let kp = KeyPair::generate();

    // Create a message to sign
    let message = b"Federation duplicate test";

    // Create a map of allowed signers
    let mut allowed_signers = HashMap::new();
    allowed_signers.insert(kp.did.clone(), kp.pk);

    // Try to add same signer twice
    let signatures = vec![
        (kp.did.clone(), kp.sign(message)),
        (kp.did.clone(), kp.sign(message)), // Duplicate signer
    ];

    let proof = QuorumProof::new(QuorumType::Majority, signatures);

    // Verify should fail due to duplicate signer
    assert!(matches!(
        proof.verify(message, &allowed_signers),
        Err(QuorumError::DuplicateSigner)
    ));
}

// TrustBundle Tests
#[test]
fn trust_bundle_verify() {
    // Create 5 keypairs as trusted signers
    let keypairs: Vec<KeyPair> = (0..5).map(|_| KeyPair::generate()).collect();

    // Create federation metadata
    let metadata = FederationMetadata {
        name: "Test Federation".to_string(),
        description: Some("A test federation for unit tests".to_string()),
        version: "1.0".to_string(),
        additional: HashMap::new(),
    };

    // Create a trust bundle
    let mut bundle = TrustBundle::new(
        "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(),
        metadata,
    );

    // Calculate the hash for signing
    let bundle_hash = bundle.calculate_hash().unwrap();

    // Create signatures from 3 signers
    let signatures = vec![
        (keypairs[0].did.clone(), keypairs[0].sign(&bundle_hash)),
        (keypairs[1].did.clone(), keypairs[1].sign(&bundle_hash)),
        (keypairs[2].did.clone(), keypairs[2].sign(&bundle_hash)),
    ];

    // Create a quorum proof
    let proof = QuorumProof::new(QuorumType::Majority, signatures);

    // Add the proof to the bundle
    bundle.add_quorum_proof(proof);

    // Create a map of trusted signer verifying keys
    let mut signer_keys = HashMap::new();
    for kp in &keypairs {
        signer_keys.insert(kp.did.clone(), kp.pk);
    }

    // Verify the trust bundle
    assert!(bundle.verify(&signer_keys).is_ok());

    // Test with a tampered bundle
    let mut tampered_bundle = bundle.clone();
    tampered_bundle.root_dag_cid =
        "bafybeiczsscdsbs7ffqz55asqdf3smv6klcw3gofszvwlyarci47bgf354".to_string();

    // Verification should fail for the tampered bundle
    assert!(tampered_bundle.verify(&signer_keys).is_err());
}
