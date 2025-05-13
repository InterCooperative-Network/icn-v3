use anyhow::{Result, Context};
use async_trait::async_trait;
use icn_identity::Did;
use sled::Db;
use std::sync::Arc; // May not be needed directly here, but often with sled
use std::str::FromStr; // Added for Did::from_str

use crate::mana::{ManaLedger, ManaState};

const MANA_STATE_TREE_NAME: &str = "mana_states";

/// A ManaLedger implementation using Sled persistent storage.
#[derive(Clone)] // Clone is possible because sled::Db is Arc internally
pub struct SledManaLedger {
    db: Db,
}

impl SledManaLedger {
    /// Opens or creates a Sled database at the given path for the mana ledger.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let db = sled::open(path).context("Failed to open Sled database for mana ledger")?;
        // It's good practice to open the tree we'll be using to ensure it's registered
        // or to handle any potential errors specific to tree opening early.
        db.open_tree(MANA_STATE_TREE_NAME)
            .context("Failed to open mana_states tree in Sled database")?;
        Ok(Self { db })
    }

    // Helper to get the specific tree for mana states
    fn get_tree(&self) -> Result<sled::Tree> {
        self.db.open_tree(MANA_STATE_TREE_NAME)
            .context("Failed to access mana_states tree in Sled database")
    }
}

#[async_trait]
impl ManaLedger for SledManaLedger {
    async fn get_mana_state(&self, did: &Did) -> Result<Option<ManaState>> {
        let tree = self.get_tree()?;
        let did_key = did.to_string(); // Sled keys are typically &[u8]
        
        match tree.get(did_key.as_bytes())? {
            Some(ivec) => {
                // Deserialize ManaState from bytes (e.g., using bincode or serde_json)
                let mana_state: ManaState = bincode::deserialize(&ivec)
                    .context("Failed to deserialize ManaState from Sled")?;
                Ok(Some(mana_state))
            }
            None => Ok(None),
        }
    }

    async fn update_mana_state(&self, did: &Did, new_state: ManaState) -> Result<()> {
        let tree = self.get_tree()?;
        let did_key = did.to_string();
        
        // Serialize ManaState to bytes
        let serialized_state = bincode::serialize(&new_state)
            .context("Failed to serialize ManaState for Sled")?;
        
        tree.insert(did_key.as_bytes(), serialized_state)?;
        // It's good practice to flush, especially if immediate persistence is critical,
        // though Sled does auto-flush. For critical updates, explicit flush is safer.
        // tree.flush_async().await.context("Failed to flush Sled tree after mana update")?;
        Ok(())
    }

    async fn all_dids(&self) -> Result<Vec<Did>> {
        let tree = self.get_tree()?;
        let mut dids = Vec::new();
        for item_result in tree.iter() {
            let (key_ivec, _value_ivec) = item_result?;
            // Convert key from &[u8] back to Did String, then parse to Did if necessary
            // Assuming Did::from_str is available and appropriate
            match String::from_utf8(key_ivec.to_vec()) {
                Ok(did_str) => {
                    match Did::from_str(&did_str) {
                        Ok(parsed_did) => dids.push(parsed_did),
                        Err(e) => {
                            // Log error: failed to parse Did from key
                            eprintln!("Error parsing Did from Sled key '{}': {}", did_str, e);
                            // Optionally, skip this key or handle error differently
                        }
                    }
                }
                Err(e) => {
                    // Log error: key is not valid UTF-8
                    eprintln!("Sled key for mana state is not valid UTF-8: {:?}, error: {}", key_ivec, e);
                }
            }
        }
        Ok(dids)
    }
}

// Optional: Add basic unit tests for SledManaLedger here using a temporary sled DB.
#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::did::generate_did_key;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_sled_mana_ledger_set_get() -> Result<()> {
        let dir = tempdir()?;
        let ledger = SledManaLedger::open(dir.path())?;
        
        let did1 = generate_did_key().unwrap();
        let state1 = ManaState {
            current_mana: 100,
            max_mana: 200,
            regen_rate_per_epoch: 10,
            last_updated_epoch: 1,
        };

        ledger.update_mana_state(&did1, state1.clone()).await?;
        let retrieved_state = ledger.get_mana_state(&did1).await?;

        assert_eq!(retrieved_state, Some(state1));
        Ok(())
    }

    #[tokio::test]
    async fn test_sled_mana_ledger_get_non_existent() -> Result<()> {
        let dir = tempdir()?;
        let ledger = SledManaLedger::open(dir.path())?;
        let did_non_existent = generate_did_key().unwrap();

        let retrieved_state = ledger.get_mana_state(&did_non_existent).await?;
        assert!(retrieved_state.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_sled_mana_ledger_all_dids() -> Result<()> {
        let dir = tempdir()?;
        let ledger = SledManaLedger::open(dir.path())?;

        let did1 = generate_did_key().unwrap();
        let did2 = generate_did_key().unwrap();
        let state = ManaState::default(); // Assuming ManaState has a Default impl for simplicity

        ledger.update_mana_state(&did1, state.clone()).await?;
        ledger.update_mana_state(&did2, state.clone()).await?;

        let mut all_dids_retrieved = ledger.all_dids().await?;
        // Sort by string representation if Did does not implement Ord directly
        all_dids_retrieved.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
        
        let mut expected_dids = vec![did1.clone(), did2.clone()];
        // Sort by string representation
        expected_dids.sort_by(|a, b| a.to_string().cmp(&b.to_string()));

        assert_eq!(all_dids_retrieved, expected_dids);
        Ok(())
    }
    
    #[tokio::test]
    async fn test_sled_mana_ledger_update_existing() -> Result<()> {
        let dir = tempdir()?;
        let ledger = SledManaLedger::open(dir.path())?;
        let did1 = generate_did_key().unwrap();
        let initial_state = ManaState {
            current_mana: 50,
            max_mana: 100,
            regen_rate_per_epoch: 5,
            last_updated_epoch: 0,
        };
        ledger.update_mana_state(&did1, initial_state.clone()).await?;

        let updated_mana_state = ManaState {
            current_mana: 75,
            max_mana: 100,
            regen_rate_per_epoch: 5,
            last_updated_epoch: 1, // Simulate an epoch update
        };
        ledger.update_mana_state(&did1, updated_mana_state.clone()).await?;

        let retrieved_state = ledger.get_mana_state(&did1).await?.unwrap();
        assert_eq!(retrieved_state.current_mana, 75);
        assert_eq!(retrieved_state.last_updated_epoch, 1);
        Ok(())
    }
} 