use crate::{Did, TrustBundle, TrustBundleError};
use ed25519_dalek::VerifyingKey;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Errors related to trust validation.
#[derive(Debug, Error)]
pub enum TrustValidationError {
    #[error("trust bundle verification failed: {0}")]
    BundleError(#[from] TrustBundleError),
    
    #[error("no trust bundle configured")]
    NoBundleConfigured,
    
    #[error("trust bundle access error")]
    BundleAccessError,
}

/// A service that validates trust bundles and maintains the current
/// federation's trusted signers.
#[derive(Debug, Clone)]
pub struct TrustValidator {
    // The current trust bundle, if one is set
    trust_bundle: Arc<RwLock<Option<TrustBundle>>>,
    
    // Known signer public keys
    trusted_keys: Arc<RwLock<HashMap<Did, VerifyingKey>>>,
}

impl TrustValidator {
    /// Creates a new TrustValidator with no configured bundle.
    pub fn new() -> Self {
        Self {
            trust_bundle: Arc::new(RwLock::new(None)),
            trusted_keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Registers a trusted signer DID and verifying key.
    pub fn register_signer(&self, did: Did, key: VerifyingKey) {
        let mut keys = self.trusted_keys.write().unwrap();
        keys.insert(did, key);
    }
    
    /// Sets the active trust bundle and validates it against known signer keys.
    pub fn set_trust_bundle(&self, bundle: TrustBundle) -> Result<(), TrustValidationError> {
        // First verify the bundle
        let keys = self.trusted_keys.read().map_err(|_| TrustValidationError::BundleAccessError)?;
        bundle.verify(&keys)?;
        
        // If verification succeeds, set the bundle
        let mut current = self.trust_bundle.write().map_err(|_| TrustValidationError::BundleAccessError)?;
        *current = Some(bundle);
        
        Ok(())
    }
    
    /// Gets a reference to the current trust bundle, if one exists.
    pub fn get_trust_bundle(&self) -> Result<Option<TrustBundle>, TrustValidationError> {
        let current = self.trust_bundle.read().map_err(|_| TrustValidationError::BundleAccessError)?;
        Ok(current.clone())
    }
    
    /// Validates if the given signer is authorized in the current trust bundle.
    pub fn is_authorized_signer(&self, did: &Did) -> Result<bool, TrustValidationError> {
        let _bundle = self.get_trust_bundle()?
            .ok_or(TrustValidationError::NoBundleConfigured)?;
            
        // Since we no longer track authorized signers in the bundle,
        // we check if the DID is registered as a trusted signer
        let keys = self.trusted_keys.read().map_err(|_| TrustValidationError::BundleAccessError)?;
        Ok(keys.contains_key(did))
    }
} 