use crate::ExecutionReceipt;
use icn_identity::{KeyPair, Signature};
use ed25519_dalek::Verifier;
use serde_cbor;
use thiserror::Error;

/// Errors that can occur during receipt signing operations
#[derive(Debug, Error)]
pub enum SignError {
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Invalid signature")]
    InvalidSignature,
}

/// Sign an ExecutionReceipt to prove authenticity
/// 
/// This function uses CBOR serialization to deterministically
/// serialize the receipt for signing.
pub fn sign_receipt(receipt: &ExecutionReceipt, kp: &KeyPair) -> Result<Signature, SignError> {
    let cbor_bytes = serde_cbor::to_vec(&receipt)
        .map_err(|e| SignError::Serialization(e.to_string()))?;
    
    Ok(kp.sign(&cbor_bytes))
}

/// Verify the signature of an ExecutionReceipt
///
/// This function uses CBOR serialization to deterministically
/// serialize the receipt for verification.
pub fn verify_receipt(receipt: &ExecutionReceipt, signature: &Signature) -> Result<bool, SignError> {
    // Get CBOR serialization of the receipt
    let cbor_bytes = serde_cbor::to_vec(&receipt)
        .map_err(|e| SignError::Serialization(e.to_string()))?;
    
    // Verify using the executor's public key in the receipt
    let executor_did = &receipt.executor;
    
    // Convert the DID to verifying key
    let verifying_key = executor_did.to_ed25519()
        .map_err(|_| SignError::InvalidSignature)?;
    
    // Verify signature using ed25519_dalek's Verifier trait
    Ok(verifying_key.verify(&cbor_bytes, signature).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExecutionReceipt;
    use chrono::Utc;
    use icn_economics::ResourceType;
    use std::collections::HashMap;
    
    #[test]
    fn test_sign_and_verify_receipt() {
        // Create a test keypair
        let kp = KeyPair::generate();
        
        // Create a simple receipt
        let mut usage = HashMap::new();
        usage.insert(ResourceType::Cpu, 1000);
        
        let receipt = ExecutionReceipt {
            task_cid: "bafybeideputvakentavfc".to_string(),
            executor: kp.did.clone(),
            resource_usage: usage,
            timestamp: Utc::now(),
            signature: Vec::new(), // Empty for now
        };
        
        // Sign the receipt
        let signature = sign_receipt(&receipt, &kp).unwrap();
        
        // Verify the signature
        let is_valid = verify_receipt(&receipt, &signature).unwrap();
        assert!(is_valid, "Signature verification should succeed");
        
        // Try with a different keypair (should fail)
        let another_kp = KeyPair::generate();
        let invalid_sig = sign_receipt(&receipt, &another_kp).unwrap();
        let is_invalid = verify_receipt(&receipt, &invalid_sig).unwrap();
        assert!(!is_invalid, "Verification with wrong signature should fail");
    }
} 