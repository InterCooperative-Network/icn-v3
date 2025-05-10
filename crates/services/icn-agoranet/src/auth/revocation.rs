use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    
    /// Check if a token is revoked
    fn is_revoked(&self, jti: &str) -> bool;
    
    /// Get all revoked tokens for a subject
    fn get_revoked_tokens_for_subject(&self, subject: &str) -> Vec<RevokedToken>;
    
    /// Clear expired revocations to free up memory/storage
    fn clear_expired_revocations(&self, older_than: DateTime<Utc>) -> usize;
}

/// In-memory implementation of the token revocation store
#[derive(Debug, Clone, Default)]
pub struct InMemoryRevocationStore {
    /// Set of revoked tokens
    revoked_tokens: Arc<RwLock<HashSet<RevokedToken>>>,
}

impl InMemoryRevocationStore {
    /// Create a new in-memory revocation store
    pub fn new() -> Self {
        Self {
            revoked_tokens: Arc::new(RwLock::new(HashSet::new())),
        }
    }
}

impl TokenRevocationStore for InMemoryRevocationStore {
    fn revoke_token(&self, token: RevokedToken) -> bool {
        let mut store = self.revoked_tokens.write().unwrap();
        store.insert(token)
    }
    
    fn is_revoked(&self, jti: &str) -> bool {
        let store = self.revoked_tokens.read().unwrap();
        store.iter().any(|token| token.jti == jti)
    }
    
    fn get_revoked_tokens_for_subject(&self, subject: &str) -> Vec<RevokedToken> {
        let store = self.revoked_tokens.read().unwrap();
        store
            .iter()
            .filter(|token| token.subject == subject)
            .cloned()
            .collect()
    }
    
    fn clear_expired_revocations(&self, older_than: DateTime<Utc>) -> usize {
        let mut store = self.revoked_tokens.write().unwrap();
        let before_count = store.len();
        store.retain(|token| token.revoked_at >= older_than);
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