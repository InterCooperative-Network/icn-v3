use anyhow::Result;
use bincode;
use ed25519_dalek::{Signature, VerifyingKey};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::str::FromStr; // Use the crate directly

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
        let issuer_did = Did::from_str(issuer_did_str).map_err(|e| {
            anyhow::anyhow!("Invalid issuer DID format '{}': {}", issuer_did_str, e)
        })?;

        // Get the verification key from the DID (requires Did::verifying_key() method)
        // TODO: Ensure icn_identity::Did implements verifying_key() -> Result<VerifyingKey>
        let verifying_key: VerifyingKey = issuer_did.verifying_key().map_err(|e| {
            anyhow::anyhow!(
                "Failed to get verifying key for DID '{}': {}",
                issuer_did_str,
                e
            )
        })?;

        // Get and serialize the payload that should have been signed
        let payload = self.get_payload_for_signing()?;
        let serialized_payload = bincode::serialize(&payload).map_err(|e| {
            anyhow::anyhow!(
                "Failed to serialize receipt payload for verification: {}",
                e
            )
        })?;

        // Parse the signature from bytes
        let signature = Signature::try_from(sig_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid signature byte format: {}", e))?;

        // Perform cryptographic verification
        verifying_key
            .verify_strict(&serialized_payload, &signature)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Signature verification failed for issuer '{}': {}",
                    issuer_did_str,
                    e
                )
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    // A mock struct that implements VerifiableReceipt for testing purposes
    #[derive(Clone)]
    struct MockReceipt {
        id: String,
        issuer_did_str: String,
        timestamp: u64,
        signature_bytes: Option<Vec<u8>>,
        // Other fields that might be part of its actual payload
        proposal_id_val: Option<String>,
        wasm_cid_val: Option<String>,
        ccl_cid_val: Option<String>,
    }

    impl VerifiableReceipt for MockReceipt {
        fn get_payload_for_signing(&self) -> Result<ExecutionReceiptPayload> {
            Ok(ExecutionReceiptPayload {
                id: self.id.clone(),
                issuer: self.issuer_did_str.clone(),
                proposal_id: self.proposal_id_val.clone(),
                wasm_cid: self.wasm_cid_val.clone(),
                ccl_cid: self.ccl_cid_val.clone(),
                timestamp: self.timestamp,
            })
        }

        fn get_signature_bytes(&self) -> Option<&[u8]> {
            self.signature_bytes.as_deref()
        }

        fn get_issuer_did_str(&self) -> &str {
            &self.issuer_did_str
        }
    }

    // Helper to create a valid, signed MockReceipt
    fn create_valid_signed_mock_receipt(keypair: &KeyPair) -> MockReceipt {
        let mut receipt = MockReceipt {
            id: "test-receipt-123".to_string(),
            issuer_did_str: keypair.did.to_string(),
            timestamp: 1678886400,
            signature_bytes: None,
            proposal_id_val: Some("prop-abc".to_string()),
            wasm_cid_val: Some("wasm-xyz".to_string()),
            ccl_cid_val: Some("ccl-123".to_string()),
        };
        let payload = receipt.get_payload_for_signing().unwrap();
        let bytes_to_sign = bincode::serialize(&payload).unwrap();
        let signature = keypair.sign(&bytes_to_sign);
        receipt.signature_bytes = Some(signature.to_bytes().to_vec());
        receipt
    }

    #[test]
    fn verify_valid_signature_succeeds() {
        let keypair = KeyPair::generate();
        let receipt = create_valid_signed_mock_receipt(&keypair);
        assert!(receipt.verify_signature().is_ok());
    }

    #[test]
    fn verify_tampered_signature_fails() {
        let keypair = KeyPair::generate();
        let mut receipt = create_valid_signed_mock_receipt(&keypair);
        if let Some(sig) = receipt.signature_bytes.as_mut() {
            sig[0] ^= 0xFF; // Flip some bits in the signature
        }
        let result = receipt.verify_signature();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Signature verification failed"));
    }

    #[test]
    fn verify_missing_signature_fails() {
        let keypair = KeyPair::generate();
        let mut receipt = create_valid_signed_mock_receipt(&keypair);
        receipt.signature_bytes = None;
        let result = receipt.verify_signature();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Receipt signature is missing"));
    }

    #[test]
    fn verify_empty_issuer_did_fails() {
        let keypair = KeyPair::generate();
        let mut receipt = create_valid_signed_mock_receipt(&keypair);
        receipt.issuer_did_str = "".to_string();
        let result = receipt.verify_signature();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Receipt issuer DID is empty"));
    }

    #[test]
    fn verify_malformed_issuer_did_fails() {
        let keypair = KeyPair::generate();
        let mut receipt = create_valid_signed_mock_receipt(&keypair);
        receipt.issuer_did_str = "did:key:not_a_valid_multibase_char!".to_string();
        let result = receipt.verify_signature();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid issuer DID format"));
    }

    #[test]
    fn verify_unsupported_did_method_fails() {
        let keypair = KeyPair::generate();
        let mut receipt = create_valid_signed_mock_receipt(&keypair);
        // `Did::from_str` and then `verifying_key` (via to_ed25519) checks for `did:key`
        receipt.issuer_did_str = "did:web:example.com".to_string();
        let result = receipt.verify_signature();
        assert!(result.is_err());
        // The error will come from Did::from_str via Did::verifying_key -> to_ed25519 failing to parse non did:key
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid issuer DID format"));
    }

    #[test]
    fn verify_unsupported_key_type_in_did_key_fails() {
        let keypair = KeyPair::generate();
        let mut receipt = create_valid_signed_mock_receipt(&keypair);
        // Construct a synthetic did:key with a different multicodec prefix (e.g., 0xec for secp256k1)
        // This requires a bit of manual multicodec/multibase encoding knowledge.
        // For simplicity, we'll test a DID string that would cause to_ed25519 to return UnsupportedCodec.
        // A real secp256k1 did:key would be: did:key:zQ3sh... (0xe7 prefix)
        // Let's use a valid multibase string but with a fake prefix that to_ed25519 would reject.
        // prefix 0x12 (sha2-256) + 32 zero bytes, base58btc encoded: z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Y (example)
        receipt.issuer_did_str = "did:key:z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Z2Y".to_string(); // Example non-ed25519 did:key
        let result = receipt.verify_signature();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("UnsupportedCodec"));
    }

    #[test]
    fn verify_malformed_signature_bytes_fails() {
        let keypair = KeyPair::generate();
        let mut receipt = create_valid_signed_mock_receipt(&keypair);
        receipt.signature_bytes = Some(vec![0, 1, 2, 3]); // Too short for an Ed25519 signature
        let result = receipt.verify_signature();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid signature byte format"));
    }
}
