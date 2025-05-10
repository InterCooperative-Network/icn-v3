use std::collections::{HashSet, HashMap};
use std::sync::{Arc, RwLock};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Information about a revoked token
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RevokedToken {
    /// The token identifier (jti claim)
    pub jti: String,
    /// Subject of the token (user DID)
    pub subject: String,
    /// Issuer of the token
    pub issuer: Option<String>,
    /// When the token was revoked
    pub revoked_at: DateTime<Utc>,
    /// Reason for revocation
    pub reason: Option<String>,
    /// Who revoked the token (admin DID)
    pub revoked_by: String,
}

/// Request to revoke a token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeTokenRequest {
    /// Token ID to revoke (jti claim)
    pub jti: Option<String>,
    /// Subject to revoke all tokens for
    pub subject: Option<String>,
    /// Issuer of the token
    pub issuer: Option<String>,
    /// Reason for revocation
    pub reason: Option<String>,
}

/// Response to a token revocation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeTokenResponse {
    /// Whether the token was successfully revoked
    pub revoked: bool,
    /// Time of revocation
    pub revoked_at: DateTime<Utc>,
    /// The token identifier
    pub jti: Option<String>,
    /// The subject whose token was revoked
    pub subject: Option<String>,
    /// The issuer of the revoked token
    pub issuer: Option<String>,
}

/// Token rotation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotateTokenRequest {
    /// Current token jti to revoke
    pub current_jti: String,
    /// Subject (user DID) for the new token
    pub subject: String,
    /// Expiration time in seconds from now
    pub expires_in: Option<u64>,
    /// Federation IDs to grant access to 
    pub federation_ids: Option<Vec<String>>,
    /// Cooperative IDs to grant access to
    pub coop_ids: Option<Vec<String>>,
    /// Community IDs to grant access to
    pub community_ids: Option<Vec<String>>,
    /// Roles to assign by organization ID
    pub roles: Option<std::collections::HashMap<String, Vec<String>>>,
    /// Reason for rotation
    pub reason: Option<String>,
}

/// Interface for token revocation storage
pub trait TokenRevocationStore: Send + Sync {
    /// Revoke a token by its jti
    fn revoke_token(&self, token: RevokedToken) -> bool;
    
    /// Check if a token is revoked by JTI
    fn is_revoked(&self, jti: &str) -> bool;
    
    /// Check if a token is revoked by subject and issuer
    fn is_revoked_by_subject_issuer(&self, subject: &str, issuer: Option<&str>) -> bool;
    
    /// Get all revoked tokens for a subject
    fn get_revoked_tokens_for_subject(&self, subject: &str) -> Vec<RevokedToken>;
    
    /// Get all revoked tokens for a subject and issuer
    fn get_revoked_tokens_for_subject_issuer(&self, subject: &str, issuer: Option<&str>) -> Vec<RevokedToken>;
    
    /// Clear expired revocations to free up memory/storage
    fn clear_expired_revocations(&self, older_than: DateTime<Utc>) -> usize;
}

/// In-memory implementation of the token revocation store
#[derive(Debug, Clone, Default)]
pub struct InMemoryRevocationStore {
    /// Set of revoked tokens
    revoked_tokens: Arc<RwLock<HashSet<RevokedToken>>>,
    /// Index of revoked tokens by JTI for fast lookup
    jti_index: Arc<RwLock<HashMap<String, RevokedToken>>>,
    /// Index of revoked tokens by subject+issuer for fast lookup
    subject_issuer_index: Arc<RwLock<HashMap<(String, Option<String>), Vec<RevokedToken>>>>,
}

impl InMemoryRevocationStore {
    /// Create a new in-memory revocation store
    pub fn new() -> Self {
        Self {
            revoked_tokens: Arc::new(RwLock::new(HashSet::new())),
            jti_index: Arc::new(RwLock::new(HashMap::new())),
            subject_issuer_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Add a token to all indices
    fn add_to_indices(&self, token: &RevokedToken) {
        // Add to JTI index
        let mut jti_idx = self.jti_index.write().unwrap();
        jti_idx.insert(token.jti.clone(), token.clone());
        
        // Add to subject+issuer index
        let mut subj_idx = self.subject_issuer_index.write().unwrap();
        let key = (token.subject.clone(), token.issuer.clone());
        
        subj_idx
            .entry(key)
            .or_insert_with(Vec::new)
            .push(token.clone());
    }
    
    /// Remove a token from all indices
    fn remove_from_indices(&self, token: &RevokedToken) {
        // Remove from JTI index
        let mut jti_idx = self.jti_index.write().unwrap();
        jti_idx.remove(&token.jti);
        
        // Remove from subject+issuer index
        let mut subj_idx = self.subject_issuer_index.write().unwrap();
        let key = (token.subject.clone(), token.issuer.clone());
        
        if let Some(tokens) = subj_idx.get_mut(&key) {
            tokens.retain(|t| t.jti != token.jti);
            if tokens.is_empty() {
                subj_idx.remove(&key);
            }
        }
    }
}

impl TokenRevocationStore for InMemoryRevocationStore {
    fn revoke_token(&self, token: RevokedToken) -> bool {
        let mut store = self.revoked_tokens.write().unwrap();
        let inserted = store.insert(token.clone());
        
        if inserted {
            // Only update indices if it was actually inserted
            self.add_to_indices(&token);
        }
        
        inserted
    }
    
    fn is_revoked(&self, jti: &str) -> bool {
        // Fast lookup using the JTI index
        let jti_idx = self.jti_index.read().unwrap();
        jti_idx.contains_key(jti)
    }
    
    fn is_revoked_by_subject_issuer(&self, subject: &str, issuer: Option<&str>) -> bool {
        let subj_idx = self.subject_issuer_index.read().unwrap();
        let issuer_str = issuer.map(|s| s.to_string());
        
        subj_idx.contains_key(&(subject.to_string(), issuer_str))
    }
    
    fn get_revoked_tokens_for_subject(&self, subject: &str) -> Vec<RevokedToken> {
        let subj_idx = self.subject_issuer_index.read().unwrap();
        
        subj_idx
            .iter()
            .filter(|((s, _), _)| s == subject)
            .flat_map(|(_, tokens)| tokens.clone())
            .collect()
    }
    
    fn get_revoked_tokens_for_subject_issuer(&self, subject: &str, issuer: Option<&str>) -> Vec<RevokedToken> {
        let subj_idx = self.subject_issuer_index.read().unwrap();
        let issuer_str = issuer.map(|s| s.to_string());
        
        match subj_idx.get(&(subject.to_string(), issuer_str)) {
            Some(tokens) => tokens.clone(),
            None => Vec::new(),
        }
    }
    
    fn clear_expired_revocations(&self, older_than: DateTime<Utc>) -> usize {
        let mut store = self.revoked_tokens.write().unwrap();
        let before_count = store.len();
        
        // First, find tokens to remove
        let to_remove: Vec<RevokedToken> = store
            .iter()
            .filter(|token| token.revoked_at < older_than)
            .cloned()
            .collect();
        
        // Remove them from the main set
        for token in &to_remove {
            store.remove(token);
            self.remove_from_indices(token);
        }
        
        before_count - store.len()
    }
}

/// Extension to perform revocation checks in JWT validation
pub fn check_token_not_revoked(
    revocation_store: &dyn TokenRevocationStore,
    jti: &Option<String>,
) -> Result<(), super::AuthError> {
    if let Some(token_id) = jti {
        if revocation_store.is_revoked(token_id) {
            return Err(super::AuthError::TokenRevoked);
        }
    } else {
        // If token has no JTI, can't verify against revocation list
        // For production, you might want to reject tokens without JTI
        // return Err(super::AuthError::InvalidTokenFormat);
    }
    
    Ok(())
}

/// Extension to perform revocation checks by subject and issuer
pub fn check_subject_not_revoked(
    revocation_store: &dyn TokenRevocationStore,
    subject: &str,
    issuer: Option<&str>,
) -> Result<(), super::AuthError> {
    if revocation_store.is_revoked_by_subject_issuer(subject, issuer) {
        return Err(super::AuthError::TokenRevoked);
    }
    
    Ok(())
} 