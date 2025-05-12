use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use icn_identity::ScopeKey;

pub type Did = String;

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
    pub fn available(&mut self) -> u64 {
        self.apply_regeneration();
        self.current
    }

    /// Attempt to consume the requested amount of mana. Returns Ok(()) if successful, otherwise Err.
    pub fn consume(&mut self, amount: u64) -> Result<(), ManaError> {
        self.apply_regeneration();
        if self.current >= amount {
            self.current -= amount;
            Ok(())
        } else {
            Err(ManaError::InsufficientMana { requested: amount, available: self.current })
        }
    }

    /// Adds regeneration based on time elapsed.
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
    pub fn credit(&mut self, amount: u64) {
        self.apply_regeneration();
        self.current = (self.current + amount).min(self.max);
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Manages mana pools for multiple DIDs/orgs.
#[derive(Default)]
pub struct ManaManager {
    pools: HashMap<ScopeKey, ManaPool>,
}

impl ManaManager {
    pub fn new() -> Self { Self { pools: HashMap::new() } }

    pub fn ensure_pool(&mut self, key: &ScopeKey, max: u64, regen_per_sec: u64) {
        self.pools.entry(key.clone()).or_insert_with(|| ManaPool::new(max, regen_per_sec));
    }

    /// Get mutable reference to a mana pool if it exists.
    pub fn pool_mut(&mut self, key: &ScopeKey) -> Option<&mut ManaPool> {
        self.pools.get_mut(key)
    }

    /// Get current available mana balance for the key after regeneration.
    pub fn balance(&mut self, key: &ScopeKey) -> Option<u64> {
        self.pools.get_mut(key).map(|p| p.available())
    }

    /// Spend the specified amount of mana from the key's pool.
    pub fn spend(&mut self, key: &ScopeKey, amount: u64) -> Result<(), ManaError> {
        match self.pools.get_mut(key) {
            Some(pool) => pool.consume(amount),
            None => Err(ManaError::InsufficientMana { requested: amount, available: 0 }),
        }
    }

    /// Atomically transfer mana between scopes.
    ///
    /// * `from` – scope to deduct from (must have sufficient balance).
    /// * `to`   – scope to credit.  If the target pool does not yet exist it will be
    ///           created with `max = amount` and `regen_per_sec = 1` (sane default).
    pub fn transfer(&mut self, from: &ScopeKey, to: &ScopeKey, amount: u64) -> Result<(), ManaError> {
        // First, attempt to deduct from the source (includes regeneration update).
        self.spend(from, amount)?;

        // Ensure the destination pool exists with at least `amount` max capacity.
        if !self.pools.contains_key(to) {
            self.ensure_pool(to, amount, 1);
        }

        // Safe: pool exists now.
        if let Some(pool) = self.pools.get_mut(to) {
            pool.credit(amount);
        }

        Ok(())
    }
} 