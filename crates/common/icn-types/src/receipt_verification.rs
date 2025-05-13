use serde::{Serialize, Deserialize};
use anyhow::Result;
use ed25519_dalek::{Signature, VerifyingKey};
use icn_identity::Did;
use std::str::FromStr;
use crate::bincode; // Assuming bincode is available in icn-types crate dependencies

// Common payload structure for signing and verification across receipt types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionReceiptPayload {
    /// Receipt ID (e.g., UUID) or Job ID it corresponds to
    pub id: String,                      
    /// DID string of the node that executed the job/task and issued the receipt
    pub issuer: String,                 
    /// Optional identifier linking this receipt back to a specific proposal
    pub proposal_id: Option<String>,    
    /// Optional CID of the primary WASM module executed
    pub wasm_cid: Option<String>,       
    /// Optional CID of the associated CCL module executed (if applicable)
    pub ccl_cid: Option<String>,        
    /// Timestamp marking the completion of the execution (Unix epoch seconds)
    pub timestamp: u64,                 
}

/// Trait for receipts that can be cryptographically verified
pub trait VerifiableReceipt {
    /// Get the specific data payload that was signed to produce the signature.
    fn get_payload_for_signing(&self) -> Result<ExecutionReceiptPayload>;
    
    /// Get the raw signature bytes associated with this receipt.
    fn get_signature_bytes(&self) -> Option<&[u8]>;
    
    /// Get the DID string of the entity that allegedly signed this receipt.
    fn get_issuer_did_str(&self) -> &str;

    /// Verify the signature against the payload using the issuer's public key.
    /// This provides a default implementation.
    fn verify_signature(&self) -> Result<()> {
        let sig_bytes = self
            .get_signature_bytes()
            .ok_or_else(|| anyhow::anyhow!("Receipt signature is missing"))?;

        let issuer_did_str = self.get_issuer_did_str();
        if issuer_did_str.is_empty() {
             return Err(anyhow::anyhow!("Receipt issuer DID is empty"));
        }
        
        // Parse the issuer DID string
        let issuer_did = Did::from_str(issuer_did_str)
            .map_err(|e| anyhow::anyhow!("Invalid issuer DID format '{}': {}", issuer_did_str, e))?;
        
        // Get the verification key from the DID (requires Did::verifying_key() method)
        // TODO: Ensure icn_identity::Did implements verifying_key() -> Result<VerifyingKey>
        let verifying_key: VerifyingKey = issuer_did.verifying_key()
            .map_err(|e| anyhow::anyhow!("Failed to get verifying key for DID '{}': {}", issuer_did_str, e))?;

        // Get and serialize the payload that should have been signed
        let payload = self.get_payload_for_signing()?;
        let serialized_payload = bincode::serialize(&payload)
            .map_err(|e| anyhow::anyhow!("Failed to serialize receipt payload for verification: {}", e))?;

        // Parse the signature from bytes
        let signature = Signature::try_from(sig_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid signature byte format: {}", e))?;

        // Perform cryptographic verification
        verifying_key
            .verify_strict(&serialized_payload, &signature)
            .map_err(|e| anyhow::anyhow!("Signature verification failed for issuer '{}': {}", issuer_did_str, e))?;

        Ok(())
    }
} 