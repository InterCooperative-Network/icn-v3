use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Represents the state of mana for an entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManaState {
    /// Current available mana.
    pub current_mana: u64,
    /// Maximum mana capacity.
    pub max_mana: u64,
    /// Mana points regenerated per epoch (e.g., per hour or per day).
    /// The definition of an "epoch" will need to be standardized across the system.
    pub regen_rate_per_epoch: f64,
    /// The timestamp or epoch number when mana was last updated.
    /// Used to calculate current mana considering regeneration.
    pub last_updated_epoch: u64,
}

impl Default for ManaState {
    fn default() -> Self {
        Self {
            current_mana: 0,
            max_mana: 1000,             // Default max mana, can be configured
            regen_rate_per_epoch: 10.0, // Default regen rate
            last_updated_epoch: 0,      // Default epoch
        }
    }
}

/// Associates ManaState with a specific executor and optionally a cooperative.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScopedMana {
    /// The DID of the executor whose mana this represents.
    pub executor_did: Did,
    /// Optional DID of the cooperative to which this mana scope is primarily associated.
    /// Mana regeneration or limits might be influenced by cooperative policies.
    pub cooperative_did: Option<Did>,
    /// The actual mana state.
    pub state: ManaState,
}

impl ScopedMana {
    /// Creates a new ScopedMana for an executor, optionally tied to a cooperative.
    pub fn new(
        executor_did: Did,
        cooperative_did: Option<Did>,
        initial_mana: u64,
        max_mana: u64,
        regen_rate: f64,
        current_epoch: u64,
    ) -> Self {
        Self {
            executor_did,
            cooperative_did,
            state: ManaState {
                current_mana: initial_mana,
                max_mana,
                regen_rate_per_epoch: regen_rate,
                last_updated_epoch: current_epoch,
            },
        }
    }
}
