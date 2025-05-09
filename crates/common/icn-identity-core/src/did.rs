use ed25519_dalek::PublicKey;
use multibase::{decode, encode, Base};
use thiserror::Error;

const DID_KEY_PREFIX: &str = "did:key:z";
const ED25519_MULTICODEC_PREFIX: [u8; 2] = [0xed, 0x01];

/// Error types for DID key operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid DID key format")]
    InvalidFormat,

    #[error("Unsupported key type")]
    UnsupportedKeyType,

    #[error("Base encoding/decoding error: {0}")]
    BaseEncoding(String),

    #[error("Public key error: {0}")]
    PublicKeyError(String),
}

/// Result type for DID key operations
pub type Result<T> = std::result::Result<T, Error>;

/// Convert a public key to a did:key identifier
///
/// Follows the format specified in the DID Key Method specification:
/// https://w3c-ccg.github.io/did-method-key/
pub fn did_key_from_pk(pk: &PublicKey) -> String {
    // Get the raw bytes from the public key
    let pk_bytes = pk.to_bytes();

    // Prepend the multicodec prefix for Ed25519
    let mut prefixed_bytes = Vec::with_capacity(ED25519_MULTICODEC_PREFIX.len() + pk_bytes.len());
    prefixed_bytes.extend_from_slice(&ED25519_MULTICODEC_PREFIX);
    prefixed_bytes.extend_from_slice(&pk_bytes);

    // Encode using multibase
    let multibase_encoded = encode(Base::Base58Btc, &prefixed_bytes);

    // Return the DID key format
    format!("did:key:{}", multibase_encoded)
}

/// Extract a public key from a did:key identifier
///
/// Follows the format specified in the DID Key Method specification:
/// https://w3c-ccg.github.io/did-method-key/
pub fn pk_from_did_key(did: &str) -> Result<PublicKey> {
    // Validate the DID key format
    if !did.starts_with(DID_KEY_PREFIX) {
        return Err(Error::InvalidFormat);
    }

    // Extract the multibase-encoded part
    let multibase_encoded = did.trim_start_matches("did:key:");

    // Decode the multibase-encoded value
    let (_, decoded_bytes) =
        decode(multibase_encoded).map_err(|e| Error::BaseEncoding(e.to_string()))?;

    // Validate and extract the multicodec prefix
    if decoded_bytes.len() < 2 + 32
        || decoded_bytes[0] != ED25519_MULTICODEC_PREFIX[0]
        || decoded_bytes[1] != ED25519_MULTICODEC_PREFIX[1]
    {
        return Err(Error::UnsupportedKeyType);
    }

    // Extract the public key bytes (skip the multicodec prefix)
    let key_bytes = &decoded_bytes[2..];

    // Convert to PublicKey
    PublicKey::from_bytes(key_bytes).map_err(|e| Error::PublicKeyError(e.to_string()))
}
