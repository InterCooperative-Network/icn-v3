use ed25519_dalek::{Keypair, PublicKey};
use icn_crypto::{sign_detached_jws, verify_detached_jws};
use icn_types::identity::{CredentialProof, VerifiableCredential};
use serde_json::{json, to_value, Value};
use thiserror::Error;

/// Error types for Verifiable Credential operations
#[derive(Error, Debug)]
pub enum VcError {
    #[error("Failed to serialize credential: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Failed to sign credential: {0}")]
    Signing(#[from] icn_crypto::jws::JwsError),

    #[error("Invalid credential structure")]
    InvalidStructure,

    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Result type for VC operations
pub type Result<T> = std::result::Result<T, VcError>;

impl VerifiableCredential {
    /// Canonicalize the credential into a deterministic byte representation
    /// 
    /// This produces a normalized JSON representation that can be used for
    /// signing and verification across different implementations.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        // Create a JSON representation with the proof removed
        // This is because the proof contains the signature which shouldn't be included
        // in what gets signed
        let mut vc_value = to_value(self)?;
        
        if let Value::Object(ref mut map) = vc_value {
            // Remove the proof field for canonicalization
            map.remove("proof");
            
            // Sort all fields deterministically
            let sorted_map = sort_json_object(map);
            
            // Serialize to a compact representation with sorted keys
            let canonical = serde_json::to_vec(&sorted_map)?;
            return Ok(canonical);
        }
        
        Err(VcError::InvalidStructure)
    }
    
    /// Sign the credential with the given keypair
    ///
    /// Returns a detached JWS signature that can be used to verify the credential
    pub fn sign(&self, keypair: &Keypair) -> Result<String> {
        let canonical = self.canonical_bytes()?;
        let jws = sign_detached_jws(&canonical, keypair)?;
        Ok(jws)
    }
    
    /// Verify the credential's signature against a public key
    pub fn verify(&self, public_key: &PublicKey) -> Result<()> {
        let canonical = self.canonical_bytes()?;
        
        // Extract the JWS from the proof
        let jws = &self.proof.jws;
        
        // Verify the signature
        verify_detached_jws(&canonical, jws, public_key)
            .map_err(|e| VcError::Signing(e))?;
            
        Ok(())
    }
    
    /// Create a signed credential from an unsigned one
    pub fn with_signature(mut self, keypair: &Keypair, verification_method: &str) -> Result<Self> {
        // Generate the signature
        let jws = self.sign(keypair)?;
        
        // Create the proof
        let proof = CredentialProof {
            type_: "Ed25519Signature2020".to_string(),
            created: chrono::Utc::now().to_rfc3339(),
            verification_method: verification_method.to_string(),
            proof_purpose: "assertionMethod".to_string(),
            jws,
        };
        
        // Add the proof to the credential
        self.proof = proof;
        
        Ok(self)
    }
}

/// Recursively sort a JSON object to ensure deterministic serialization
fn sort_json_object(obj: &serde_json::Map<String, Value>) -> Value {
    // Create a new sorted map
    let mut sorted = serde_json::Map::new();
    
    // Get sorted keys
    let mut keys: Vec<&String> = obj.keys().collect();
    keys.sort();
    
    // Add each value in sorted key order
    for key in keys {
        let value = &obj[key];
        
        // Recursively sort any nested objects
        let sorted_value = match value {
            Value::Object(ref map) => sort_json_object(map),
            Value::Array(ref arr) => sort_json_array(arr),
            _ => value.clone(),
        };
        
        sorted.insert(key.clone(), sorted_value);
    }
    
    Value::Object(sorted)
}

/// Recursively sort a JSON array to ensure deterministic serialization
fn sort_json_array(arr: &[Value]) -> Value {
    let mut result = Vec::with_capacity(arr.len());
    
    for value in arr {
        let sorted_value = match value {
            Value::Object(ref map) => sort_json_object(map),
            Value::Array(ref nested_arr) => sort_json_array(nested_arr),
            _ => value.clone(),
        };
        
        result.push(sorted_value);
    }
    
    Value::Array(result)
} 