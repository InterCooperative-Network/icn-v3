use crate::Did;
use rand::rngs::OsRng;
use ed25519_dalek::{Signer, Verifier};
use serde::{Serialize, Deserialize};

pub type Signature = ed25519_dalek::Signature;

/// Ed25519 keypair bound to a DID.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyPair {
    pub did: Did,
    pub pk: ed25519_dalek::VerifyingKey,
    sk: ed25519_dalek::SigningKey,
}

impl KeyPair {
    /// Generate a new random keypair.
    pub fn generate() -> Self {
        let sk = ed25519_dalek::SigningKey::generate(&mut OsRng);
        let pk = sk.verifying_key();
        let did = Did::new_ed25519(&pk);
        Self { did, pk, sk }
    }

    /// Sign arbitrary bytes, returning an Ed25519 signature.
    pub fn sign(&self, msg: &[u8]) -> Signature {
        self.sk.sign(msg)
    }

    /// Verify a signature against `msg`.
    pub fn verify(&self, msg: &[u8], sig: &Signature) -> bool {
        self.pk.verify(msg, sig).is_ok()
    }
    
    /// Return the bytes of the signing key
    /// This is used for serialization purposes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.sk.to_bytes()
    }
} 