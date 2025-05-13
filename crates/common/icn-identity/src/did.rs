use multibase::{Base, decode};
use thiserror::Error;
use std::str::FromStr;
use std::fmt;
use serde::{Deserialize, Serialize};
use anyhow::Context;

// Ed25519 public key multicodec prefix
const ED25519_MULTICODEC_PREFIX: u8 = 0xed;

/// Error type for DID operations.
#[derive(Debug, Error)]
pub enum DidError {
    #[error("malformed DID string")]
    Malformed,
    #[error("unsupported multicodec: {0:#x}")]
    UnsupportedCodec(u64),
}

/// A W3C-compatible Decentralized Identifier.
///
/// Currently supports **`did:key:z…`** for Ed25519 public keys.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Did(String);

impl Did {
    /// Construct a DID from an Ed25519 public key.
    pub fn new_ed25519(pk: &ed25519_dalek::VerifyingKey) -> Self {
        // 0xED: Ed25519 public key multicodec prefix
        let mut bytes = vec![ED25519_MULTICODEC_PREFIX];
        bytes.extend_from_slice(pk.as_bytes());

        let encoded = multibase::encode(Base::Base58Btc, bytes);
        Self(format!("did:key:{}", encoded))
    }

    /// Return the DID string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Decode and return the embedded Ed25519 public key.
    pub fn to_ed25519(&self) -> Result<ed25519_dalek::VerifyingKey, DidError> {
        let parts: Vec<&str> = self.0.split(':').collect();
        if parts.len() != 3 || parts[0] != "did" || parts[1] != "key" {
            return Err(DidError::Malformed);
        }
        let (_, data) = decode(parts[2]).map_err(|_| DidError::Malformed)?;

        // First byte is the multicodec code.
        if let Some((codec, key_bytes)) = data.split_first() {
            if *codec == ED25519_MULTICODEC_PREFIX {
                if key_bytes.len() != 32 {
                    return Err(DidError::Malformed);
                }
                
                // Convert to array for ed25519_dalek::VerifyingKey
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(key_bytes);
                
                let pk = ed25519_dalek::VerifyingKey::from_bytes(&bytes)
                    .map_err(|_| DidError::Malformed)?;
                Ok(pk)
            } else {
                Err(DidError::UnsupportedCodec(*codec as u64))
            }
        } else {
            Err(DidError::Malformed)
        }
    }

    /// Returns the Ed25519 verifying key embedded in the DID.
    /// Only supports `did:key` using Ed25519 multicodec (0xED).
    /// This maps the internal `to_ed25519` method and converts the error type.
    pub fn verifying_key(&self) -> anyhow::Result<ed25519_dalek::VerifyingKey> {
        self.to_ed25519()
            .context("Failed to extract Ed25519 verifying key from DID")
    }
}

impl FromStr for Did {
    type Err = DidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Basic validation
        if !s.starts_with("did:key:") {
            return Err(DidError::Malformed);
        }
        
        // Attempt to decode the public key to validate it's a proper DID
        let did = Did(s.to_string());
        did.to_ed25519()?;
        
        Ok(did)
    }
}

impl fmt::Display for Did {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
} 