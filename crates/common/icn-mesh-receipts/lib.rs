#![forbid(unsafe_code)]

mod sign;

pub use sign::{sign_receipt, verify_receipt, SignError};

use chrono::{DateTime, Utc};
use cid::{Cid, multihash};
use cid::multihash::MultihashDigest;
use icn_economics::ResourceType;
use icn_identity::Did;
use icn_types::org::{CooperativeId, CommunityId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use thiserror::Error;

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
    pub task_cid: String,
    pub executor: Did,
    pub resource_usage: HashMap<ResourceType, u64>,
    pub timestamp: DateTime<Utc>,
    pub signature: Vec<u8>,
    /// Optional cooperative ID that this receipt is associated with
    pub coop_id: Option<CooperativeId>,
    /// Optional community ID that this receipt is associated with
    pub community_id: Option<CommunityId>,
}

impl ExecutionReceipt {
    /// Generate a CID (Content Identifier) for this receipt
    /// 
    /// The CID is a unique identifier based on the content of the receipt.
    /// It uses SHA-256 for hashing and the DAG-CBOR codec (0x71).
    pub fn cid(&self) -> Result<Cid, ReceiptError> {
        // Serialize receipt to CBOR
        let bytes = serde_cbor::to_vec(self)
            .map_err(|e| ReceiptError::Serialization(e.to_string()))?;
        
        // Generate multihash using SHA-256
        let hash = multihash::Code::Sha2_256.digest(&bytes);
        
        // Create CID with DAG-CBOR codec (0x71)
        Ok(Cid::new_v1(0x71, hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_economics::ResourceType;
    use icn_identity::KeyPair;
    use serde_cbor;
    use serde_json;

    #[test]
    fn test_json_roundtrip() {
        let mut usage = HashMap::new();
        usage.insert(ResourceType::Cpu, 1000);
        
        let receipt = ExecutionReceipt {
            task_cid: "test-cid".to_string(),
            executor: Did::from_str("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap(),
            resource_usage: usage,
            timestamp: Utc::now(),
            signature: vec![1, 2, 3, 4],
            coop_id: None,
            community_id: None,
        };
        
        let json = serde_json::to_string(&receipt).unwrap();
        let deserialized: ExecutionReceipt = serde_json::from_str(&json).unwrap();
        
        assert_eq!(receipt, deserialized);
    }
    
    #[test]
    fn test_cbor_roundtrip() {
        let mut usage = HashMap::new();
        usage.insert(ResourceType::Cpu, 1000);
        
        let receipt = ExecutionReceipt {
            task_cid: "test-cid".to_string(),
            executor: Did::from_str("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").unwrap(),
            resource_usage: usage,
            timestamp: Utc::now(),
            signature: vec![1, 2, 3, 4],
            coop_id: None,
            community_id: None,
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
        let timestamp = DateTime::parse_from_rfc3339("2023-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let receipt = ExecutionReceipt {
            task_cid: "bafybeideputvakentvavfc".to_string(),
            executor: kp.did.clone(),
            resource_usage: usage,
            timestamp,
            signature: vec![9, 8, 7, 6],
            coop_id: None,
            community_id: None,
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
        receipt3.task_cid = "different-cid".to_string();
        let cid3 = receipt3.cid().unwrap();
        assert_ne!(cid, cid3, "Different receipts should have different CIDs");
    }
} 