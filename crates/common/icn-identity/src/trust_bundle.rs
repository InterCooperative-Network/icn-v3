use crate::{Did, QuorumError, QuorumProof};
use cid::Cid;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur with TrustBundles.
#[derive(Debug, Error)]
pub enum TrustBundleError {
    #[error("invalid quorum proof: {0}")]
    QuorumError(#[from] QuorumError),
    
    #[error("serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("CID parse error: {0}")]
    CidError(String),
    
    #[error("missing required field: {0}")]
    MissingField(String),
}

/// Federation metadata containing essential information about a federation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMetadata {
    /// Name of the federation
    pub name: String,
    
    /// Description of the federation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    
    /// Version of the federation metadata schema
    pub version: String,
    
    /// Additional custom metadata fields
    #[serde(flatten)]
    pub additional: HashMap<String, Value>,
}

/// A bundle of trust information that forms the basis of trust in a federation.
/// 
/// This bundle is signed by a quorum of signers and serves as the 
/// cryptographic root of trust for the federation. The bundle's hash is 
/// anchored in the DAG to provide tamper-proof verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustBundle {
    /// CID of the root DAG block
    pub root_dag_cid: String,
    
    /// Metadata about the federation
    pub federation_metadata: FederationMetadata,
    
    /// Proof that a quorum of signers have signed this bundle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quorum_proof: Option<QuorumProof>,
}

impl TrustBundle {
    /// Creates a new TrustBundle with the given DAG CID and federation metadata.
    pub fn new(root_dag_cid: String, federation_metadata: FederationMetadata) -> Self {
        Self {
            root_dag_cid,
            federation_metadata,
            quorum_proof: None,
        }
    }
    
    /// Parse a CID from the root_dag_cid string.
    pub fn parse_cid(&self) -> Result<Cid, TrustBundleError> {
        Cid::try_from(self.root_dag_cid.as_str())
            .map_err(|e| TrustBundleError::CidError(e.to_string()))
    }
    
    /// Calculates a deterministic hash of the bundle for signing.
    /// This hash includes the DAG CID and federation metadata, but NOT the quorum proof.
    pub fn calculate_hash(&self) -> Result<Vec<u8>, TrustBundleError> {
        // Create a temporary bundle without the quorum proof for hashing
        let hash_bundle = TrustBundle {
            root_dag_cid: self.root_dag_cid.clone(),
            federation_metadata: self.federation_metadata.clone(),
            quorum_proof: None,
        };
        
        // Serialize to JSON in a deterministic order
        let bytes = serde_json::to_vec(&hash_bundle)?;
        Ok(bytes)
    }
    
    /// Add a quorum proof to this bundle.
    pub fn add_quorum_proof(&mut self, proof: QuorumProof) {
        self.quorum_proof = Some(proof);
    }
    
    /// Verifies the trust bundle by checking the quorum proof against the bundle hash.
    pub fn verify(&self, allowed_signers: &HashMap<Did, VerifyingKey>) -> Result<(), TrustBundleError> {
        // Ensure we have a quorum proof
        let proof = self.quorum_proof.as_ref()
            .ok_or_else(|| TrustBundleError::MissingField("quorum_proof".to_string()))?;
        
        // Calculate the hash that should have been signed
        let hash = self.calculate_hash()?;
        
        // Verify the quorum proof against the hash
        proof.verify(&hash, allowed_signers)
            .map_err(|e| TrustBundleError::QuorumError(e))
    }
} 