use crate::error::CryptoError;
use ed25519_dalek::{Signature as DalekSignature, SigningKey, VerifyingKey};
use rand_core::OsRng; // Changed from rand::rngs::OsRng
use serde::{Deserialize, Serialize};
use signature::{Signer as DalekSigner, Verifier as DalekVerifier}; // Aliased to avoid conflict

/// A trait for objects that can sign messages
pub trait Signer {
    /// Sign a message, returning the signature
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, CryptoError>;

    /// Get the public key associated with this signer
    fn public_key(&self) -> Vec<u8>;
}

/// Ed25519 keypair implementation
pub struct Keypair {
    signing_key: SigningKey,
    // VerifyingKey can be derived from SigningKey, so not strictly needed to store
    // but storing it can be convenient if used frequently.
    // For this refactor, we'll derive it when needed or assume it's passed if Keypair is from_verifying_key.
}

impl Clone for Keypair {
    fn clone(&self) -> Self {
        // SigningKey::from_bytes takes &[u8], SecretKey::as_bytes() returns [u8; 32]
        // Need to ensure the types match or handle conversion.
        // For simplicity, assuming SigningKey can be reconstructed if we have its bytes.
        // ed25519_dalek::SigningKey itself is cloneable if the feature "std" or "alloc" is enabled for it.
        // Let's assume it's cloneable for now for a simpler refactor.
        // If not, we'd use `SigningKey::from_bytes(&self.signing_key.to_bytes())`
        Self {
            signing_key: self.signing_key.clone(),
        }
    }
}

impl Keypair {
    /// Generate a new random keypair
    pub fn generate() -> Result<Self, CryptoError> {
        let mut csprng = OsRng {};
        let signing_key = SigningKey::generate(&mut csprng);
        Ok(Self { signing_key })
    }

    /// Create a keypair from existing secret key bytes
    pub fn from_secret_key(secret_key_bytes: &[u8]) -> Result<Self, CryptoError> {
        let sk_array: &[u8; 32] = secret_key_bytes.try_into().map_err(|_|
            CryptoError::KeyFormatGeneric("Invalid secret key length, must be 32 bytes".to_string())
        )?;
        let signing_key = SigningKey::from_bytes(sk_array);
        Ok(Self { signing_key })
    }

    /// Get the secret key bytes
    pub fn secret_key_bytes(&self) -> Vec<u8> {
        self.signing_key.to_bytes().to_vec()
    }

    /// Get the verifying key (public key)
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Get a reference to the internal signing key
    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    /// Verify a signature against a message using this keypair's public key
    pub fn verify(&self, message: &[u8], signature_bytes: &[u8]) -> Result<bool, CryptoError> {
        let sig_array: &[u8; 64] = signature_bytes.try_into().map_err(|_|
            CryptoError::KeyFormatGeneric("Invalid signature length for verification, must be 64 bytes".to_string())
        )?;
        let dalek_sig = DalekSignature::from_bytes(sig_array);

        self.signing_key
            .verifying_key()
            .verify(message, &dalek_sig)
            .map(|_| true)
            .map_err(|e| CryptoError::Verification { source: e })
    }
}

impl Signer for Keypair {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        // self.signing_key is ed25519_dalek::SigningKey which implements signature::Signer
        let signature: DalekSignature = self.signing_key.sign(message);
        Ok(signature.to_bytes().to_vec())
    }

    fn public_key(&self) -> Vec<u8> {
        self.signing_key.verifying_key().as_bytes().to_vec()
    }
}

/// DID key format utilities
pub mod did {
    use super::*; // Imports Keypair, CryptoError, VerifyingKey etc.
    use base64::{engine::general_purpose, Engine};

    /// Creates a did:key identifier from a public key
    pub fn key_to_did(public_key_bytes: &[u8]) -> String {
        // Changed arg name for clarity
        let multicodec_prefix = [0xed, 0x01]; // ed25519-pub multicodec
        let mut prefixed = Vec::with_capacity(2 + public_key_bytes.len());
        prefixed.extend_from_slice(&multicodec_prefix);
        prefixed.extend_from_slice(public_key_bytes);

        let encoded = general_purpose::URL_SAFE_NO_PAD.encode(&prefixed);
        format!("did:key:z{}", encoded)
    }

    /// Extracts a public key from a did:key identifier
    pub fn did_to_key(did_string: &str) -> Result<Vec<u8>, CryptoError> {
        // Changed arg name
        if !did_string.starts_with("did:key:z") {
            return Err(CryptoError::KeyFormatGeneric(
                "Invalid DID key format: must start with did:key:z".to_string(),
            ));
        }

        let encoded = &did_string[10..]; // Skip "did:key:z"
        let decoded = general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(CryptoError::KeyFormatBase64)?;

        if decoded.len() < 3 || decoded[0] != 0xed || decoded[1] != 0x01 {
            return Err(CryptoError::KeyFormatGeneric(
                "Invalid multicodec prefix after decoding DID key".to_string(),
            ));
        }

        Ok(decoded[2..].to_vec())
    }
}

// This Signature struct seems to be a custom one for ICN, not ed25519_dalek::Signature.
// It was previously clashing. Now DalekSignature is the alias for ed25519_dalek's one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Signature {
    pub algorithm: String, // e.g., "Ed25519"
    pub value: Vec<u8>,
}
