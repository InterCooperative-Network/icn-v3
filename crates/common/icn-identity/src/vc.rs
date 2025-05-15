use crate::{Did, KeyPair, Signature};
use chrono::{DateTime, Utc};
use ed25519_dalek::{SignatureError as Ed25519SignatureError, Verifier};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Generic W3C-style Verifiable Credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiableCredential<T>
where
    T: Serialize + for<'a> Deserialize<'a> + Clone,
{
    #[serde(rename = "@context")]
    pub context: Vec<String>,

    #[serde(rename = "type")]
    pub types: Vec<String>,

    pub issuer: Did,
    #[serde(rename = "issuanceDate")]
    pub issuance_date: DateTime<Utc>,

    #[serde(bound = "")] // generic bounds handled above
    pub credential_subject: T,

    /// Optional proof until signed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<Proof>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    #[serde(rename = "type")]
    pub proof_type: String, // e.g., "Ed25519Signature2020"
    #[serde(rename = "created")]
    pub created: DateTime<Utc>,
    #[serde(rename = "verificationMethod")]
    pub verification_method: String,
    #[serde(rename = "proofPurpose")]
    pub proof_purpose: String,
    pub signature_value_hex: String, // hex-encoded raw signature bytes
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("credential already signed")]
    AlreadySigned,
    #[error("cryptographic signature verification failed: {0}")]
    CryptoVerification(#[from] Ed25519SignatureError),
    #[error("serialization error: {0}")]
    Ser(#[from] serde_json::Error),
}

/// Convenience wrapper holding the raw signature while keeping original VC.
#[derive(Debug, Clone)]
pub struct SignedCredential<T>
where
    T: Serialize + for<'a> Deserialize<'a> + Clone,
{
    pub vc: VerifiableCredential<T>,
    pub signature: Signature,
}

impl<T> VerifiableCredential<T>
where
    T: Serialize + for<'a> Deserialize<'a> + Clone,
{
    /// Return canonical JSON bytes (stable field order).
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CredentialError> {
        // Create a copy without the proof to ensure deterministic pre-image.
        let mut tmp = self.clone();
        tmp.proof = None;

        let value: Value = serde_json::to_value(&tmp)?;
        // **Deterministic ordering** – map entries are already ordered by serde_json
        // for structs; nested maps in `credential_subject` should also be stable
        // if they are `Map<String, Value>`.
        Ok(serde_json::to_vec(&value)?)
    }

    /// Sign with the supplied keypair, producing a `SignedCredential`.
    pub fn sign(mut self, kp: &KeyPair) -> Result<SignedCredential<T>, CredentialError> {
        if self.proof.is_some() {
            return Err(CredentialError::AlreadySigned);
        }

        let bytes = self.canonical_bytes()?;
        let sig = kp.sign(&bytes);

        // Attach minimal proof metadata (detached JWS style).
        self.proof = Some(Proof {
            proof_type: "Ed25519Signature2020".into(),
            created: chrono::Utc::now(),
            verification_method: kp.did.as_str().into(),
            proof_purpose: "assertionMethod".into(),
            signature_value_hex: hex::encode(sig.to_bytes()),
        });

        Ok(SignedCredential {
            vc: self,
            signature: sig,
        })
    }
}

impl<T> SignedCredential<T>
where
    T: Serialize + for<'a> Deserialize<'a> + Clone,
{
    pub fn verify(&self, pk: &ed25519_dalek::VerifyingKey) -> Result<(), CredentialError> {
        let bytes = self.vc.canonical_bytes()?;
        pk.verify(&bytes, &self.signature)?;
        Ok(())
    }
}
