use crate::mana_metrics::*;
use anyhow::Result;
use async_trait::async_trait;
use icn_identity::Did;
use icn_identity::ScopeKey;
pub use icn_types::mana::ManaState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{self, debug, trace, warn};

/// Trait for reporting mana balance changes to a metrics system.
pub trait ManaMetricsHook: std::fmt::Debug {
    /// Update the reported balance for a given scope.
    fn update_balance(&self, scope: &ScopeKey, balance: u64);
    // Optional: Add fn remove_balance(&self, scope: &ScopeKey); if pools can be deleted.
}

#[derive(Debug, thiserror::Error)]
pub enum ManaError {
    #[error("Insufficient mana: requested {requested}, available {available}")]
    InsufficientMana { requested: u64, available: u64 },
}

#[derive(Debug, Clone)]
pub struct ManaPool {
    /// Current available mana units
    current: u64,
    /// Maximum mana that can be accumulated
    max: u64,
    /// Regeneration rate per second
    regen_per_sec: u64,
    /// Unix timestamp of the last time the pool was updated
    last_updated: u64,
}

impl ManaPool {
    pub fn new(max: u64, regen_per_sec: u64) -> Self {
        Self {
            current: max,
            max,
            regen_per_sec,
            last_updated: now_secs(),
        }
    }

    /// Return the currently available mana after applying regeneration.
    /// Updates metrics hook if balance changed.
    pub fn available(
        &mut self,
        scope: &ScopeKey,
        hook: Option<&(dyn ManaMetricsHook + Send + Sync)>,
    ) -> u64 {
        let old_balance = self.current;
        self.apply_regeneration();
        if self.current != old_balance {
            if let Some(h) = hook {
                h.update_balance(scope, self.current);
            }
        }
        self.current
    }

    /// Attempt to consume the requested amount of mana. Returns Ok(()) if successful, otherwise Err.
    /// Updates metrics hook on success.
    pub fn consume(
        &mut self,
        amount: u64,
        scope: &ScopeKey,
        hook: Option<&(dyn ManaMetricsHook + Send + Sync)>,
    ) -> Result<(), ManaError> {
        // available() already calls apply_regeneration and updates the hook if needed
        let current_available = self.available(scope, hook);
        if current_available >= amount {
            self.current -= amount;
            if let Some(h) = hook {
                h.update_balance(scope, self.current);
            }
            Ok(())
        } else {
            Err(ManaError::InsufficientMana {
                requested: amount,
                available: current_available,
            })
        }
    }

    /// Adds regeneration based on time elapsed. Does NOT update hook directly.
    fn apply_regeneration(&mut self) {
        let now = now_secs();
        if now > self.last_updated {
            let elapsed = now - self.last_updated;
            let regen_amount = elapsed * self.regen_per_sec;
            self.current = (self.current + regen_amount).min(self.max);
            self.last_updated = now;
        }
    }

    /// Credit the pool with additional mana, respecting the max cap.
    /// Updates metrics hook.
    pub fn credit(
        &mut self,
        amount: u64,
        scope: &ScopeKey,
        hook: Option<&(dyn ManaMetricsHook + Send + Sync)>,
    ) {
        // available() already calls apply_regeneration and updates the hook if needed
        self.available(scope, hook);
        let old_balance = self.current;
        self.current = (self.current + amount).min(self.max);
        if self.current != old_balance {
            if let Some(h) = hook {
                h.update_balance(scope, self.current);
            }
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Manages mana pools for multiple DIDs/orgs.
#[derive(Debug)]
pub struct ManaManager {
    pools: HashMap<ScopeKey, ManaPool>,
    metrics_hook: Option<Arc<dyn ManaMetricsHook + Send + Sync>>,
}

impl ManaManager {
    /// Creates a new ManaManager without metrics reporting.
    pub fn new() -> Self {
        Self {
            pools: HashMap::new(),
            metrics_hook: None,
        }
    }

    /// Creates a new ManaManager with metrics reporting enabled.
    pub fn with_metrics_hook(hook: Arc<dyn ManaMetricsHook + Send + Sync>) -> Self {
        Self {
            pools: HashMap::new(),
            metrics_hook: Some(hook),
        }
    }

    pub fn ensure_pool(&mut self, key: &ScopeKey, max: u64, regen_per_sec: u64) {
        let hook = self.metrics_hook.clone();
        let pool = self.pools.entry(key.clone()).or_insert_with(|| {
            let new_pool = ManaPool::new(max, regen_per_sec);
            // Report initial balance when pool is created
            if let Some(h) = &hook {
                h.update_balance(key, new_pool.current);
            }
            new_pool
        });
        // If pool already existed, ensure metrics are up-to-date (e.g., after restart)
        // available() handles regeneration update internally
        let current_balance = pool.available(key, hook.as_deref());
        if let Some(h) = &hook {
            h.update_balance(key, current_balance);
        }
    }

    /// Get mutable reference to a mana pool if it exists.
    pub fn pool_mut(&mut self, key: &ScopeKey) -> Option<&mut ManaPool> {
        // Note: Getting mut ref doesn't change balance, but subsequent ops might.
        // We don't update metrics here.
        self.pools.get_mut(key)
    }

    /// Get current available mana balance for the key after regeneration.
    /// Updates metrics via available().
    pub fn balance(&mut self, key: &ScopeKey) -> Option<u64> {
        let hook = self.metrics_hook.as_deref();
        self.pools.get_mut(key).map(|p| p.available(key, hook))
    }

    /// Spend the specified amount of mana from the key's pool.
    /// Updates metrics via consume().
    pub fn spend(&mut self, key: &ScopeKey, amount: u64) -> Result<(), ManaError> {
        let hook = self.metrics_hook.as_deref();
        match self.pools.get_mut(key) {
            Some(pool) => pool.consume(amount, key, hook),
            None => Err(ManaError::InsufficientMana {
                requested: amount,
                available: 0,
            }),
        }
    }

    /// Atomically transfer mana between scopes.
    /// Updates metrics via spend() and credit().
    pub fn transfer(
        &mut self,
        from: &ScopeKey,
        to: &ScopeKey,
        amount: u64,
    ) -> Result<(), ManaError> {
        // Spend first (updates metrics)
        self.spend(from, amount)?;

        // Ensure destination pool exists (updates metrics on creation)
        // If pool exists, ensure_pool also updates metrics via available()
        self.ensure_pool(to, amount, 1); // Default regen 1/s if created

        // Credit destination (updates metrics)
        // We can safely unwrap here because ensure_pool guarantees existence.
        if let Some(pool) = self.pools.get_mut(to) {
            let hook = self.metrics_hook.as_deref();
            pool.credit(amount, to, hook);
        } else {
            // This case should ideally be unreachable due to ensure_pool
            eprintln!("Error: Destination pool missing after ensure_pool in transfer");
            // Potentially return an error or panic depending on desired robustness
        }

        Ok(())
    }
}

impl Default for ManaManager {
    fn default() -> Self {
        Self::new()
    }
}

// --- ManaLedger Trait ---
#[async_trait]
pub trait ManaLedger: Send + Sync {
    async fn get_mana_state(&self, did: &Did) -> Result<Option<ManaState>>;
    async fn update_mana_state(&self, did: &Did, new_state: ManaState) -> Result<()>;
    async fn all_dids(&self) -> Result<Vec<Did>>;
}

// --- RegenerationPolicy Enum ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegenerationPolicy {
    FixedRatePerTick(u64), // Fixed mana regenerated each tick
                           // Future policies could include: PercentageOfMax, ReputationScaled, etc.
}

// --- ManaRegenerator Struct ---

#[derive(Debug)]
pub struct RegenerationTickDetails {
    pub processed_dids_count: usize,
    pub regenerated_dids_count: usize,
    pub errors: Vec<(Did, String)>,
}

pub struct ManaRegenerator<L: ManaLedger> {
    pub ledger: Arc<L>,
    pub policy: RegenerationPolicy,
    // Could also include a time source or epoch tracker if regeneration is time-dependent
}

impl<L: ManaLedger + Send + Sync> ManaRegenerator<L> {
    // Ensure L is Send + Sync for Arc<L>
    pub fn new(ledger: Arc<L>, policy: RegenerationPolicy) -> Self {
        Self { ledger, policy }
    }

    pub async fn tick(&self) -> Result<RegenerationTickDetails> {
        let mut regenerated_dids_count = 0;
        let mut errors = Vec::new();

        let dids_result = self.ledger.all_dids().await;
        let processed_dids_count_val: usize;

        match dids_result {
            Ok(dids) => {
                processed_dids_count_val = dids.len();
                MANA_ACTIVE_DIDS_GAUGE.set(dids.len() as i64); // Set active DIDs gauge

                for did in dids {
                    match self.ledger.get_mana_state(&did).await {
                        Ok(Some(mut state)) => {
                            let original_mana = state.current_mana;

                            let RegenerationPolicy::FixedRatePerTick(regen_amount) = self.policy;

                            state.current_mana =
                                (state.current_mana + regen_amount).min(state.max_mana);

                            if state.current_mana != original_mana {
                                regenerated_dids_count += 1;
                                if let Err(e) =
                                    self.ledger.update_mana_state(&did, state.clone()).await
                                {
                                    // Pass cloned state
                                    errors.push((did.clone(), format!("update_failed: {}", e)));
                                } else {
                                    // Successfully updated, log if needed (original log was here)
                                    // Log was: tracing::debug!(did = %did, old_mana = original_mana, new_mana = state.current_mana, regen_amount = regen_amount, "Mana regenerated");
                                    // Avoiding ledger read just for log: new_mana = state.current_mana
                                    debug!(did = %did, old_mana = original_mana, new_mana = state.current_mana, regen_amount = regen_amount, "Mana regenerated");
                                }
                            } else {
                                trace!(did = %did, mana = original_mana, "Mana already at max or regen amount is zero.");
                            }
                        }
                        Ok(None) => {
                            warn!(did = %did, "ManaState not found for DID listed in all_dids during tick, skipping.");
                            // Optionally count this as a specific type of processing error if desired
                            // errors.push((did.clone(), "state_not_found_post_all_dids".to_string()));
                        }
                        Err(e) => {
                            errors.push((did.clone(), format!("read_failed: {}", e)));
                        }
                    }
                }
            }
            Err(e) => {
                // This error means we couldn't even get the list of DIDs to process.
                // It's a more fundamental issue with the tick operation itself.
                let policy_label = policy_to_label(&self.policy);
                MANA_REGENERATION_ERRORS_TOTAL
                    .with_label_values(&[policy_label, "all_dids_read_failed"])
                    .inc();
                return Err(anyhow::anyhow!(
                    "Failed to retrieve all DIDs from ledger for tick: {}",
                    e
                ));
            }
        }

        let details = RegenerationTickDetails {
            processed_dids_count: processed_dids_count_val,
            regenerated_dids_count,
            errors,
        };

        // Increment metrics based on collected details
        let policy_label = policy_to_label(&self.policy);

        MANA_REGENERATION_TICKS_TOTAL
            .with_label_values(&[policy_label])
            .inc();

        MANA_PROCESSED_DIDS_TOTAL
            .with_label_values(&[policy_label])
            .inc_by(details.processed_dids_count as u64);

        MANA_REGENERATED_DIDS_TOTAL
            .with_label_values(&[policy_label])
            .inc_by(details.regenerated_dids_count as u64);

        for (did, reason) in &details.errors {
            // Iterate over details.errors
            // Determine error scope for metrics from the reason string
            let error_scope = if reason.starts_with("read_failed") {
                "ledger_read"
            } else if reason.starts_with("update_failed") {
                "ledger_update"
            } else {
                "unknown" // Fallback for other types of errors if any
            };
            MANA_REGENERATION_ERRORS_TOTAL
                .with_label_values(&[policy_label, error_scope])
                .inc();
            // Original log for individual errors was here, handled by errors vector now.
            warn!(did = %did, error = %reason, "Error during mana regeneration for DID.");
        }

        Ok(details)
    }
}

// --- InMemoryManaLedger (for testing and simple scenarios) ---
#[derive(Default)]
pub struct InMemoryManaLedger {
    inner: RwLock<HashMap<Did, ManaState>>,
}

impl InMemoryManaLedger {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    // Helper for tests to set initial states easily
    pub async fn set_initial_state(&self, did: Did, state: ManaState) {
        self.inner.write().await.insert(did, state);
    }
}

#[async_trait]
impl ManaLedger for InMemoryManaLedger {
    async fn get_mana_state(&self, did: &Did) -> Result<Option<ManaState>> {
        Ok(self.inner.read().await.get(did).cloned())
    }

    async fn update_mana_state(&self, did: &Did, new_state: ManaState) -> Result<()> {
        self.inner.write().await.insert(did.clone(), new_state);
        Ok(())
    }

    async fn all_dids(&self) -> Result<Vec<Did>> {
        Ok(self.inner.read().await.keys().cloned().collect())
    }
}
