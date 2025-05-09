use ed25519_dalek::{Keypair, PublicKey};
// Removed: use icn_crypto::{sign_detached_jws, verify_detached_jws};
use icn_types::identity::{CredentialProof, VerifiableCredential};
use icn_types::error::VcError as IcnVcError; // Import and alias VcError from icn-types
use serde::{Deserialize, Serialize};
// use serde_json::to_value; // Removed Value
// use thiserror::Error; // Keep this commented/removed for now

// Include the execution receipt module
pub mod execution_receipt;

/// Result type for VC operations, now using VcError from icn-types
pub type Result<T> = std::result::Result<T, IcnVcError>;

/// Metrics collected during execution
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    /// Fuel consumed during execution (a measure of computational resources)
    pub fuel_used: u64,

    /// Number of host calls made
    pub host_calls: u64,

    /// Total bytes read/written through host functions
    pub io_bytes: u64,
}

/// Execution receipt issued after successful execution of a governance proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceiptCredential {
    /// Standard VC context
    #[serde(rename = "@context")]
    pub context: Vec<String>,

    /// Credential ID
    pub id: String,

    /// Credential type
    #[serde(rename = "type")]
    pub type_: Vec<String>,

    /// Issuer of the credential (the federation)
    pub issuer: String,

    /// Issuance date
    pub issuance_date: String,

    /// Credential subject containing receipt data
    pub credential_subject: ExecutionReceiptSubject,

    /// Cryptographic proof
    pub proof: CredentialProof,
}

/// The subject of an execution receipt credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceiptSubject {
    /// Subject identifier (typically the proposal ID)
    pub id: String,

    /// Proposal ID this receipt is for
    pub proposal_id: String,

    /// Content ID (CID) of the executed WASM module
    pub wasm_cid: String,

    /// Content ID (CID) of the source CCL
    pub ccl_cid: String,

    /// Execution metrics
    pub metrics: ExecutionMetrics,

    /// Anchored CIDs during execution
    pub anchored_cids: Vec<String>,

    /// Resource usage during execution
    pub resource_usage: Vec<ResourceUsage>,

    /// Timestamp of execution
    pub timestamp: u64,

    /// DAG epoch of execution
    pub dag_epoch: Option<u64>,

    /// Receipt CID
    pub receipt_cid: Option<String>,
}

/// Resource usage record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Type of resource used
    pub resource_type: String,

    /// Amount of resource used
    pub amount: u64,
}

impl ExecutionReceiptCredential {
    /// Create a new ExecutionReceipt credential
    pub fn new(
        id: String,
        issuer: String,
        proposal_id: String,
        wasm_cid: String,
        ccl_cid: String,
        metrics: ExecutionMetrics,
        anchored_cids: Vec<String>,
        resource_usage: Vec<(String, u64)>,
        timestamp: u64,
        dag_epoch: Option<u64>,
        receipt_cid: Option<String>,
    ) -> Self {
        // Convert resource usage tuples to structured objects
        let resource_usage = resource_usage
            .into_iter()
            .map(|(resource_type, amount)| ResourceUsage {
                resource_type,
                amount,
            })
            .collect();

        let subject = ExecutionReceiptSubject {
            id: proposal_id.clone(),
            proposal_id,
            wasm_cid,
            ccl_cid,
            metrics,
            anchored_cids,
            resource_usage,
            timestamp,
            dag_epoch,
            receipt_cid,
        };

        Self {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://icn.network/credentials/execution-receipt/v1".to_string(),
            ],
            id,
            type_: vec![
                "VerifiableCredential".to_string(),
                "ExecutionReceiptCredential".to_string(),
            ],
            issuer,
            issuance_date: chrono::Utc::now().to_rfc3339(),
            credential_subject: subject,
            proof: CredentialProof {
                type_: "Ed25519Signature2020".to_string(),
                created: chrono::Utc::now().to_rfc3339(),
                verification_method: "".to_string(), // To be filled when signing
                proof_purpose: "assertionMethod".to_string(),
                jws: "".to_string(), // To be filled when signing
            },
        }
    }

    /// Convert to a standard VerifiableCredential
    pub fn to_verifiable_credential(&self) -> Result<VerifiableCredential> {
        // Convert to the generic VerifiableCredential type
        let vc_value = serde_json::to_value(self)?;
        let vc: VerifiableCredential = serde_json::from_value(vc_value)?;
        Ok(vc)
    }

    /// Create a signed ExecutionReceipt credential
    pub fn with_signature(mut self, keypair: &Keypair, verification_method: &str) -> Result<Self> {
        // Convert to VerifiableCredential for standardized signing
        let vc = self.to_verifiable_credential()?;

        // Generate the signature
        let jws = vc.sign(keypair)?;

        // Create the proof
        let proof = CredentialProof {
            type_: "Ed25519Signature2020".to_string(),
            created: chrono::Utc::now().to_rfc3339(),
            verification_method: verification_method.to_string(),
            proof_purpose: "assertionMethod".to_string(),
            jws,
        };

        // Add the proof to the credential
        self.proof = proof;

        Ok(self)
    }

    /// Verify the credential's signature against a public key
    pub fn verify(&self, public_key: &PublicKey) -> Result<()> {
        // Convert to VerifiableCredential for standardized verification
        let vc = self.to_verifiable_credential()?;
        vc.verify(public_key)
    }
}
