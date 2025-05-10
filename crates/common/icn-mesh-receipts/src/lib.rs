#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use icn_economics::ResourceType;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A verifiable receipt of WASM execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub task_cid: String,
    pub executor: Did,
    pub resource_usage: HashMap<ResourceType, u64>,
    pub timestamp: DateTime<Utc>,
    pub signature: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_economics::ResourceType;
    use serde_cbor;
    use serde_json;

    #[test]
    fn test_json_roundtrip() {
        let mut usage = HashMap::new();
        usage.insert(ResourceType::CpuTime, 1000);
        
        let receipt = ExecutionReceipt {
            task_cid: "test-cid".to_string(),
            executor: Did::from_string("did:icn:test").unwrap(),
            resource_usage: usage,
            timestamp: Utc::now(),
            signature: vec![1, 2, 3, 4],
        };
        
        let json = serde_json::to_string(&receipt).unwrap();
        let deserialized: ExecutionReceipt = serde_json::from_str(&json).unwrap();
        
        assert_eq!(receipt, deserialized);
    }
    
    #[test]
    fn test_cbor_roundtrip() {
        let mut usage = HashMap::new();
        usage.insert(ResourceType::CpuTime, 1000);
        
        let receipt = ExecutionReceipt {
            task_cid: "test-cid".to_string(),
            executor: Did::from_string("did:icn:test").unwrap(),
            resource_usage: usage,
            timestamp: Utc::now(),
            signature: vec![1, 2, 3, 4],
        };
        
        let cbor = serde_cbor::to_vec(&receipt).unwrap();
        let deserialized: ExecutionReceipt = serde_cbor::from_slice(&cbor).unwrap();
        
        assert_eq!(receipt, deserialized);
    }
} 