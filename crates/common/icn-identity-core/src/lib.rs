//! # ICN Identity Core [DEPRECATED]
//!
//! This crate is deprecated and will be removed in a future version.
//! Please use `icn-identity` instead, which provides all the same functionality.

// Re-export everything from icn-identity for backwards compatibility
pub use icn_identity::*;

// For specific modules that need backwards compatibility
pub mod did {
    //! DID module re-exported from icn-identity
    pub use icn_identity::Did;
    pub use icn_identity::DidError as Error;
}

pub mod vc {
    //! VC module re-exported from icn-identity
    pub use icn_identity::{VerifiableCredential, SignedCredential, CredentialError as Error};
    
    // Legacy types needed for runtime receipt verification
    pub use super::ExecutionReceiptCredential;
    pub use super::ExecutionMetrics;
    
    /// Result type for VC operations
    pub type Result<T> = std::result::Result<T, Error>;
}

// Legacy types - defined here to maintain backward compatibility
// with existing code using these types

/// Execution metrics from receipts
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionMetrics {
    pub fuel_used: u64,
    pub host_calls: u64,
    pub io_bytes: u64,
}

/// Execution receipt credential for ICN Runtime
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionReceiptCredential {
    /// Unique identifier for this credential
    pub id: String,
    
    /// Issuer DID
    pub issuer: String,
    
    /// Associated proposal ID
    pub proposal_id: String,
    
    /// WASM CID
    pub wasm_cid: String,
    
    /// CCL CID
    pub ccl_cid: String,
    
    /// Execution metrics
    pub metrics: ExecutionMetrics,
    
    /// Anchored CIDs during execution
    pub anchored_cids: Vec<String>,
    
    /// Resource usage during execution
    pub resource_usage: Vec<(String, u64)>,
    
    /// Timestamp of execution
    pub timestamp: u64,
    
    /// DAG epoch of execution
    pub dag_epoch: Option<u64>,
    
    /// Receipt CID (filled after anchoring)
    pub receipt_cid: Option<String>,
    
    /// Signature from the executing federation
    pub signature: Option<String>,
}

/// Subject of an execution receipt
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionReceiptSubject {
    /// Proposal ID
    pub proposal_id: String,
    
    /// CID of the WASM module
    pub wasm_cid: String,
    
    /// CID of the CCL source
    pub ccl_cid: String,
    
    /// Execution metrics
    pub metrics: ExecutionMetrics,
    
    /// Anchored CIDs during execution
    pub anchored_cids: Vec<String>,
    
    /// Resource usage
    pub resource_usage: Vec<(String, u64)>,
    
    /// Timestamp of execution
    pub timestamp: u64,
    
    /// DAG epoch at time of execution
    pub dag_epoch: Option<u64>,
    
    /// CID of the receipt
    pub receipt_cid: Option<String>,
}

impl ExecutionReceiptCredential {
    #[allow(clippy::too_many_arguments)]
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
        signature: Option<String>,
    ) -> Self {
        Self {
            id,
            issuer,
            proposal_id,
            wasm_cid,
            ccl_cid,
            metrics,
            anchored_cids,
            resource_usage,
            timestamp,
            dag_epoch,
            receipt_cid,
            signature,
        }
    }
}
