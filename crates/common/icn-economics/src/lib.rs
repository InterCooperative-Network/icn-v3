#![forbid(unsafe_code)]

pub mod types;
pub mod policy;
pub mod economics;

pub use types::ResourceType;
pub use policy::ResourceAuthorizationPolicy;
pub use economics::Economics;
// Using a different name for the import to avoid conflict
pub use economics::EconomicsError as ResourceAuthorizationError;
pub use economics::LedgerKey;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
// use icn_identity_core::did::Did;
type Did = String; // DIDs are strings in the format did:key:...
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

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
    PermitList(Vec<String>),
}

/// Repository for resource usage tracking
#[async_trait]
pub trait ResourceRepository {
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
pub trait PolicyEnforcer {
    /// Check if a resource usage is authorized
    async fn check_authorization(&self, did: &Did, token: &ScopedResourceToken) -> Result<bool>;
}

/// Resource policy enforcer implementation
pub struct ResourcePolicyEnforcer {
    /// Repository for resource usage tracking
    repository: Box<dyn ResourceRepository + Send + Sync>,

    /// Policies by resource type and scope
    policies: HashMap<(String, String), ResourceAuthorization>,
}

impl ResourcePolicyEnforcer {
    /// Create a new policy enforcer with the specified repository
    pub fn new(repository: Box<dyn ResourceRepository + Send + Sync>) -> Self {
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
                if permits.contains(&did.to_string()) {
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
    usage: HashMap<(String, String, String), Vec<(u64, u64)>>,
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

        // Clone the repository since we're working with &self
        let mut usage = self.usage.clone();

        // Add the usage record
        let records = usage.entry(key).or_default();
        records.push((now, token.amount));

        // Update the cloned repository
        // Note: In a real implementation, this would persist to a database
        Ok(())
    }

    async fn get_usage(&self, did: &Did, resource_type: &str, scope: &str) -> Result<u64> {
        let key = (
            did.to_string(),
            resource_type.to_string(),
            scope.to_string(),
        );

        // Sum up all usage
        let total = self
            .usage
            .get(&key)
            .map(|records| records.iter().map(|(_, amount)| amount).sum())
            .unwrap_or(0);

        Ok(total)
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

        // Filter usage records by timestamp
        let filtered = self
            .usage
            .get(&key)
            .map(|records| {
                records
                    .iter()
                    .filter(|(timestamp, _)| *timestamp >= since_timestamp)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        Ok(filtered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_quota_policy() {
        // Set up a repository
        let repository = Box::new(InMemoryResourceRepository::default());

        // Set up a policy enforcer
        let mut enforcer = ResourcePolicyEnforcer::new(repository);

        // Set a quota policy
        enforcer.set_policy("compute", "test-scope", ResourceAuthorization::Quota(100));

        // Create a test DID
        let did = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string();

        // Create a token
        let token = ScopedResourceToken {
            resource_type: "compute".to_string(),
            amount: 50,
            scope: "test-scope".to_string(),
            expires_at: None,
            issuer: None,
        };

        // Check authorization (should pass)
        let result = enforcer.check_authorization(&did, &token).await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Create a token that exceeds the quota
        let token2 = ScopedResourceToken {
            resource_type: "compute".to_string(),
            amount: 101,
            scope: "test-scope".to_string(),
            expires_at: None,
            issuer: None,
        };

        // Check authorization (should fail)
        let result = enforcer.check_authorization(&did, &token2).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Quota of 100 exceeded"));
    }

    #[tokio::test]
    async fn test_permit_list_policy() {
        // Set up a repository
        let repository = Box::new(InMemoryResourceRepository::default());

        // Set up a policy enforcer
        let mut enforcer = ResourcePolicyEnforcer::new(repository);

        // Create test DIDs
        let did1 = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string();
        let did2 = "did:key:z6MkuBsxRsRu3PU1VzZ5xnqNtXWRwLtrGdxdMeMFuxP5xyVp".to_string();

        // Set a permit list policy that includes did1 but not did2
        enforcer.set_policy(
            "admin",
            "test-scope",
            ResourceAuthorization::PermitList(vec![did1.to_string()]),
        );

        // Create a token
        let token = ScopedResourceToken {
            resource_type: "admin".to_string(),
            amount: 1,
            scope: "test-scope".to_string(),
            expires_at: None,
            issuer: None,
        };

        // Check authorization for did1 (should pass)
        let result = enforcer.check_authorization(&did1, &token).await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Check authorization for did2 (should fail)
        let result = enforcer.check_authorization(&did2, &token).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not in permit list"));
    }
}
