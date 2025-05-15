use crate::{Did, Signature};
use ed25519_dalek::{Verifier, VerifyingKey};
use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::fmt;
use thiserror::Error;

/// Defines the type of quorum needed for validating a set of signatures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuorumType {
    /// Simple majority of allowed signers (>50%)
    Majority,

    /// Specific threshold of signers needed (e.g., 3 of 5)
    Threshold(u8),

    /// Weighted voting, where each signer has a specific voting power
    Weighted(HashMap<Did, u16>),
}

/// Errors that can occur during quorum verification.
#[derive(Debug, Error)]
pub enum QuorumError {
    #[error("insufficient signers to meet quorum requirements")]
    InsufficientSigners,

    #[error("duplicate signer detected")]
    DuplicateSigner,

    #[error("unauthorized signer: {0}")]
    UnauthorizedSigner(Did),

    #[error("minimum threshold ({threshold}) exceeds the number of available authorized signers ({available_signers})")]
    ThresholdTooHigh {
        threshold: u8,
        available_signers: usize,
    },

    #[error("signer {0} is required by weighted quorum but not found in the provided weight map")]
    SignerNotInWeightMap(Did),
}

/// A collection of signatures that proves a quorum of authorized entities
/// have approved some content.
#[derive(Debug, Clone)]
pub struct QuorumProof {
    /// The type of quorum required for verification
    pub quorum_type: QuorumType,

    /// Collection of signatures from different entities
    pub signatures: Vec<(Did, Signature)>,
}

impl Serialize for QuorumProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("QuorumProof", 2)?;
        state.serialize_field("quorum_type", &self.quorum_type)?;

        // Convert signatures to a serializable format
        let serialized_signatures: Vec<(Did, String)> = self
            .signatures
            .iter()
            .map(|(did, sig)| (did.clone(), hex::encode(sig.to_bytes())))
            .collect();

        state.serialize_field("signatures", &serialized_signatures)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for QuorumProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Field {
            QuorumType,
            Signatures,
        }

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(deserializer: D) -> Result<Field, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct FieldVisitor;

                impl Visitor<'_> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("`quorum_type` or `signatures`")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "quorum_type" => Ok(Field::QuorumType),
                            "signatures" => Ok(Field::Signatures),
                            _ => Err(de::Error::unknown_field(
                                value,
                                &["quorum_type", "signatures"],
                            )),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct QuorumProofVisitor;

        impl<'de> Visitor<'de> for QuorumProofVisitor {
            type Value = QuorumProof;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct QuorumProof")
            }

            fn visit_map<V>(self, mut map: V) -> Result<QuorumProof, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut quorum_type = None;
                let mut signatures_str: Option<Vec<(Did, String)>> = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::QuorumType => {
                            if quorum_type.is_some() {
                                return Err(de::Error::duplicate_field("quorum_type"));
                            }
                            quorum_type = Some(map.next_value()?);
                        }
                        Field::Signatures => {
                            if signatures_str.is_some() {
                                return Err(de::Error::duplicate_field("signatures"));
                            }
                            signatures_str = Some(map.next_value()?);
                        }
                    }
                }

                let quorum_type =
                    quorum_type.ok_or_else(|| de::Error::missing_field("quorum_type"))?;
                let signatures_str =
                    signatures_str.ok_or_else(|| de::Error::missing_field("signatures"))?;

                // Convert signature strings back to Signature objects
                let signatures = signatures_str
                    .into_iter()
                    .map(|(did, sig_hex)| {
                        let sig_bytes = hex::decode(&sig_hex).map_err(|e| {
                            de::Error::custom(format!("Invalid signature hex: {}", e))
                        })?;

                        if sig_bytes.len() != 64 {
                            return Err(de::Error::custom("Invalid signature length"));
                        }

                        let mut bytes = [0u8; 64];
                        bytes.copy_from_slice(&sig_bytes);

                        let signature = ed25519_dalek::Signature::from_bytes(&bytes);
                        Ok((did, signature))
                    })
                    .collect::<Result<Vec<_>, V::Error>>()?;

                Ok(QuorumProof {
                    quorum_type,
                    signatures,
                })
            }
        }

        const FIELDS: &[&str] = &["quorum_type", "signatures"];
        deserializer.deserialize_struct("QuorumProof", FIELDS, QuorumProofVisitor)
    }
}

impl QuorumProof {
    /// Creates a new QuorumProof with the specified quorum type and initial signatures.
    pub fn new(quorum_type: QuorumType, signatures: Vec<(Did, Signature)>) -> Self {
        Self {
            quorum_type,
            signatures,
        }
    }

    /// Adds a signature to the proof.
    pub fn add_signature(&mut self, did: Did, signature: Signature) -> Result<(), QuorumError> {
        // Check for duplicate signers
        if self
            .signatures
            .iter()
            .any(|(existing_did, _)| existing_did == &did)
        {
            return Err(QuorumError::DuplicateSigner);
        }

        self.signatures.push((did, signature));
        Ok(())
    }

    /// Verifies the quorum proof against a message hash and a set of allowed signers.
    pub fn verify(
        &self,
        message: &[u8],
        allowed_signers: &HashMap<Did, VerifyingKey>,
    ) -> Result<(), QuorumError> {
        // Check for duplicate signers (safeguard even though add_signature checks too)
        let mut seen_signers = HashSet::new();
        for (did, _) in &self.signatures {
            if !seen_signers.insert(did) {
                return Err(QuorumError::DuplicateSigner);
            }
        }

        // Verify each signature
        let valid_signatures: Vec<&Did> = self
            .signatures
            .iter()
            .filter_map(|(did, sig)| {
                if let Some(pk) = allowed_signers.get(did) {
                    if pk.verify(message, sig).is_ok() {
                        Some(did)
                    } else {
                        None // Invalid signature
                    }
                } else {
                    None // Not an allowed signer
                }
            })
            .collect();

        // Verify the quorum is met
        match &self.quorum_type {
            QuorumType::Majority => {
                if valid_signatures.len() * 2 > allowed_signers.len() {
                    Ok(())
                } else {
                    Err(QuorumError::InsufficientSigners)
                }
            }
            QuorumType::Threshold(min) => {
                if *min as usize > allowed_signers.len() {
                    return Err(QuorumError::ThresholdTooHigh {
                        threshold: *min,
                        available_signers: allowed_signers.len(),
                    });
                }

                if valid_signatures.len() >= *min as usize {
                    Ok(())
                } else {
                    Err(QuorumError::InsufficientSigners)
                }
            }
            QuorumType::Weighted(weights) => {
                // Calculate the total possible weight
                let total_weight: u16 = weights.values().sum();

                // Calculate the weight of valid signatures
                let mut valid_weight: u16 = 0;
                for did in valid_signatures {
                    match weights.get(did) {
                        Some(weight) => valid_weight += weight,
                        None => return Err(QuorumError::SignerNotInWeightMap(did.clone())),
                    }
                }

                // Check if we have a majority of the weighted votes
                if valid_weight * 2 > total_weight {
                    Ok(())
                } else {
                    Err(QuorumError::InsufficientSigners)
                }
            }
        }
    }
}
