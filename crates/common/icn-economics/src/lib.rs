#![forbid(unsafe_code)]

pub mod economics;
pub mod mana;
pub mod mana_metrics;
pub mod policy;
pub mod sled_mana_ledger;
pub mod types;

pub use economics::Economics;
pub use icn_types::resource::ResourceType;
pub use policy::ResourceAuthorizationPolicy;
// Using a different name for the import to avoid conflict
pub use economics::EconomicsError as ResourceAuthorizationError;
pub use economics::LedgerKey;

// Use the canonical Did type from icn_identity
use icn_identity::Did;

// Mana-related types will be re-exported from the new mana.rs as needed.
// For now, removing old re-exports if they conflict or are replaced by new design.
// pub use mana::{ManaPool, ManaManager, ManaError}; // Comment out for now, to be re-evaluated

use anyhow::{anyhow, Result};
use async_trait::async_trait;
// use icn_identity_core::did::Did;
// type Did = String; // DIDs are strings in the format did:key:...
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::Mutex;

/// Error types specific to the economics module
#[derive(Error, Debug)]
pub enum EconomicsError {
    #[error("Resource quota exceeded: {0}")]
    QuotaExceeded(String),

    #[error("Resource rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Invalid policy configuration: {0}")]
    InvalidPolicy(String),

    #[error("Invalid token: {0}")]
    InvalidToken(String),
}

/// Resource token with a specific scope of usage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScopedResourceToken {
    /// Type of resource (computation, storage, bandwidth, etc.)
    pub resource_type: String,

    /// Amount of resource (units depend on the resource type)
    pub amount: u64,

    /// Scope of the token (federation ID, group ID, project ID, etc.)
    pub scope: String,

    /// Optional expiration timestamp
    pub expires_at: Option<u64>,

    /// Optional issuer of the token
    pub issuer: Option<String>,
}

/// Policy for authorizing resource usage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResourceAuthorization {
    /// Allow all access to the resource
    AllowAll,

    /// Enforce a quota limit on the resource
    Quota(u64),

    /// Enforce a rate limit (amount per time period)
    RateLimit {
        /// Maximum amount per period
        amount: u64,

        /// Period in seconds
        period_secs: u64,
    },

    /// Allow only specific DIDs to access the resource
    PermitList(Vec<Did>),
}

/// Repository for resource usage tracking
#[async_trait]
pub trait ResourceRepository: Send + Sync {
    /// Record resource usage
    async fn record_usage(&self, did: &Did, token: &ScopedResourceToken) -> Result<()>;

    /// Get total resource usage for a DID and resource type within a scope
    async fn get_usage(&self, did: &Did, resource_type: &str, scope: &str) -> Result<u64>;

    /// Get usage history for rate limiting
    async fn get_usage_history(
        &self,
        did: &Did,
        resource_type: &str,
        scope: &str,
        since_timestamp: u64,
    ) -> Result<Vec<(u64, u64)>>;
}

/// Policy enforcer for resource authorization
#[async_trait]
pub trait PolicyEnforcer: Send + Sync {
    /// Check if a resource usage is authorized
    async fn check_authorization(&self, did: &Did, token: &ScopedResourceToken) -> Result<bool>;
}

/// Resource policy enforcer implementation
pub struct ResourcePolicyEnforcer {
    /// Repository for resource usage tracking
    repository: Box<dyn ResourceRepository>,

    /// Policies by resource type and scope
    policies: HashMap<(String, String), ResourceAuthorization>,
}

impl ResourcePolicyEnforcer {
    /// Create a new policy enforcer with the specified repository
    pub fn new(repository: Box<dyn ResourceRepository>) -> Self {
        Self {
            repository,
            policies: HashMap::new(),
        }
    }

    /// Set a policy for a resource type within a scope
    pub fn set_policy(&mut self, resource_type: &str, scope: &str, policy: ResourceAuthorization) {
        self.policies
            .insert((resource_type.to_string(), scope.to_string()), policy);
    }

    /// Get the policy for a resource type within a scope
    pub fn get_policy(&self, resource_type: &str, scope: &str) -> Option<&ResourceAuthorization> {
        self.policies
            .get(&(resource_type.to_string(), scope.to_string()))
    }
}

#[async_trait]
impl PolicyEnforcer for ResourcePolicyEnforcer {
    async fn check_authorization(&self, did: &Did, token: &ScopedResourceToken) -> Result<bool> {
        // Get the policy for this resource and scope
        let policy = self
            .get_policy(&token.resource_type, &token.scope)
            .ok_or_else(|| {
                anyhow!(
                    "No policy found for resource type {} in scope {}",
                    token.resource_type,
                    token.scope
                )
            })?;

        // Check if the token is expired
        if let Some(expires_at) = token.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| anyhow!("Error getting current time: {}", e))?
                .as_secs();

            if now > expires_at {
                return Err(EconomicsError::InvalidToken(format!(
                    "Token expired at {}, current time is {}",
                    expires_at, now
                ))
                .into());
            }
        }

        // Apply the policy
        match policy {
            ResourceAuthorization::AllowAll => {
                // Allow all access
                Ok(true)
            }

            ResourceAuthorization::Quota(quota) => {
                // Check total usage against quota
                let usage = self
                    .repository
                    .get_usage(did, &token.resource_type, &token.scope)
                    .await?;

                if usage + token.amount <= *quota {
                    Ok(true)
                } else {
                    Err(EconomicsError::QuotaExceeded(format!(
                        "Quota of {} exceeded (usage: {}, requested: {})",
                        quota, usage, token.amount
                    ))
                    .into())
                }
            }

            ResourceAuthorization::RateLimit {
                amount,
                period_secs,
            } => {
                // Check usage within the time period
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| anyhow!("Error getting current time: {}", e))?
                    .as_secs();

                let since = now.saturating_sub(*period_secs);

                let usage_history = self
                    .repository
                    .get_usage_history(did, &token.resource_type, &token.scope, since)
                    .await?;

                let total_usage: u64 = usage_history.iter().map(|(_, amount)| amount).sum();

                if total_usage + token.amount <= *amount {
                    Ok(true)
                } else {
                    Err(EconomicsError::RateLimitExceeded(format!(
                        "Rate limit of {} per {} seconds exceeded (usage: {}, requested: {})",
                        amount, period_secs, total_usage, token.amount
                    ))
                    .into())
                }
            }

            ResourceAuthorization::PermitList(permits) => {
                // Check if DID is in the permit list
                if permits.contains(did) {
                    Ok(true)
                } else {
                    Err(EconomicsError::AccessDenied(format!(
                        "DID {} not in permit list for resource type {} in scope {}",
                        did, token.resource_type, token.scope
                    ))
                    .into())
                }
            }
        }
    }
}

/// In-memory implementation of the ResourceRepository trait for testing
#[derive(Default)]
pub struct InMemoryResourceRepository {
    /// Usage records (did, resource_type, scope) -> [(timestamp, amount)]
    usage: Mutex<HashMap<(String, String, String), Vec<(u64, u64)>>>,
}

impl InMemoryResourceRepository {
    pub fn new() -> Self {
        Self {
            usage: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl ResourceRepository for InMemoryResourceRepository {
    async fn record_usage(&self, did: &Did, token: &ScopedResourceToken) -> Result<()> {
        let key = (
            did.to_string(),
            token.resource_type.clone(),
            token.scope.clone(),
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow!("Error getting current time: {}", e))?
            .as_secs();

        let mut usage_guard = self.usage.lock().await;
        usage_guard
            .entry(key)
            .or_default()
            .push((now, token.amount));
        Ok(())
    }

    async fn get_usage(&self, did: &Did, resource_type: &str, scope: &str) -> Result<u64> {
        let key = (
            did.to_string(),
            resource_type.to_string(),
            scope.to_string(),
        );
        let usage_guard = self.usage.lock().await;
        Ok(usage_guard
            .get(&key)
            .map_or(0, |records| records.iter().map(|(_, amount)| amount).sum()))
    }

    async fn get_usage_history(
        &self,
        did: &Did,
        resource_type: &str,
        scope: &str,
        since_timestamp: u64,
    ) -> Result<Vec<(u64, u64)>> {
        let key = (
            did.to_string(),
            resource_type.to_string(),
            scope.to_string(),
        );
        let usage_guard = self.usage.lock().await;
        Ok(usage_guard.get(&key).map_or_else(Vec::new, |records| {
            records
                .iter()
                .filter(|(ts, _)| *ts >= since_timestamp)
                .cloned()
                .collect()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    // Helper to create a DID for testing
    fn test_did() -> Did {
        KeyPair::generate().did
    }

    #[tokio::test]
    async fn test_allow_all_policy() {
        let repo = Box::new(InMemoryResourceRepository::default());
        let mut enforcer = ResourcePolicyEnforcer::new(repo);
        enforcer.set_policy("compute", "global", ResourceAuthorization::AllowAll);

        let token = ScopedResourceToken {
            resource_type: "compute".to_string(),
            amount: 100,
            scope: "global".to_string(),
            expires_at: None,
            issuer: None,
        };
        assert!(enforcer
            .check_authorization(&test_did(), &token)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_quota_policy() {
        let repo = Box::new(InMemoryResourceRepository::default());
        let mut enforcer = ResourcePolicyEnforcer::new(repo);
        enforcer.set_policy("storage", "project_x", ResourceAuthorization::Quota(1000));

        let did = test_did();
        let token1 = ScopedResourceToken {
            resource_type: "storage".to_string(),
            amount: 600,
            scope: "project_x".to_string(),
            expires_at: None,
            issuer: None,
        };
        assert!(enforcer.check_authorization(&did, &token1).await.unwrap());
        enforcer
            .repository
            .record_usage(&did, &token1)
            .await
            .unwrap();

        let token2 = ScopedResourceToken {
            resource_type: "storage".to_string(),
            amount: 300,
            scope: "project_x".to_string(),
            expires_at: None,
            issuer: None,
        };
        assert!(enforcer.check_authorization(&did, &token2).await.unwrap());
        enforcer
            .repository
            .record_usage(&did, &token2)
            .await
            .unwrap();

        let token3 = ScopedResourceToken {
            resource_type: "storage".to_string(),
            amount: 200, // This should exceed the quota (600+300+200 = 1100 > 1000)
            scope: "project_x".to_string(),
            expires_at: None,
            issuer: None,
        };
        let result = enforcer.check_authorization(&did, &token3).await;
        assert!(result.is_err());
        match result.err().unwrap().downcast_ref::<EconomicsError>() {
            Some(EconomicsError::QuotaExceeded(_)) => {} // Expected
            _ => panic!("Expected QuotaExceeded error"),
        }
    }

    #[tokio::test]
    async fn test_rate_limit_policy() {
        let repo = Box::new(InMemoryResourceRepository::default());
        let mut enforcer = ResourcePolicyEnforcer::new(repo);
        enforcer.set_policy(
            "api_calls",
            "user_group_a",
            ResourceAuthorization::RateLimit {
                amount: 3,
                period_secs: 60,
            },
        );

        let did = test_did();
        let create_token = || ScopedResourceToken {
            resource_type: "api_calls".to_string(),
            amount: 1,
            scope: "user_group_a".to_string(),
            expires_at: None,
            issuer: None,
        };

        // First 3 calls should succeed
        for _ in 0..3 {
            let token = create_token();
            assert!(enforcer.check_authorization(&did, &token).await.unwrap());
            enforcer
                .repository
                .record_usage(&did, &token)
                .await
                .unwrap();
        }

        // 4th call should fail
        let token4 = create_token();
        let result = enforcer.check_authorization(&did, &token4).await;
        assert!(result.is_err());
        match result.err().unwrap().downcast_ref::<EconomicsError>() {
            Some(EconomicsError::RateLimitExceeded(_)) => {} // Expected
            _ => panic!("Expected RateLimitExceeded error"),
        }

        // If we could advance time by 60s here, the limit would reset.
        // For simplicity, this test only checks immediate rate limiting.
    }

    #[tokio::test]
    async fn test_permit_list_policy() {
        let repo = Box::new(InMemoryResourceRepository::default());
        let mut enforcer = ResourcePolicyEnforcer::new(repo);
        let did1 = test_did();
        let did2 = test_did();
        let did3 = test_did();

        enforcer.set_policy(
            "special_feature",
            "beta_users",
            ResourceAuthorization::PermitList(vec![did1.clone(), did2.clone()]),
        );

        let token = ScopedResourceToken {
            resource_type: "special_feature".to_string(),
            amount: 1,
            scope: "beta_users".to_string(),
            expires_at: None,
            issuer: None,
        };

        assert!(enforcer.check_authorization(&did1, &token).await.unwrap());
        assert!(enforcer.check_authorization(&did2, &token).await.unwrap());
        let result = enforcer.check_authorization(&did3, &token).await;
        assert!(result.is_err());
        match result.err().unwrap().downcast_ref::<EconomicsError>() {
            Some(EconomicsError::AccessDenied(_)) => {} // Expected
            _ => panic!("Expected AccessDenied error"),
        }
    }

    #[tokio::test]
    async fn test_expired_token() {
        let repo = Box::new(InMemoryResourceRepository::default());
        let mut enforcer = ResourcePolicyEnforcer::new(repo);
        enforcer.set_policy("compute", "global", ResourceAuthorization::AllowAll);

        let past_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 3600; // 1 hour ago

        let token = ScopedResourceToken {
            resource_type: "compute".to_string(),
            amount: 100,
            scope: "global".to_string(),
            expires_at: Some(past_timestamp),
            issuer: None,
        };
        let result = enforcer.check_authorization(&test_did(), &token).await;
        assert!(result.is_err());
        match result.err().unwrap().downcast_ref::<EconomicsError>() {
            Some(EconomicsError::InvalidToken(_)) => {} // Expected
            _ => panic!("Expected InvalidToken error for expired token"),
        }
    }
}
