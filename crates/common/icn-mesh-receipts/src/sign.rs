use crate::ExecutionReceipt;
use ed25519_dalek::Signature as DalekSignature;
use icn_identity::KeyPair;
use serde_cbor;
use signature::Verifier;
use thiserror::Error;

/// Errors that can occur during receipt signing operations
#[derive(Debug, Error)]
pub enum SignError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("DID conversion error: {0}")]
    DidConversion(String),
}

/// Creates the canonical byte representation of the receipt for signing or verification.
/// This involves temporarily emptying the signature field before serialization.
fn get_receipt_signing_payload(receipt: &ExecutionReceipt) -> Result<Vec<u8>, SignError> {
    let mut receipt_clone = receipt.clone();
    receipt_clone.signature = Vec::new(); // Ensure signature field is empty for payload generation
    serde_cbor::to_vec(&receipt_clone).map_err(|e| {
        SignError::Serialization(format!("Failed to serialize receipt for payload: {}", e))
    })
}

/// Sign an ExecutionReceipt to prove authenticity and store the signature within the receipt.
///
/// This function uses CBOR serialization to deterministically
/// serialize the receipt (with an empty signature field) for signing.
pub fn sign_receipt_in_place(
    receipt: &mut ExecutionReceipt,
    kp: &KeyPair,
) -> Result<(), SignError> {
    // Ensure the DID in the receipt matches the keypair trying to sign it.
    if receipt.executor != kp.did {
        return Err(SignError::InvalidSignature(format!(
            "KeyPair DID '{}' does not match receipt executor DID '{}'",
            kp.did, receipt.executor
        )));
    }

    let payload_bytes = get_receipt_signing_payload(receipt)?;
    let dalek_signature: DalekSignature = kp.sign(&payload_bytes);
    receipt.signature = dalek_signature.to_bytes().to_vec(); // Store as Vec<u8>
    Ok(())
}

/// Verify the signature embedded within an ExecutionReceipt.
///
/// This function reconstructs the original signing payload by temporarily
/// emptying the signature field of a cloned receipt before verification.
pub fn verify_embedded_signature(receipt: &ExecutionReceipt) -> Result<bool, SignError> {
    if receipt.signature.is_empty() {
        return Err(SignError::InvalidSignature(
            "Receipt has no signature to verify.".to_string(),
        ));
    }

    let payload_bytes = get_receipt_signing_payload(receipt)?;

    let signature_bytes: &[u8; 64] =
        receipt.signature.as_slice().try_into().map_err(|_| {
            SignError::InvalidSignature("Signature is not 64 bytes long".to_string())
        })?;

    let dalek_signature = DalekSignature::from_bytes(signature_bytes);

    let verifying_key = receipt.executor.to_ed25519().map_err(|e| {
        SignError::DidConversion(format!("Failed to convert DID to ed25519 key: {}", e))
    })?;

    Ok(verifying_key
        .verify(&payload_bytes, &dalek_signature)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExecutionReceipt;
    use chrono::Utc;
    use icn_economics::ResourceType;
    use icn_types::mesh::JobStatus;
    use std::collections::HashMap;

    // Helper to create a basic receipt for testing
    fn create_test_receipt(kp: &KeyPair) -> ExecutionReceipt {
        let mut usage = HashMap::new();
        usage.insert(ResourceType::Cpu, 1000);
        let now = Utc::now();

        ExecutionReceipt {
            job_id: "test_job_123".to_string(),
            executor: kp.did.clone(),
            status: JobStatus::Completed,
            result_data_cid: Some("mock_result_cid".to_string()),
            logs_cid: Some("mock_logs_cid".to_string()),
            resource_usage: usage,
            execution_start_time: now.timestamp() as u64 - 60,
            execution_end_time: now.timestamp() as u64,
            execution_end_time_dt: now,
            signature: Vec::new(),
            coop_id: None,
            community_id: None,
        }
    }

    #[test]
    fn test_sign_and_verify_receipt_in_place() {
        let kp = KeyPair::generate();
        let mut receipt = create_test_receipt(&kp);

        // Sign the receipt in place
        sign_receipt_in_place(&mut receipt, &kp).expect("Signing failed");
        assert!(
            !receipt.signature.is_empty(),
            "Signature should not be empty after signing"
        );

        // Verify the embedded signature
        let is_valid = verify_embedded_signature(&receipt).expect("Verification failed");
        assert!(is_valid, "Signature verification should succeed");

        // Test with a different keypair (should fail verification if we could tamper with DID)
        // More practically, tamper with the signature or payload
        let mut tampered_receipt = receipt.clone();
        tampered_receipt.signature[0] = tampered_receipt.signature[0].wrapping_add(1); // Corrupt signature
        let is_tampered_valid = verify_embedded_signature(&tampered_receipt);
        assert!(
            is_tampered_valid.is_err() || !is_tampered_valid.unwrap(),
            "Verification of tampered signature should fail or error"
        );

        // Test with a different DID in a cloned receipt (signing should fail if we try to sign with wrong KP)
        let another_kp = KeyPair::generate();
        let mut receipt_for_other_kp = create_test_receipt(&another_kp); // executor DID is now another_kp.did
                                                                         // Try to sign with original kp - should fail due to DID mismatch
        let sign_mismatch_result = sign_receipt_in_place(&mut receipt_for_other_kp, &kp);
        assert!(
            sign_mismatch_result.is_err(),
            "Signing with mismatched KeyPair DID and executor DID should fail"
        );

        // Sign correctly with its own keypair
        sign_receipt_in_place(&mut receipt_for_other_kp, &another_kp)
            .expect("Signing with correct keypair failed");
        // Try to verify this receipt (signed by another_kp) using the original receipt's context (implicitly, if verify_embedded_signature was using a DID from outside)
        // verify_embedded_signature correctly uses the DID *from the receipt itself*, so this test is fine.
        let is_valid_other = verify_embedded_signature(&receipt_for_other_kp)
            .expect("Verification of other receipt failed");
        assert!(
            is_valid_other,
            "Verification of correctly signed other receipt should succeed"
        );
    }

    #[test]
    fn test_verify_empty_signature() {
        let kp = KeyPair::generate();
        let receipt_no_sig = create_test_receipt(&kp); // signature is Vec::new()
        let verification_result = verify_embedded_signature(&receipt_no_sig);
        assert!(verification_result.is_err());
        match verification_result.unwrap_err() {
            SignError::InvalidSignature(msg) => {
                assert!(msg.contains("Receipt has no signature to verify"))
            }
            _ => panic!("Expected InvalidSignature error for empty signature verification"),
        }
    }
}
