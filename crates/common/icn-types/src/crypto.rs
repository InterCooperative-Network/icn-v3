use ed25519_dalek::{Keypair as DalekKeypair, PublicKey, SecretKey, Signature, Signer as DalekSigner, Verifier};
use rand::rngs::OsRng;
use crate::error::CryptoError;

/// A trait for objects that can sign messages
pub trait Signer {
    /// Sign a message, returning the signature
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, CryptoError>;
    
    /// Get the public key associated with this signer
    fn public_key(&self) -> Vec<u8>;
}

/// Ed25519 keypair implementation
pub struct Keypair {
    inner: DalekKeypair,
}

impl Clone for Keypair {
    fn clone(&self) -> Self {
        // Since DalekKeypair doesn't implement Clone, we need to recreate it from the secret key
        let secret_bytes = self.inner.secret.as_bytes();
        Keypair::from_secret_key(secret_bytes).expect("Failed to clone keypair")
    }
}

impl Keypair {
    /// Generate a new random keypair
    pub fn generate() -> Result<Self, CryptoError> {
        let mut csprng = OsRng {};
        let keypair = DalekKeypair::generate(&mut csprng);
        Ok(Self { inner: keypair })
    }

    /// Create a keypair from existing secret key bytes
    pub fn from_secret_key(secret_key: &[u8]) -> Result<Self, CryptoError> {
        if secret_key.len() != 32 {
            return Err(CryptoError::KeyGenError("Invalid secret key length".to_string()));
        }

        let secret = SecretKey::from_bytes(secret_key)
            .map_err(|e| CryptoError::KeyGenError(e.to_string()))?;
        let public = PublicKey::from(&secret);
        
        Ok(Self {
            inner: DalekKeypair { secret, public }
        })
    }
    
    /// Get the secret key bytes
    pub fn secret_key_bytes(&self) -> Vec<u8> {
        self.inner.secret.as_bytes().to_vec()
    }

    /// Verify a signature against a message
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<bool, CryptoError> {
        if signature.len() != 64 {
            return Err(CryptoError::VerificationError("Invalid signature length".to_string()));
        }

        let sig = Signature::from_bytes(signature)
            .map_err(|e| CryptoError::VerificationError(e.to_string()))?;

        self.inner.public.verify(message, &sig)
            .map(|_| true)
            .map_err(|e| CryptoError::VerificationError(e.to_string()))
    }
}

impl Signer for Keypair {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let signature = self.inner.sign(message);
        Ok(signature.to_bytes().to_vec())
    }
    
    fn public_key(&self) -> Vec<u8> {
        self.inner.public.as_bytes().to_vec()
    }
}

/// DID key format utilities
pub mod did {
    use super::*;
    use base64::{engine::general_purpose, Engine};

    /// Creates a did:key identifier from a public key
    pub fn key_to_did(public_key: &[u8]) -> String {
        let multicodec_prefix = [0xed, 0x01]; // ed25519-pub multicodec
        let mut prefixed = Vec::with_capacity(2 + public_key.len());
        prefixed.extend_from_slice(&multicodec_prefix);
        prefixed.extend_from_slice(public_key);
        
        let encoded = general_purpose::URL_SAFE_NO_PAD.encode(&prefixed);
        format!("did:key:z{}", encoded)
    }

    /// Extracts a public key from a did:key identifier
    pub fn did_to_key(did: &str) -> Result<Vec<u8>, CryptoError> {
        if !did.starts_with("did:key:z") {
            return Err(CryptoError::KeyGenError("Invalid DID key format".to_string()));
        }

        let encoded = &did[10..]; // Skip "did:key:z"
        let decoded = general_purpose::URL_SAFE_NO_PAD.decode(encoded)
            .map_err(|e| CryptoError::KeyGenError(format!("Invalid base64: {}", e)))?;
        
        if decoded.len() < 3 || decoded[0] != 0xed || decoded[1] != 0x01 {
            return Err(CryptoError::KeyGenError("Invalid multicodec prefix".to_string()));
        }

        Ok(decoded[2..].to_vec())
    }
} 