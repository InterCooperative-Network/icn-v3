use anyhow::Context;
use ed25519_dalek::SignatureError as Ed25519SignatureError;
use multibase::{decode, Base, Error as MultibaseError};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

// Ed25519 public key multicodec prefix
const ED25519_MULTICODEC_PREFIX: u8 = 0xed;
const ED25519_KEY_LENGTH: usize = 32;

/// Error type for DID operations.
#[derive(Debug, Error)]
pub enum DidError {
    #[error("DID string is empty")]
    EmptyInput,

    #[error("DID string must start with 'did:' prefix, found '{0}'")]
    InvalidPrefix(String),

    #[error("Unsupported DID method: expected 'key', found '{0}'")]
    UnsupportedMethod(String),

    #[error("DID string is missing the method-specific identifier (e.g., 'did:key:<identifier>')")]
    MissingMethodSpecificId,

    #[error("Method-specific identifier '{identifier_part}' is not valid multibase: {source}")]
    InvalidMethodSpecificIdEncoding {
        identifier_part: String,
        #[source] source: MultibaseError
    },

    #[error("Decoded method-specific identifier is empty (no multicodec prefix)")]
    EmptyDecodedMethodSpecificId,

    #[error("Unsupported multicodec for 'did:key': expected 0x{expected_codec:x} (Ed25519), found 0x{found_codec:x}")]
    UnsupportedKeyMulticodec {
        expected_codec: u64,
        found_codec: u64,
    },

    #[error("Invalid Ed25519 key length in 'did:key': expected {expected_len} bytes, found {found_len}")]
    InvalidKeyLength {
        expected_len: usize,
        found_len: usize,
    },

    #[error("Invalid Ed25519 key bytes: {0}")]
    InvalidKeyBytes(#[from] Ed25519SignatureError),
}

/// A W3C-compatible Decentralized Identifier.
///
/// Currently supports **`did:key:z…`** for Ed25519 public keys.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Did(String);

impl Did {
    /// Construct a DID from an Ed25519 public key.
    pub fn new_ed25519(pk: &ed25519_dalek::VerifyingKey) -> Self {
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
        if self.0.is_empty() {
            return Err(DidError::EmptyInput);
        }

        let parts: Vec<&str> = self.0.split(':').collect();

        if parts.len() < 1 || parts[0] != "did" {
            return Err(DidError::InvalidPrefix(self.0.split_once(':').map_or_else(|| self.0.to_string(), |(p, _)| p.to_string())));
        }
        if parts.len() < 2 || parts[1] != "key" {
             // parts[0] is "did". If parts[1] exists, capture it, else it's an unknown method scenario for "did:"
            return Err(DidError::UnsupportedMethod(parts.get(1).unwrap_or(&"").to_string()));
        }
        if parts.len() < 3 || parts[2].is_empty() {
            return Err(DidError::MissingMethodSpecificId);
        }
        // Consider if parts.len() > 3 implies an error. For did:key, the identifier parts[2]
        // can itself contain characters that are not colons, but if it did, multibase decode would likely fail.
        // If parts.len() > 3, it means the original string was e.g. "did:key:abc:extra", parts[2] would be "abc".
        // This seems like an invalid DID structure, but multibase processing of parts[2] might be the first to catch it
        // if parts[2] is not a valid multibase encoding, or it might decode parts[2] successfully.
        // For now, let's assume parts.len() > 3 is not an explicit structural error here IF parts[2] itself is a single valid segment.
        // The current code implicitly takes parts[2] as the method specific ID.

        let identifier_part = parts[2];

        match decode(identifier_part) {
            Ok((_, data)) => {
                if let Some((codec, key_bytes)) = data.split_first() {
                    if *codec == ED25519_MULTICODEC_PREFIX {
                        if key_bytes.len() != ED25519_KEY_LENGTH {
                            return Err(DidError::InvalidKeyLength {
                                expected_len: ED25519_KEY_LENGTH,
                                found_len: key_bytes.len(),
                            });
                        }
                        let mut pk_bytes = [0u8; ED25519_KEY_LENGTH];
                        pk_bytes.copy_from_slice(key_bytes);
                        ed25519_dalek::VerifyingKey::from_bytes(&pk_bytes).map_err(DidError::from)
                    } else {
                        Err(DidError::UnsupportedKeyMulticodec {
                            expected_codec: ED25519_MULTICODEC_PREFIX as u64,
                            found_codec: *codec as u64,
                        })
                    }
                } else {
                    Err(DidError::EmptyDecodedMethodSpecificId)
                }
            }
            Err(e) => Err(DidError::InvalidMethodSpecificIdEncoding {
                identifier_part: identifier_part.to_string(),
                source: e,
            }),
        }
    }

    /// Returns the Ed25519 verifying key embedded in the DID.
    /// Only supports `did:key` using Ed25519 multicodec (0xED).
    /// This maps the internal `to_ed25519` method and converts the error type.
    pub fn verifying_key(&self) -> anyhow::Result<ed25519_dalek::VerifyingKey> {
        self.to_ed25519()
            .with_context(|| format!("Failed to extract Ed25519 verifying key from DID '{}'", self.0))
    }
}

impl FromStr for Did {
    type Err = DidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(DidError::EmptyInput);
        }
        // The more detailed parsing is now in to_ed25519.
        // We construct the Did first, then validate by calling to_ed25519.
        // This ensures the string s is stored in Did before validation attempts.
        let did = Did(s.to_string());
        did.to_ed25519()?; // This will propagate the new specific errors
        Ok(did)
    }
}

impl fmt::Display for Did {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
