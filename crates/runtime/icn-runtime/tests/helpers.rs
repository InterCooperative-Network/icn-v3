#![allow(dead_code)]

use anyhow::Result;
use icn_identity::{Did, FederationMetadata, KeyPair, QuorumProof, QuorumType, TrustBundle};
use std::collections::HashMap;
use std::path::Path;

/// Generate a specified number of signer keypairs
pub fn generate_signers(n: usize) -> Vec<KeyPair> {
    (0..n).map(|_| KeyPair::generate()).collect()
}

/// Create a map of allowed signers from keypairs
pub fn create_signer_map(keypairs: &[KeyPair]) -> HashMap<Did, ed25519_dalek::VerifyingKey> {
    let mut signers = HashMap::new();
    for kp in keypairs {
        signers.insert(kp.did.clone(), kp.pk);
    }
    signers
}

/// Create a federation trust bundle with the given signers
pub fn create_trust_bundle(
    keypairs: &[KeyPair],
    name: &str,
    description: Option<&str>,
) -> Result<TrustBundle> {
    // Create federation metadata
    let metadata = FederationMetadata {
        name: name.to_string(),
        description: description.map(String::from),
        version: "1.0".to_string(),
        additional: HashMap::new(),
    };

    // Create a trust bundle with a test DAG CID
    let mut bundle = TrustBundle::new(
        format!("bafybeiczsscdsbs7ffqz55asqdf3smv6klcw3gofszvwlyarci47bgf354"),
        metadata,
    );

    // Calculate the hash for signing
    let bundle_hash = bundle.calculate_hash()?;

    // Create signatures from the signers (all of them)
    let signatures = keypairs
        .iter()
        .map(|kp| (kp.did.clone(), kp.sign(&bundle_hash)))
        .collect();

    // Create a majority quorum proof
    let proof = QuorumProof::new(QuorumType::Majority, signatures);

    // Add the proof to the bundle
    bundle.add_quorum_proof(proof);

    Ok(bundle)
}

/// Save a trust bundle to a file
pub fn save_trust_bundle(bundle: &TrustBundle, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(bundle)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Load a trust bundle from a file
pub fn load_trust_bundle(path: &Path) -> Result<TrustBundle> {
    let json = std::fs::read_to_string(path)?;
    let bundle: TrustBundle = serde_json::from_str(&json)?;
    Ok(bundle)
}

/// NoopTrustValidator for testing - always validates
pub struct NoopTrustValidator {
    pub is_valid: bool,
}

impl NoopTrustValidator {
    pub fn new(is_valid: bool) -> Self {
        Self { is_valid }
    }

    pub fn always_valid() -> Self {
        Self { is_valid: true }
    }
}
