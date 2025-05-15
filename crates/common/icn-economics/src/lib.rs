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
pub use icn_types::EconomicsError as ResourceAuthorizationError;
pub use economics::LedgerKey;

// Use the canonical Did type from icn_identity
use icn_identity::Did;
// use icn_types::EconomicsError; // This direct import is fine, or can be removed if ResourceAuthorizationError is used exclusively

// Mana-related types will be re-exported from the new mana.rs as needed.
// For now, removing old re-exports if they conflict or are replaced by new design.
// pub use mana::{ManaPool, ManaManager, ManaError}; // Comment out for now, to be re-evaluated

use anyhow::{anyhow, Result};
use async_trait::async_trait;
// use icn_identity_core::did::Did;
// type Did = String; // DIDs are strings in the format did:key:...
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::Mutex;

pub type UsageKey = (String, String, String);
pub type UsageData = Vec<(u64, u64)>;

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
    async fn check_authorization(&self, did: &Did, token: &ScopedResourceToken) -> Result<bool, ResourceAuthorizationError>;
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
    async fn check_authorization(&self, did: &Did, token: &ScopedResourceToken) -> Result<bool, ResourceAuthorizationError> {
        // Get the policy for this resource and scope
        let policy = self
            .get_policy(&token.resource_type, &token.scope)
            .ok_or_else(|| {
                ResourceAuthorizationError::NoPolicyFound {
                    resource_type: token.resource_type.clone(),
                    scope: token.scope.clone(),
                }
            })?;

        // Check if the token is expired
        if let Some(expires_at) = token.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| ResourceAuthorizationError::SystemTimeError(e.to_string()))?
                .as_secs();

            if now > expires_at {
                return Err(ResourceAuthorizationError::TokenExpired {
                    expires_at,
                    current_time: now,
                    resource_type: token.resource_type.clone(),
                    scope: token.scope.clone(),
                });
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
                    .await
                    .map_err(|e| ResourceAuthorizationError::SystemTimeError(format!("Failed to get usage: {}", e)))?; // Assuming get_usage can also have misc errors, mapping to SystemTimeError for now or needs own variant

                if usage + token.amount <= *quota {
                    Ok(true)
                } else {
                    Err(ResourceAuthorizationError::QuotaExceeded {
                        quota: *quota,
                        current_usage: usage,
                        requested_amount: token.amount,
                        resource_type: token.resource_type.clone(),
                        scope: token.scope.clone(),
                    })
                }
            }

            ResourceAuthorization::RateLimit {
                amount,
                period_secs,
            } => {
                // Check usage within the time period
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| ResourceAuthorizationError::SystemTimeError(e.to_string()))?
                    .as_secs();

                let since = now.saturating_sub(*period_secs);

                let usage_history = self
                    .repository
                    .get_usage_history(did, &token.resource_type, &token.scope, since)
                    .await
                    .map_err(|e| ResourceAuthorizationError::SystemTimeError(format!("Failed to get usage history: {}", e)))?; // Similar to get_usage

                let total_usage: u64 = usage_history.iter().map(|(_, amount)| amount).sum();

                if total_usage + token.amount <= *amount {
                    Ok(true)
                } else {
                    Err(ResourceAuthorizationError::RateLimitExceeded {
                        limit_amount: *amount,
                        period_seconds: *period_secs,
                        current_usage_in_period: total_usage,
                        requested_amount: token.amount,
                        resource_type: token.resource_type.clone(),
                        scope: token.scope.clone(),
                    })
                }
            }

            ResourceAuthorization::PermitList(permits) => {
                // Check if DID is in the permit list
                if permits.contains(did) {
                    Ok(true)
                } else {
                    Err(ResourceAuthorizationError::AccessDenied {
                        did: did.clone(), // Assuming Did is Cloneable
                        resource_type: token.resource_type.clone(),
                        scope: token.scope.clone(),
                    })
                }
            }
        }
    }
}

/// In-memory implementation of the ResourceRepository trait for testing
#[derive(Debug, Default)]
pub struct InMemoryResourceRepository {
    /// Usage records (did, resource_type, scope) -> [(timestamp, amount)]
    usage: Mutex<HashMap<UsageKey, UsageData>>,
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

pub use sled_mana_ledger::SledManaLedger;

// ---- New ManaRepositoryAdapter ----
use crate::mana::{ManaLedger, ManaState};
use std::sync::Arc;

/// Adapts a ManaLedger to the ResourceRepository trait for "mana" tokens.
pub struct ManaRepositoryAdapter<L: ManaLedger> {
    ledger: Arc<L>,
}

impl<L: ManaLedger> ManaRepositoryAdapter<L> {
    /// Creates a new ManaRepositoryAdapter.
    #[allow(dead_code)] // Potentially unused initially
    pub fn new(ledger: Arc<L>) -> Self {
        Self { ledger }
    }
}

#[async_trait::async_trait] // Ensure async_trait is available
impl<L: ManaLedger + Send + Sync + 'static> ResourceRepository for ManaRepositoryAdapter<L> {
    async fn record_usage(&self, did: &Did, token: &ScopedResourceToken) -> Result<()> {
        if token.resource_type != "mana" {
            return Err(anyhow::anyhow!(
                "ManaRepositoryAdapter: unsupported resource type '{}', expected 'mana'",
                token.resource_type
            ));
        }

        // Ensure scope matches if relevant, for now, we assume "mana" is global for the DID
        // or the scope in ScopedResourceToken isn't strictly enforced by ManaState itself.

        let maybe_state = self.ledger.get_mana_state(did).await.map_err(|e| {
            anyhow::anyhow!("Failed to get mana state for DID {}: {}", did, e)
        })?;

        let mut state = maybe_state.unwrap_or_else(|| {
            // Default state if DID has no mana record yet.
            // Max mana and regen rate would ideally come from a default policy or config.
            // For now, if no record, they can't spend unless these are non-zero.
            ManaState {
                current_mana: 0,
                max_mana: 0, // Or a default capacity if applicable
                regen_rate_per_epoch: 0.0,
                last_updated_epoch: 0, // Or current epoch if relevant
            }
        });

        if state.current_mana < token.amount {
            return Err(anyhow::anyhow!(
                "Insufficient mana for DID {}: has {}, needs {}",
                did,
                state.current_mana,
                token.amount
            ));
        }

        state.current_mana -= token.amount;
        self.ledger
            .update_mana_state(did, state)
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to update mana state for DID {}: {}", did, e)
            })?;
        Ok(())
    }

    async fn get_usage(&self, did: &Did, resource_type: &str, _scope: &str) -> Result<u64> {
        if resource_type != "mana" {
            return Err(anyhow::anyhow!(
                "ManaRepositoryAdapter: unsupported resource type '{}', expected 'mana'",
                resource_type
            ));
        }
        let state = self.ledger.get_mana_state(did).await.map_err(|e| {
            anyhow::anyhow!("Failed to get mana state for DID {}: {}", did, e)
        })?;
        Ok(state.map(|s| s.current_mana).unwrap_or(0))
    }

    async fn get_usage_history(
        &self,
        _did: &Did,
        resource_type: &str,
        _scope: &str,
        _since_epoch: u64,
    ) -> Result<Vec<(u64, u64)>> {
         if resource_type != "mana" {
            return Err(anyhow::anyhow!(
                "ManaRepositoryAdapter: unsupported resource type '{}', expected 'mana'",
                resource_type
            ));
        }
        // Optional: Implement actual history retrieval if ManaLedger supports it
        // or if regeneration logic needs to be factored in for rate limiting.
        // For now, returns an empty Vec as per the plan.
        Ok(vec![])
    }
}

// ---- End ManaRepositoryAdapter ----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mana::InMemoryManaLedger;
    use std::str::FromStr; // For Did::from_str
    use icn_identity::KeyPair; // For the test_did() helper function

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
        match result.err().unwrap() {
            ResourceAuthorizationError::QuotaExceeded { .. } => {} // Expected
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
        match result.err().unwrap() {
            ResourceAuthorizationError::RateLimitExceeded { .. } => {} // Expected
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
        match result.err().unwrap() {
            ResourceAuthorizationError::AccessDenied { .. } => {} // Expected
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
        match result.err().unwrap() {
            ResourceAuthorizationError::TokenExpired { .. } => {} // Expected
            _ => panic!("Expected TokenExpired error for expired token"),
        }
    }

    #[tokio::test]
    async fn test_mana_repository_adapter_records_usage() {
        let ledger = Arc::new(InMemoryManaLedger::new());
        let adapter = ManaRepositoryAdapter::new(ledger.clone()); // Use constructor

        // Create a DID. Assuming Did can be created from a string for test purposes.
        // If Did::new is not available, adjust accordingly (e.g. KeyPair::new().did)
        let did_str = "did:example:123456789abcdefghi";
        let did = Did::from_str(did_str).expect("Failed to create DID for test");

        let initial_state = ManaState {
            current_mana: 100,
            max_mana: 100,          // or some other value
            regen_rate_per_epoch: 1.0, // or some other value
            last_updated_epoch: 0,   // or some other value
        };
        ledger
            .update_mana_state(&did, initial_state.clone())
            .await
            .unwrap();

        let token = ScopedResourceToken {
            resource_type: "mana".to_string(),
            amount: 40,
            scope: "default".to_string(), // Scope might be used by PolicyEnforcer
            expires_at: None,
            issuer: None,
        };

        adapter.record_usage(&did, &token).await.unwrap();
        let remaining = adapter
            .get_usage(&did, "mana", "default")
            .await
            .unwrap();
        assert_eq!(remaining, 60, "Mana should be debited correctly");

        // Test spending more than available
        let token_overdraft = ScopedResourceToken {
            resource_type: "mana".to_string(),
            amount: 70, // remaining is 60, this should fail
            scope: "default".to_string(),
            expires_at: None,
            issuer: None,
        };
        let overdraft_result = adapter.record_usage(&did, &token_overdraft).await;
        assert!(overdraft_result.is_err(), "Should not allow overdraft");
        if let Err(e) = overdraft_result {
            assert!(
                e.to_string().contains("Insufficient mana"),
                "Error message should indicate insufficient mana"
            );
        }


        let remaining_after_failed_spend = adapter
            .get_usage(&did, "mana", "default")
            .await
            .unwrap();
        assert_eq!(remaining_after_failed_spend, 60, "Mana should not change on failed spend");


        // Test spending exact remaining mana
        let token_exact_spend = ScopedResourceToken {
            resource_type: "mana".to_string(),
            amount: 60,
            scope: "default".to_string(),
            expires_at: None,
            issuer: None,
        };
         adapter.record_usage(&did, &token_exact_spend).await.unwrap();
         let remaining_after_exact_spend = adapter
            .get_usage(&did, "mana", "default")
            .await
            .unwrap();
        assert_eq!(remaining_after_exact_spend, 0, "Mana should be zero after exact spend");


        // Test spending from a DID with no prior record
        let did_new_str = "did:example:newuser";
        let did_new = Did::from_str(did_new_str).expect("Failed to create new DID for test");
        let token_for_new_user = ScopedResourceToken {
            resource_type: "mana".to_string(),
            amount: 10,
            scope: "default".to_string(),
            expires_at: None,
            issuer: None,
        };
        let new_user_result = adapter.record_usage(&did_new, &token_for_new_user).await;
        assert!(new_user_result.is_err(), "Should not allow spending for new user with 0 default mana");
         if let Err(e) = new_user_result {
            assert!(
                e.to_string().contains("Insufficient mana"),
                "Error message for new user should indicate insufficient mana"
            );
        }

        // Test unsupported resource type for record_usage
        let non_mana_token_record = ScopedResourceToken {
            resource_type: "compute".to_string(),
            amount: 10,
            scope: "default".to_string(),
            expires_at: None,
            issuer: None,
        };
        let non_mana_result_record = adapter.record_usage(&did, &non_mana_token_record).await;
        assert!(non_mana_result_record.is_err(), "Should reject non-mana resource type for record_usage");
         if let Err(e) = non_mana_result_record {
            assert!(
                e.to_string().contains("unsupported resource type"),
                "Error message for record_usage should indicate unsupported resource type"
            );
        }


        // Test unsupported resource type for get_usage
        let non_mana_result_get = adapter.get_usage(&did, "compute", "default").await;
        assert!(non_mana_result_get.is_err(), "Should reject non-mana resource type for get_usage");
        if let Err(e) = non_mana_result_get {
            assert!(
                e.to_string().contains("unsupported resource type"),
                "Error message for get_usage should indicate unsupported resource type"
            );
        }
    }

    #[tokio::test]
    async fn test_policy_enforcer_with_mana_quota() {
        use tempfile::tempdir; // For SledManaLedger temporary directory

        // 1. Instantiate a SledManaLedger using a temporary directory.
        let dir = tempdir().expect("Failed to create temp dir for SledManaLedger");
        let sled_ledger = SledManaLedger::open(dir.path()).expect("Failed to open SledManaLedger");

        // 2. Insert an initial ManaState for a test Did
        let did_alice_str = "did:coop:example:alice";
        let did_alice = Did::from_str(did_alice_str).expect("Failed to create alice DID");
        let initial_mana_state = ManaState {
            current_mana: 50,
            max_mana: 100,
            regen_rate_per_epoch: 1.0,
            last_updated_epoch: 0,
        };
        sled_ledger
            .update_mana_state(&did_alice, initial_mana_state.clone())
            .await
            .expect("Failed to set initial mana state for alice");

        // 3. Construct a ManaRepositoryAdapter instance for direct use (if needed for direct calls later)
        // and an Arc for the SledManaLedger to be shared.
        let shared_sled_ledger_arc = Arc::new(sled_ledger.clone());
        let direct_mana_repo_adapter = ManaRepositoryAdapter::new(shared_sled_ledger_arc.clone());

        // 4. Define a ScopedResourceToken of type "mana"
        let mana_token_spend_30 = ScopedResourceToken {
            resource_type: "mana".to_string(),
            amount: 30,
            scope: "global_mana_scope".to_string(), // Arbitrary scope
            expires_at: None,
            issuer: None,
        };

        // 5. Define a ResourceAuthorization::Quota(40) for "mana"
        let mana_quota_policy = ResourceAuthorization::Quota(40);

        // 6. Instantiate a ResourcePolicyEnforcer
        // Create a new ManaRepositoryAdapter instance specifically for the PolicyEnforcer's Box.
        // It will share the same underlying Sled database via the Arc-ed SledManaLedger.
        let enforcer_repo = ManaRepositoryAdapter::new(shared_sled_ledger_arc.clone());
        let mut policy_enforcer = ResourcePolicyEnforcer::new(Box::new(enforcer_repo));

        policy_enforcer.set_policy(
            &mana_token_spend_30.resource_type,
            &mana_token_spend_30.scope,
            mana_quota_policy,
        );

        // 7. Call check_authorization — it should return true
        let auth_result1 = policy_enforcer
            .check_authorization(&did_alice, &mana_token_spend_30)
            .await;
        assert!(auth_result1.is_ok(), "Auth check 1 failed: {:?}", auth_result1.err());
        assert!(auth_result1.unwrap(), "Auth check 1 should be true (30 <= 40, has 50)");

        // 8. Call record_usage for that token using the direct adapter instance.
        direct_mana_repo_adapter
            .record_usage(&did_alice, &mana_token_spend_30)
            .await
            .expect("Failed to record mana usage");

        // Verify mana was deducted using the direct adapter instance.
        let current_mana_after_spend = direct_mana_repo_adapter
            .get_usage(&did_alice, "mana", &mana_token_spend_30.scope)
            .await
            .expect("Failed to get current mana after spend");
        assert_eq!(current_mana_after_spend, 20, "Alice's mana should be 20 after spending 30 (50-30)");

        // 9. Call check_authorization again with amount 25
        let mana_token_spend_25 = ScopedResourceToken {
            resource_type: "mana".to_string(),
            amount: 25,
            scope: "global_mana_scope".to_string(),
            expires_at: None,
            issuer: None,
        };

        let auth_result2 = policy_enforcer
            .check_authorization(&did_alice, &mana_token_spend_25)
            .await;
        assert!(auth_result2.is_err(), "Auth check 2 should fail due to quota");
        match auth_result2.err().unwrap() {
            ResourceAuthorizationError::QuotaExceeded { .. } => {} // Expected
            other_err => panic!("Expected QuotaExceeded error, got {:?}", other_err),
        }

        // Test case: Quota allows, but ledger balance (via adapter) would prevent actual record_usage
        policy_enforcer.set_policy(
            &mana_token_spend_30.resource_type,
            &mana_token_spend_30.scope,
            ResourceAuthorization::Quota(60), // New quota: 60. User has 20 mana. Tries to spend 25.
        );
        let auth_result3 = policy_enforcer
            .check_authorization(&did_alice, &mana_token_spend_25)
            .await;
        assert!(auth_result3.is_ok(), "Auth check 3 (policy pass) failed: {:?}", auth_result3.err());
        assert!(auth_result3.unwrap(), "Auth check 3 should be true by policy (20+25 <= 60)");

        let record_result_after_policy_pass = direct_mana_repo_adapter // Use the direct adapter for this check
            .record_usage(&did_alice, &mana_token_spend_25)
            .await;
        assert!(record_result_after_policy_pass.is_err(), "Recording usage should fail due to insufficient mana in ledger");
        if let Err(e) = record_result_after_policy_pass {
             assert!(
                e.to_string().contains("Insufficient mana"),
                "Error message for recording should be Insufficient mana, got: {}", e
            );
        }
    }
}
