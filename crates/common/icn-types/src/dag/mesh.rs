use cid::Cid;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A wrapper for anchoring mesh receipts in the DAG.
///
/// This provides a standard format for mesh receipts to be stored
/// in the DAG, allowing for queries and verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct ReceiptNode {
    /// The CID of the receipt
    #[serde(
        serialize_with = "crate::dag::serialize_cid",
        deserialize_with = "crate::dag::deserialize_cid"
    )]
    pub receipt_cid: Cid,
    
    /// The CBOR-encoded receipt bytes
    pub receipt_cbor: Vec<u8>,
    
    /// The timestamp of when this receipt was anchored
    pub anchor_timestamp: u64,
    
    /// The federation ID that anchored this receipt
    pub federation_id: String,
}

impl ReceiptNode {
    /// Create a new receipt node
    pub fn new(receipt_cid: Cid, receipt_cbor: Vec<u8>, federation_id: String) -> Self {
        Self {
            receipt_cid,
            receipt_cbor,
            anchor_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs(),
            federation_id,
        }
    }
}

impl fmt::Display for ReceiptNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ReceiptNode {{ cid: {}, size: {} bytes, federation: {} }}",
            self.receipt_cid,
            self.receipt_cbor.len(),
            self.federation_id
        )
    }
} 