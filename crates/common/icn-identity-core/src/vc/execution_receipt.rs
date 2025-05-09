use ed25519_dalek::{Keypair, PublicKey, Signature, Signer, Verifier};
use icn_crypto::{sign_detached_jws, verify_detached_jws};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use crate::did::{Did, DidError};
use crate::vc::{VcError, Result};

/// Error types specific to ExecutionReceipt operations
#[derive(Error, Debug)]
pub enum ExecutionReceiptError {
    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("Invalid scope: {0}")]
    InvalidScope(String),

    #[error("DID error: {0}")]
    DidError(#[from] DidError),

    #[error("VC error: {0}")]
    VcError(#[from] VcError),
}

/// Scope of the execution receipt
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    /// Organization or group this execution applies to
    pub organization: String,
    
    /// Optional department or team
    pub department: Option<String>,
    
    /// Optional specific project
    pub project: Option<String>,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.organization)?;
        if let Some(dept) = &self.department {
            write!(f, "/{}", dept)?;
        }
        if let Some(proj) = &self.project {
            write!(f, "/{}", proj)?;
        }
        Ok(())
    }
}

impl TryFrom<&str> for Scope {
    type Error = ExecutionReceiptError;

    fn try_from(s: &str) -> std::result::Result<Self, Self::Error> {
        let parts: Vec<&str> = s.split('/').collect();
        
        if parts.is_empty() || parts[0].is_empty() {
            return Err(ExecutionReceiptError::InvalidScope(
                "Scope must have at least an organization".to_string()
            ));
        }
        
        Ok(Scope {
            organization: parts[0].to_string(),
            department: parts.get(1).filter(|&p| !p.is_empty()).map(|s| s.to_string()),
            project: parts.get(2).filter(|&p| !p.is_empty()).map(|s| s.to_string()),
        })
    }
}

/// Execution receipt for tracking WASM executions in the ICN network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// DID of the entity that executed the code
    pub executed_by: String,
    
    /// Content identifier (CID) of the executed code
    pub executed_cid: String,
    
    /// Scope of the execution
    pub scope: Scope,
    
    /// Amount of fuel used during execution
    pub fuel_used: u64,
    
    /// Epoch timestamp when execution occurred
    pub epoch: String,
    
    /// JSON Web Signature
    pub signature: String,
}

impl ExecutionReceipt {
    /// Create a new unsigned ExecutionReceipt
    pub fn new(
        executed_by: impl Into<String>,
        executed_cid: impl Into<String>,
        scope: Scope,
        fuel_used: u64,
        epoch: impl Into<String>,
    ) -> Self {
        Self {
            executed_by: executed_by.into(),
            executed_cid: executed_cid.into(),
            scope,
            fuel_used,
            epoch: epoch.into(),
            signature: String::new(),
        }
    }
    
    /// Convert the receipt to a canonical form for signing
    fn to_canonical_bytes(&self) -> Result<Vec<u8>> {
        // Create a copy without the signature
        let unsigned = Self {
            executed_by: self.executed_by.clone(),
            executed_cid: self.executed_cid.clone(),
            scope: self.scope.clone(),
            fuel_used: self.fuel_used,
            epoch: self.epoch.clone(),
            signature: String::new(), // Empty signature
        };
        
        // Serialize to JSON in a canonical form
        let json = serde_json::to_vec(&unsigned)
            .map_err(|e| VcError::Serialization(e))?;
        
        Ok(json)
    }
    
    /// Sign the receipt with the given keypair
    pub fn sign(mut self, keypair: &Keypair) -> Result<Self> {
        let canonical = self.to_canonical_bytes()?;
        
        // Sign the canonical form
        let jws = sign_detached_jws(&canonical, keypair)?;
        
        // Update the signature
        self.signature = jws;
        
        Ok(self)
    }
    
    /// Verify the receipt's signature against a public key
    pub fn verify(&self, public_key: &PublicKey) -> Result<()> {
        let canonical = self.to_canonical_bytes()?;
        
        // Verify the signature
        verify_detached_jws(&canonical, &self.signature, public_key)
            .map_err(|e| VcError::Signing(e))?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Keypair;
    use crate::did::pk_from_did_key;
    use rand::rngs::OsRng;
    
    #[test]
    fn test_scope_parsing() {
        // Test full scope
        let scope = Scope::try_from("org/dept/project").unwrap();
        assert_eq!(scope.organization, "org");
        assert_eq!(scope.department, Some("dept".to_string()));
        assert_eq!(scope.project, Some("project".to_string()));
        
        // Test org + dept
        let scope = Scope::try_from("org/dept").unwrap();
        assert_eq!(scope.organization, "org");
        assert_eq!(scope.department, Some("dept".to_string()));
        assert_eq!(scope.project, None);
        
        // Test org only
        let scope = Scope::try_from("org").unwrap();
        assert_eq!(scope.organization, "org");
        assert_eq!(scope.department, None);
        assert_eq!(scope.project, None);
        
        // Test empty
        let result = Scope::try_from("");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_receipt_signing_and_verification() {
        // Generate a keypair
        let mut csprng = OsRng;
        let keypair = Keypair::generate(&mut csprng);
        
        // Create a scope
        let scope = Scope {
            organization: "test-org".to_string(),
            department: Some("test-dept".to_string()),
            project: None,
        };
        
        // Create an unsigned receipt
        let receipt = ExecutionReceipt::new(
            "did:key:test",
            "QmTestCid",
            scope,
            1000,
            "2023-01-01T00:00:00Z",
        );
        
        // Sign the receipt
        let signed_receipt = receipt.sign(&keypair).unwrap();
        
        // Verify the signature
        signed_receipt.verify(&keypair.public).unwrap();
        
        // Try with wrong key (should fail)
        let mut csprng = OsRng;
        let wrong_keypair = Keypair::generate(&mut csprng);
        
        let result = signed_receipt.verify(&wrong_keypair.public);
        assert!(result.is_err());
    }
} 