use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey, SignatureError as Ed25519SignatureError};
use serde::{Deserialize, Serialize};
use signature::Verifier;
use thiserror::Error;

/// Error types for JWS operations
#[derive(Error, Debug)]
pub enum JwsError {
    #[error("Failed to serialize JWS header or payload: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Base64 encoding/decoding error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("Invalid JWS structure: expected 3 parts separated by '.', found {actual_parts} parts")]
    IncorrectJwsPartsCount { actual_parts: usize },

    #[error("Invalid detached JWS: payload part was expected to be empty but was not")]
    PayloadPresentInDetachedJws,

    #[error("Invalid signature length: expected {expected_len} bytes, found {found_len} bytes")]
    InvalidSignatureLength { expected_len: usize, found_len: usize },

    #[error("Cryptographic signature verification failed: {0}")]
    CryptoVerification(#[from] Ed25519SignatureError),
}

/// Result type for JWS operations
pub type Result<T> = std::result::Result<T, JwsError>;

/// JWS Header structure
#[derive(Serialize, Deserialize)]
struct JwsHeader {
    alg: String,
    typ: String,
}

/// Sign data with a keypair and return a detached JWS
///
/// Returns a string in the format: `<base64url(header)>..<base64url(signature)>`
/// The payload is not included in the detached JWS.
pub fn sign_detached_jws(payload: &[u8], keypair: &SigningKey) -> Result<String> {
    // Create JWS header
    let header = JwsHeader {
        alg: "EdDSA".to_string(),
        typ: "JWT".to_string(),
    };

    // Serialize and encode header
    let header_json = serde_json::to_vec(&header)?;
    let header_b64 = URL_SAFE_NO_PAD.encode(header_json);

    // Create signing input (header + "." + payload)
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload);
    let signing_input = format!("{}.{}", header_b64, payload_b64);

    // Sign
    let signature = keypair.sign(signing_input.as_bytes());
    let signature_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    // Construct detached JWS (header..signature)
    Ok(format!("{}..{}", header_b64, signature_b64))
}

/// Verify a detached JWS against the original payload
///
/// Takes a detached JWS in the format: `<base64url(header)>..<base64url(signature)>`
/// and the original payload to verify.
pub fn verify_detached_jws(
    payload: &[u8],
    detached_jws: &str,
    public_key: &VerifyingKey,
) -> Result<()> {
    // Split detached JWS into components
    let parts: Vec<&str> = detached_jws.split('.').collect();
    if parts.len() != 3 {
        return Err(JwsError::IncorrectJwsPartsCount { actual_parts: parts.len() });
    }
    if !parts[1].is_empty() {
        return Err(JwsError::PayloadPresentInDetachedJws);
    }

    let header_b64 = parts[0];
    let signature_b64 = parts[2];

    // Base64 decode the signature
    let signature_bytes = URL_SAFE_NO_PAD.decode(signature_b64)?;
    let signature_array: &[u8; 64] = signature_bytes
        .as_slice()
        .try_into()
        .map_err(|_| JwsError::InvalidSignatureLength { expected_len: 64, found_len: signature_bytes.len() })?;
    let signature = Signature::from_bytes(signature_array);

    // Reconstitute the signing input
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload);
    let signing_input = format!("{}.{}", header_b64, payload_b64);

    // Verify the signature
    public_key
        .verify(signing_input.as_bytes(), &signature)
        .map_err(JwsError::from)
}
