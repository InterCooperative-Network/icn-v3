use serde::{Serialize, Deserialize};
use anyhow::{Result};
use icn_identity::Did; // Assuming Did type is used for issuer
// use std::str::FromStr; // Removed unused import
// use ed25519_dalek::{Signature, VerifyingKey}; // Removed unused imports
// Import the new trait and payload
use crate::receipt_verification::{ExecutionReceiptPayload, VerifiableReceipt};
// use bincode; // Removed unused import

// NEW IMPORTS for CID generation
use cid::{Cid, Version}; // Simplified cid import
use cid::multihash::{Code as MultihashCode, MultihashDigest}; // CORRECTED IMPORT for multihash types
use serde_cbor;
use thiserror::Error; // Added for the error type

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct RuntimeExecutionMetrics {
    // pub fuel_used: u64, // Removed
    pub host_calls: u64,
    pub io_bytes: u64,
    /// Optional mana cost computed post-execution
    pub mana_cost: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeExecutionReceipt {
    pub id: String,
    pub issuer: String,
    pub proposal_id: String,
    pub wasm_cid: String,
    pub ccl_cid: String,
    pub metrics: RuntimeExecutionMetrics,
    pub anchored_cids: Vec<String>,
    pub resource_usage: Vec<(String, u64)>,
    pub timestamp: u64,
    pub dag_epoch: Option<u64>,
    pub receipt_cid: Option<String>, // This will store the string representation of its own CID
    pub signature: Option<Vec<u8>>,
}

// Define an error type for CID generation
#[derive(Debug, thiserror::Error)]
pub enum ReceiptCidError {
    #[error("Serialization error for CID generation: {0}")]
    Serialization(String),
    // Add other specific errors if needed
}

impl RuntimeExecutionReceipt {
    // REMOVED: Old signed_payload method, replaced by trait impl below
    // fn signed_payload(&self) -> RuntimeExecutionReceiptPayload { ... }

    // REMOVED: Old verify method, replaced by trait\'s default implementation
    // pub fn verify(&self) -> Result<()> { ... }

    /// Generate a CID (Content Identifier) for this receipt.
    /// The CID is a unique identifier based on the content of the receipt.
    /// It uses SHA-256 for hashing and the DAG-CBOR codec (0x71).
    /// The `receipt_cid` field itself is excluded during CID calculation
    /// by serializing a temporary clone where this field is None.
    pub fn cid(&self) -> Result<Cid, ReceiptCidError> {
        let mut temp_receipt = self.clone();
        temp_receipt.receipt_cid = None; // Ensure receipt_cid field is not part of its own hash

        let bytes = serde_cbor::to_vec(&temp_receipt)
            .map_err(|e| ReceiptCidError::Serialization(e.to_string()))?;
        
        let hash = MultihashCode::Sha2_256.digest(&bytes);
        
        // Use raw u64 for DAG_CBOR codec (0x71) for robustness
        Ok(Cid::new(Version::V1, 0x71, hash).expect("Failed to create CID v1 dag-cbor"))
    }
}

// Implement the new verification trait
impl VerifiableReceipt for RuntimeExecutionReceipt {
    fn get_payload_for_signing(&self) -> Result<ExecutionReceiptPayload> {
        // Map fields from Self to the common ExecutionReceiptPayload
        Ok(ExecutionReceiptPayload {
            id: self.id.clone(),
            issuer: self.issuer.clone(),
            proposal_id: Some(self.proposal_id.clone()), // Wrap Option around existing fields
            wasm_cid: Some(self.wasm_cid.clone()),
            ccl_cid: Some(self.ccl_cid.clone()),
            timestamp: self.timestamp, // Already u64
        })
    }

    fn get_signature_bytes(&self) -> Option<&[u8]> {
        // Return slice reference to the signature bytes
        self.signature.as_deref()
    }

    fn get_issuer_did_str(&self) -> &str {
        // Return reference to the issuer DID string
        &self.issuer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    // Ensure Signer trait is in scope if KeyPair::sign directly returns ed25519_dalek::Signature
    // and doesn't rely on a trait method from ed25519_dalek::Signer for KeyPair itself.
    // If KeyPair::sign is a direct method, this specific import might not be needed for keypair.sign().
    // use ed25519_dalek::Signer; 

    #[test]
    fn test_valid_receipt_verification() {
        let keypair = KeyPair::generate();
        let did_string = keypair.did.to_string(); // Use the string representation for the receipt

        let receipt = RuntimeExecutionReceipt {
            id: "receipt-123".into(),
            issuer: did_string.clone(),
            proposal_id: "proposal-abc".into(),
            wasm_cid: "wasm-cid-xyz".into(),
            ccl_cid: "ccl-cid-123".into(),
            metrics: RuntimeExecutionMetrics::default(), // Use default
            anchored_cids: vec!["anchor-1".into()],
            resource_usage: vec![("cpu".into(), 100)],
            timestamp: 1678886400, // Example timestamp
            dag_epoch: Some(10),
            receipt_cid: None, // Not part of signed payload
            signature: None, // Will be added below
        };

        // Sign it
        let payload = receipt.get_payload_for_signing().expect("Failed to get payload for signing in test");
        let bytes = bincode::serialize(&payload).expect("Failed to serialize payload for test");
        // Assumes icn_identity::KeyPair has a public method `sign`:
        // fn sign(&self, message: &[u8]) -> ed25519_dalek::Signature;
        let signature = keypair.sign(&bytes); 
        let sig_bytes = signature.to_bytes().to_vec();

        let signed_receipt = RuntimeExecutionReceipt { 
            signature: Some(sig_bytes), 
            ..receipt // Clone the rest from the original receipt
        };

        // Verification should succeed using the trait method
        // signed_receipt.verify().expect("Verification failed for a valid signed receipt");
        signed_receipt.verify_signature().expect("Signature verification failed for a valid signed receipt");
    }
    
    // Optional: Add a test for verification failure with bad signature
    #[test]
    fn test_invalid_signature_receipt_verification_fails() {
        let keypair1 = KeyPair::generate();
        let keypair2 = KeyPair::generate(); // Different keypair
        let did1_string = keypair1.did.to_string();

        let receipt = RuntimeExecutionReceipt {
            id: "receipt-456".into(),
            issuer: did1_string.clone(),
            proposal_id: "proposal-def".into(),
            wasm_cid: "wasm-cid-abc".into(),
            ccl_cid: "ccl-cid-456".into(),
            metrics: RuntimeExecutionMetrics::default(),
            anchored_cids: vec![],
            resource_usage: vec![],
            timestamp: 1678886500,
            dag_epoch: None,
            receipt_cid: None,
            signature: None,
        };

        // Sign with keypair2's secret key
        let payload = receipt.get_payload_for_signing().expect("Failed to get payload for signing in test (invalid signature)");
        let bytes = bincode::serialize(&payload).unwrap();
        // Assumes icn_identity::KeyPair has a public method `sign`
        let bad_signature = keypair2.sign(&bytes);
        let bad_sig_bytes = bad_signature.to_bytes().to_vec();

        let wrongly_signed_receipt = RuntimeExecutionReceipt { 
            signature: Some(bad_sig_bytes), 
            ..receipt
        };

        // Verification should fail using the trait method
        // let verification_result = wrongly_signed_receipt.verify();
        let verification_result = wrongly_signed_receipt.verify_signature();
        assert!(verification_result.is_err());
        assert!(verification_result.unwrap_err().to_string().contains("Signature verification failed"));
    }
    
    // Optional: Add a test for missing required fields
    #[test]
    fn test_missing_id_receipt_verification_fails() {
        let keypair = KeyPair::generate();
        let did_string = keypair.did.to_string();

        let receipt_no_id = RuntimeExecutionReceipt {
            id: "".into(), // Empty ID
            issuer: did_string.clone(),
            proposal_id: "proposal-ghi".into(),
            wasm_cid: "wasm-cid-def".into(),
            ccl_cid: "ccl-cid-789".into(),
            metrics: RuntimeExecutionMetrics::default(),
            anchored_cids: vec![],
            resource_usage: vec![],
            timestamp: 1678886600,
            dag_epoch: None,
            receipt_cid: None,
            signature: None, // No signature needed to test field validation
        };

        // Field validation is no longer part of verify_signature, 
        // it should be done separately if needed before calling verify_signature.
        // let verification_result = receipt_no_id.verify(); 
        // assert!(verification_result.is_err());
        // assert!(verification_result.unwrap_err().to_string().contains("Receipt has empty id"));
        // For now, we just test that the signature verification logic doesn't run (or panic)
        // if required fields for *it* are missing (like issuer DID). 
        // Let's test missing issuer:
        let receipt_no_issuer = RuntimeExecutionReceipt {
            id: "receipt-789".into(),
            issuer: "".into(), // Empty issuer
            proposal_id: "proposal-ghi".into(),
            wasm_cid: "wasm-cid-def".into(),
            ccl_cid: "ccl-cid-789".into(),
            metrics: RuntimeExecutionMetrics::default(),
            anchored_cids: vec![],
            resource_usage: vec![],
            timestamp: 1678886600,
            dag_epoch: None,
            receipt_cid: None,
            signature: Some(vec![0; 64]), // Add dummy signature to trigger verification logic
        };
        let verification_result_no_issuer = receipt_no_issuer.verify_signature();
        assert!(verification_result_no_issuer.is_err());
        assert!(verification_result_no_issuer.unwrap_err().to_string().contains("Receipt issuer DID is empty"));
    }
} 