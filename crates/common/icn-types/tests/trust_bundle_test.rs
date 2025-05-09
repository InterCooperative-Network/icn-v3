use icn_types::{
    identity::{CredentialProof, CredentialSubject, TrustBundle, VerifiableCredential},
    trust::QuorumConfig,
};
use std::collections::HashMap;

#[test]
fn test_trust_bundle_verification() {
    // Create a list of authorized DIDs
    let authorized_dids = vec![
        "did:key:issuer1".to_string(),
        "did:key:issuer2".to_string(),
        "did:key:issuer3".to_string(),
        "did:key:issuer4".to_string(),
        "did:key:issuer5".to_string(),
    ];

    // Create sample credentials from different issuers
    let credentials = vec![
        create_test_credential("cred1", "did:key:issuer1", "did:key:subject"),
        create_test_credential("cred2", "did:key:issuer2", "did:key:subject"),
        create_test_credential("cred3", "did:key:issuer3", "did:key:subject"),
    ];

    // Create a trust bundle with the credentials
    let bundle = TrustBundle {
        id: "bundle1".to_string(),
        credentials,
        quorum_rule: r#"{"type":"Majority"}"#.to_string(),
        created: "2023-01-01T00:00:00Z".to_string(),
        expires: None,
    };

    // Create a majority quorum config
    let config = QuorumConfig::new_majority(authorized_dids.clone());

    // Verify the bundle
    let result = bundle.verify(&config);
    assert!(result.is_ok(), "Bundle verification failed: {:?}", result);
    assert!(result.unwrap(), "Quorum not satisfied");

    // Test with threshold quorum
    let threshold_config = QuorumConfig::new_threshold(authorized_dids.clone(), 50).unwrap();
    let result = bundle.verify(&threshold_config);
    assert!(
        result.is_ok(),
        "Bundle verification with threshold failed: {:?}",
        result
    );
    assert!(result.unwrap(), "Threshold quorum not satisfied");

    // Test with weighted quorum
    let mut weights = HashMap::new();
    weights.insert("did:key:issuer1".to_string(), 5);
    weights.insert("did:key:issuer2".to_string(), 3);
    weights.insert("did:key:issuer3".to_string(), 2);
    weights.insert("did:key:issuer4".to_string(), 1);
    weights.insert("did:key:issuer5".to_string(), 1);

    let weighted_config = QuorumConfig::new_weighted(weights, 8).unwrap();
    let result = bundle.verify(&weighted_config);
    assert!(
        result.is_ok(),
        "Bundle verification with weights failed: {:?}",
        result
    );
    assert!(result.unwrap(), "Weighted quorum not satisfied");

    // Test with unauthorized signer
    let unauthorized_bundle = TrustBundle {
        id: "bundle2".to_string(),
        credentials: vec![
            create_test_credential("cred4", "did:key:issuer1", "did:key:subject"),
            create_test_credential("cred5", "did:key:unauthorized", "did:key:subject"),
        ],
        quorum_rule: r#"{"type":"Majority"}"#.to_string(),
        created: "2023-01-01T00:00:00Z".to_string(),
        expires: None,
    };

    let result = unauthorized_bundle.verify(&config);
    assert!(result.is_err(), "Should fail with unauthorized signer");

    // Test with duplicate signers
    let duplicate_bundle = TrustBundle {
        id: "bundle3".to_string(),
        credentials: vec![
            create_test_credential("cred6", "did:key:issuer1", "did:key:subject"),
            create_test_credential("cred7", "did:key:issuer1", "did:key:subject"),
        ],
        quorum_rule: r#"{"type":"Majority"}"#.to_string(),
        created: "2023-01-01T00:00:00Z".to_string(),
        expires: None,
    };

    let result = duplicate_bundle.verify(&config);
    assert!(result.is_err(), "Should fail with duplicate signers");

    println!("All TrustBundle verification tests passed!");
}

#[test]
fn test_quorum_proof_validations() {
    // Create a list of authorized DIDs
    let authorized_dids = vec![
        "did:key:issuer1".to_string(),
        "did:key:issuer2".to_string(),
        "did:key:issuer3".to_string(),
        "did:key:issuer4".to_string(),
        "did:key:issuer5".to_string(),
    ];

    // Test majority rule
    let majority_config = QuorumConfig::new_majority(authorized_dids.clone());
    let signers = vec![
        "did:key:issuer1".to_string(),
        "did:key:issuer2".to_string(),
        "did:key:issuer3".to_string(),
    ];

    let result = majority_config.validate_quorum(&signers);
    assert!(result.is_ok(), "Majority validation failed: {:?}", result);
    assert!(result.unwrap(), "Majority quorum not satisfied");

    // Test with insufficient signers
    let insufficient_signers = vec!["did:key:issuer1".to_string(), "did:key:issuer2".to_string()];

    let result = majority_config.validate_quorum(&insufficient_signers);
    assert!(result.is_ok(), "Validation failed: {:?}", result);
    assert!(!result.unwrap(), "Quorum should not be satisfied");

    // Test threshold rule
    let threshold_config = QuorumConfig::new_threshold(authorized_dids.clone(), 60).unwrap();

    // 3 out of 5 = 60%
    let result = threshold_config.validate_quorum(&signers);
    assert!(result.is_ok(), "Threshold validation failed: {:?}", result);
    assert!(result.unwrap(), "Threshold quorum not satisfied");

    // Test weighted rule
    let mut weights = HashMap::new();
    weights.insert("did:key:issuer1".to_string(), 10);
    weights.insert("did:key:issuer2".to_string(), 5);
    weights.insert("did:key:issuer3".to_string(), 3);
    weights.insert("did:key:issuer4".to_string(), 1);
    weights.insert("did:key:issuer5".to_string(), 1);

    let weighted_config = QuorumConfig::new_weighted(weights, 15).unwrap();

    // issuer1 + issuer2 = 15 weight
    let weighted_signers = vec!["did:key:issuer1".to_string(), "did:key:issuer2".to_string()];

    let result = weighted_config.validate_quorum(&weighted_signers);
    assert!(result.is_ok(), "Weighted validation failed: {:?}", result);
    assert!(result.unwrap(), "Weighted quorum not satisfied");

    println!("All quorum validation tests passed!");
}

// Helper function to create a test credential
fn create_test_credential(id: &str, issuer: &str, subject: &str) -> VerifiableCredential {
    VerifiableCredential {
        context: vec!["https://www.w3.org/2018/credentials/v1".to_string()],
        id: id.to_string(),
        type_: vec!["VerifiableCredential".to_string()],
        issuer: issuer.to_string(),
        issuance_date: "2023-01-01T00:00:00Z".to_string(),
        expiration_date: None,
        credential_subject: CredentialSubject {
            id: subject.to_string(),
            claims: HashMap::new(),
        },
        proof: CredentialProof {
            type_: "Ed25519Signature2020".to_string(),
            created: "2023-01-01T00:00:00Z".to_string(),
            verification_method: format!("{}#key1", issuer),
            proof_purpose: "assertionMethod".to_string(),
            jws: "header..signature".to_string(),
        },
    }
}
