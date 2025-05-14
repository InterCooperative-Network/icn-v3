#![forbid(unsafe_code)]

mod sign;

pub use sign::{sign_receipt_in_place, verify_embedded_signature, SignError};

use chrono::{DateTime, Utc};
use cid::multihash::MultihashDigest;
use cid::{multihash, Cid};
use icn_economics::ResourceType;
use icn_identity::Did;
use icn_types::mesh::JobStatus;
use icn_types::org::{CommunityId, CooperativeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// Import the trait and payload from icn-types
use anyhow::Result;
use icn_types::receipt_verification::{ExecutionReceiptPayload, VerifiableReceipt}; // For the Result in get_payload_for_signing

/// Error types for receipt operations
#[derive(Debug, Error)]
pub enum ReceiptError {
    #[error("Failed to serialize receipt: {0}")]
    Serialization(String),

    #[error("Failed to generate CID: {0}")]
    CidGeneration(String),

    #[error("Signature error: {0}")]
    Signature(#[from] SignError),
}

/// A verifiable receipt of WASM execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// Identifier of the job this receipt is for.
    pub job_id: String,
    /// DID of the executor node that produced this receipt.
    pub executor: Did,
    /// Status of the job execution.
    pub status: JobStatus,
    /// Optional CID pointing to the primary result data of the job.
    pub result_data_cid: Option<String>,
    /// Optional CID pointing to a collection of execution logs.
    pub logs_cid: Option<String>,
    /// Reported resource usage for the job.
    pub resource_usage: HashMap<ResourceType, u64>,
    /// Optional mana cost incurred for the job execution.
    pub mana_cost: Option<u64>,
    /// Unix timestamp (seconds since epoch) when the job execution started.
    pub execution_start_time: u64,
    /// Unix timestamp (seconds since epoch) when the job execution ended.
    pub execution_end_time: u64,
    /// DateTime<Utc> when the job execution ended (kept for convenience, renamed from timestamp).
    pub execution_end_time_dt: DateTime<Utc>,
    /// Cryptographic signature of the receipt content by the executor.
    pub signature: Vec<u8>,
    /// Optional cooperative ID that this receipt is associated with.
    pub coop_id: Option<CooperativeId>,
    /// Optional community ID that this receipt is associated with.
    pub community_id: Option<CommunityId>,
}

impl ExecutionReceipt {
    /// Generate a CID (Content Identifier) for this receipt
    ///
    /// The CID is a unique identifier based on the content of the receipt.
    /// It uses SHA-256 for hashing and the DAG-CBOR codec (0x71).
    pub fn cid(&self) -> Result<Cid, ReceiptError> {
        // Serialize receipt to CBOR
        let bytes =
            serde_cbor::to_vec(self).map_err(|e| ReceiptError::Serialization(e.to_string()))?;

        // Generate multihash using SHA-256
        let hash = multihash::Code::Sha2_256.digest(&bytes);

        // Create CID with DAG-CBOR codec (0x71)
        Ok(Cid::new_v1(0x71, hash))
    }
}

impl VerifiableReceipt for ExecutionReceipt {
    fn get_payload_for_signing(&self) -> Result<ExecutionReceiptPayload> {
        Ok(ExecutionReceiptPayload {
            id: self.job_id.clone(),
            issuer: self.executor.to_string(), // Convert Did to String
            proposal_id: None, // MeshExecutionReceipt doesn't have a direct proposal_id
            wasm_cid: None,    // MeshExecutionReceipt doesn't have wasm_cid directly
            // (it's often part of the job definition, not the receipt itself)
            ccl_cid: None, // Similarly, ccl_cid is not directly on the receipt
            timestamp: self.execution_end_time, // u64 Unix timestamp
        })
    }

    fn get_signature_bytes(&self) -> Option<&[u8]> {
        Some(&self.signature) // signature is not Option<Vec<u8>>, it's Vec<u8>
    }

    fn get_issuer_did_str(&self) -> &str {
        self.executor.as_str() // Assuming Did has an as_str() method or similar
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_types::mesh::JobStatus;

    #[test]
    fn test_json_roundtrip() {
        let mut usage = HashMap::new();
        usage.insert(ResourceType::Cpu, 1000);

        // Use a generated KeyPair to get a valid DID
        let kp = KeyPair::generate();

        let receipt = ExecutionReceipt {
            job_id: "test-cid".to_string(),
            executor: kp.did.clone(),
            status: JobStatus::Completed,
            result_data_cid: None,
            logs_cid: None,
            resource_usage: usage,
            execution_start_time: 1672502400,
            execution_end_time: 1672506000,
            execution_end_time_dt: DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            signature: vec![1, 2, 3, 4],
            coop_id: None,
            community_id: None,
            mana_cost: None,
        };

        let json = serde_json::to_string(&receipt).unwrap();
        let deserialized: ExecutionReceipt = serde_json::from_str(&json).unwrap();

        assert_eq!(receipt, deserialized);
    }

    #[test]
    fn test_cbor_roundtrip() {
        let mut usage = HashMap::new();
        usage.insert(ResourceType::Cpu, 1000);

        // Use a generated KeyPair to get a valid DID
        let kp = KeyPair::generate();

        let receipt = ExecutionReceipt {
            job_id: "test-cid".to_string(),
            executor: kp.did.clone(),
            status: JobStatus::Completed,
            result_data_cid: None,
            logs_cid: None,
            resource_usage: usage,
            execution_start_time: 1672502400,
            execution_end_time: 1672506000,
            execution_end_time_dt: DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            signature: vec![1, 2, 3, 4],
            coop_id: None,
            community_id: None,
            mana_cost: None,
        };

        let cbor = serde_cbor::to_vec(&receipt).unwrap();
        let deserialized: ExecutionReceipt = serde_cbor::from_slice(&cbor).unwrap();

        assert_eq!(receipt, deserialized);
    }

    #[test]
    fn test_cid_generation() {
        let mut usage = HashMap::new();
        usage.insert(ResourceType::Cpu, 500);

        // Create a keypair for signing
        let kp = KeyPair::generate();

        // Create a receipt with fixed timestamp for deterministic testing
        let timestamp = DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let receipt = ExecutionReceipt {
            job_id: "bafybeideputvakentvavfc".to_string(),
            executor: kp.did.clone(),
            status: JobStatus::Completed,
            result_data_cid: None,
            logs_cid: None,
            resource_usage: usage,
            execution_start_time: 1672502400,
            execution_end_time: 1672506000,
            execution_end_time_dt: timestamp,
            signature: vec![9, 8, 7, 6],
            coop_id: None,
            community_id: None,
            mana_cost: None,
        };

        // Generate CID
        let cid = receipt.cid().unwrap();

        // Verify it's a CIDv1 with DAG-CBOR codec
        assert_eq!(cid.version(), cid::Version::V1);
        assert_eq!(cid.codec(), 0x71); // DAG-CBOR

        // Create another receipt with the same values - should get same CID
        let receipt2 = receipt.clone();
        let cid2 = receipt2.cid().unwrap();
        assert_eq!(cid, cid2, "Identical receipts should have the same CID");

        // Change a value - should get different CID
        let mut receipt3 = receipt.clone();
        receipt3.job_id = "different-cid".to_string();
        let cid3 = receipt3.cid().unwrap();
        assert_ne!(cid, cid3, "Different receipts should have different CIDs");
    }
}
