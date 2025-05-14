use crate::mana_metrics::*; // Added for metrics
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use icn_identity::Did;
use sled::Db;
use std::str::FromStr; // Added for Did::from_str
use std::sync::Arc; // May not be needed directly here, but often with sled
use tracing::{debug, error}; // Added for logging

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
        self.db
            .open_tree(MANA_STATE_TREE_NAME)
            .context("Failed to access mana_states tree in Sled database")
    }
}

#[async_trait]
impl ManaLedger for SledManaLedger {
    async fn get_mana_state(&self, did: &Did) -> Result<Option<ManaState>> {
        let tree_result = self.get_tree();
        if let Err(e) = tree_result {
            MANA_LEDGER_OPERATIONS_TOTAL
                .with_label_values(&["sled", "get_tree", "error"])
                .inc();
            MANA_LEDGER_ERRORS_TOTAL
                .with_label_values(&["sled", "get_tree", "io"])
                .inc();
            error!(%did, "Failed to get Sled tree for get_mana_state: {}", e);
            return Err(e); // Propagate error early if tree cannot be opened
        }
        let tree = tree_result.unwrap();

        let did_key_bytes = did.to_string().into_bytes();
        match tree.get(&did_key_bytes) {
            Ok(Some(ivec)) => {
                match bincode::deserialize::<ManaState>(&ivec) {
                    Ok(state) => {
                        MANA_LEDGER_OPERATIONS_TOTAL
                            .with_label_values(&["sled", "get", "success"])
                            .inc();
                        Ok(Some(state))
                    }
                    Err(e) => {
                        MANA_LEDGER_OPERATIONS_TOTAL
                            .with_label_values(&["sled", "get", "error"])
                            .inc();
                        MANA_LEDGER_ERRORS_TOTAL
                            .with_label_values(&["sled", "get", "deserialization"])
                            .inc();
                        error!(%did, error = %e, "Failed to deserialize ManaState from Sled");
                        // Return error instead of Ok(None) to indicate data corruption
                        Err(anyhow!(
                            "Failed to deserialize ManaState for DID {}: {}",
                            did,
                            e
                        ))
                    }
                }
            }
            Ok(None) => {
                MANA_LEDGER_OPERATIONS_TOTAL
                    .with_label_values(&["sled", "get", "success"])
                    .inc();
                Ok(None)
            }
            Err(e) => {
                MANA_LEDGER_OPERATIONS_TOTAL
                    .with_label_values(&["sled", "get", "error"])
                    .inc();
                MANA_LEDGER_ERRORS_TOTAL
                    .with_label_values(&["sled", "get", "io"])
                    .inc();
                error!(%did, error = %e, "Failed to get ManaState from Sled tree");
                Err(anyhow!("Sled tree I/O error for DID {}: {}", did, e))
            }
        }
    }

    async fn update_mana_state(&self, did: &Did, new_state: ManaState) -> Result<()> {
        let tree_result = self.get_tree();
        if let Err(e) = tree_result {
            MANA_LEDGER_OPERATIONS_TOTAL
                .with_label_values(&["sled", "get_tree", "error"])
                .inc();
            MANA_LEDGER_ERRORS_TOTAL
                .with_label_values(&["sled", "get_tree", "io"])
                .inc();
            error!(%did, "Failed to get Sled tree for update_mana_state: {}", e);
            return Err(e);
        }
        let tree = tree_result.unwrap();

        let did_key_bytes = did.to_string().into_bytes();
        match bincode::serialize(&new_state) {
            Ok(serialized_state) => {
                match tree.insert(&did_key_bytes, serialized_state) {
                    Ok(_) => {
                        MANA_LEDGER_OPERATIONS_TOTAL
                            .with_label_values(&["sled", "set", "success"])
                            .inc();
                        // Optional: Explicit flush for critical updates
                        // if let Err(e) = tree.flush_async().await {
                        //     MANA_LEDGER_ERRORS_TOTAL
                        //         .with_label_values(&["sled", "set_flush", "io"])
                        //         .inc();
                        //     error!(%did, error = %e, "Failed to flush Sled tree after mana update");
                        //     return Err(anyhow!("Failed to flush Sled tree for {}: {}", did, e));
                        // }
                        Ok(())
                    }
                    Err(e) => {
                        MANA_LEDGER_OPERATIONS_TOTAL
                            .with_label_values(&["sled", "set", "error"])
                            .inc();
                        MANA_LEDGER_ERRORS_TOTAL
                            .with_label_values(&["sled", "set", "io"])
                            .inc();
                        error!(%did, error = %e, "Failed to insert ManaState into Sled tree");
                        Err(anyhow!("Sled tree insert I/O error for DID {}: {}", did, e))
                    }
                }
            }
            Err(e) => {
                MANA_LEDGER_OPERATIONS_TOTAL
                    .with_label_values(&["sled", "set", "error"])
                    .inc();
                MANA_LEDGER_ERRORS_TOTAL
                    .with_label_values(&["sled", "set", "deserialization"])
                    .inc();
                error!(%did, error = %e, "Failed to serialize ManaState for Sled");
                Err(anyhow!(
                    "Serialization error for ManaState for DID {}: {}",
                    did,
                    e
                ))
            }
        }
    }

    async fn all_dids(&self) -> Result<Vec<Did>> {
        let tree_result = self.get_tree();
        if let Err(e) = tree_result {
            MANA_LEDGER_OPERATIONS_TOTAL
                .with_label_values(&["sled", "get_tree", "error"])
                .inc();
            MANA_LEDGER_ERRORS_TOTAL
                .with_label_values(&["sled", "get_tree", "io"])
                .inc();
            error!("Failed to get Sled tree for all_dids: {}", e);
            return Err(e);
        }
        let tree = tree_result.unwrap();

        let mut dids = Vec::new();
        let mut operation_successful = true; // Assume success initially

        for item_result in tree.iter() {
            match item_result {
                Ok((key_ivec, _value_ivec)) => {
                    match String::from_utf8(key_ivec.to_vec()) {
                        Ok(did_str) => {
                            match Did::from_str(&did_str) {
                                Ok(parsed_did) => dids.push(parsed_did),
                                Err(e) => {
                                    // This is an error in data format, not an I/O error for the overall operation
                                    MANA_LEDGER_ERRORS_TOTAL
                                        .with_label_values(&[
                                            "sled",
                                            "list_parse_did",
                                            "deserialization",
                                        ])
                                        .inc();
                                    error!(did_str = %did_str, error = %e, "Error parsing Did from Sled key");
                                    // Optionally, continue collecting other valid DIDs
                                }
                            }
                        }
                        Err(e) => {
                            MANA_LEDGER_ERRORS_TOTAL
                                .with_label_values(&["sled", "list_utf8_key", "deserialization"])
                                .inc();
                            error!(key = ?key_ivec, error = %e, "Sled key for mana state is not valid UTF-8");
                        }
                    }
                }
                Err(e) => {
                    // This is an I/O error during iteration
                    MANA_LEDGER_ERRORS_TOTAL
                        .with_label_values(&["sled", "list_iterate", "io"])
                        .inc();
                    error!(error = %e, "Sled tree iteration I/O error");
                    operation_successful = false; // Mark operation as failed
                                                  // Depending on desired behavior, we might stop or continue iteration
                                                  // For now, let's stop and report the error for the whole operation
                    return Err(anyhow!("Sled iteration failed: {}", e));
                }
            }
        }

        if operation_successful {
            MANA_LEDGER_OPERATIONS_TOTAL
                .with_label_values(&["sled", "list", "success"])
                .inc();
        } else {
            // If we didn't return early from an iteration error, but operation_successful is false
            // (e.g. if we decided to continue iterating despite errors), this path would be hit.
            // However, with current logic, an iteration error returns early.
            MANA_LEDGER_OPERATIONS_TOTAL
                .with_label_values(&["sled", "list", "error"])
                .inc();
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
        ledger
            .update_mana_state(&did1, initial_state.clone())
            .await?;

        let updated_mana_state = ManaState {
            current_mana: 75,
            max_mana: 100,
            regen_rate_per_epoch: 5,
            last_updated_epoch: 1, // Simulate an epoch update
        };
        ledger
            .update_mana_state(&did1, updated_mana_state.clone())
            .await?;

        let retrieved_state = ledger.get_mana_state(&did1).await?.unwrap();
        assert_eq!(retrieved_state.current_mana, 75);
        assert_eq!(retrieved_state.last_updated_epoch, 1);
        Ok(())
    }
}
