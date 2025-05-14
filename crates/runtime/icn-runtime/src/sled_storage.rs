use crate::RuntimeStorage;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use bincode;
use sled::{Db, IVec};
use std::path::Path;
use std::sync::Arc;

// Import necessary types used by the trait methods
use crate::{MeshExecutionReceipt, Proposal}; // Using crate::Proposal now
use icn_types::runtime_receipt::RuntimeExecutionReceipt;

/// A persistent storage backend using Sled embedded database.
pub struct SledStorage {
    db: Db,
}

impl SledStorage {
    /// Opens or creates a Sled database at the specified path.
    pub fn open(path: &Path) -> Result<Self> {
        tracing::info!("Opening Sled database at: {:?}", path);
        let db = sled::open(path).context(format!("Failed to open sled database at {:?}", path))?;
        Ok(Self { db })
    }

    // Helper to generate keys with prefixes
    fn wasm_key(cid: &str) -> String {
        format!("wasm:{}", cid)
    }

    fn receipt_key(cid: &str) -> String {
        format!("receipt:{}", cid)
    }

    fn proposal_key(id: &str) -> String {
        format!("proposal:{}", id)
    }
}

#[async_trait]
impl RuntimeStorage for SledStorage {
    // --- WASM Storage ---
    async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
        let key = Self::wasm_key(cid);
        tracing::debug!(key = %key, "Loading WASM");
        let ivec = self
            .db
            .get(&key)?
            .ok_or_else(|| anyhow!("WASM not found for CID {} (key: {})", cid, key))?;
        Ok(ivec.to_vec())
    }

    // Note: store_wasm wasn't in the original trait, but needed by tests. Adding it.
    // If RuntimeStorage shouldn't have this, tests need adjustment.
    async fn store_wasm(&self, cid: &str, bytes: &[u8]) -> Result<()> {
        let key = Self::wasm_key(cid);
        tracing::debug!(key = %key, bytes = bytes.len(), "Storing WASM");
        self.db.insert(key, bytes)?;
        // Consider flushing explicitly if immediate durability is critical
        // self.db.flush_async().await?;
        Ok(())
    }

    // --- Receipt Storage ---
    // Updated to take RuntimeExecutionReceipt as per anchor_mesh_receipt logic
    async fn store_receipt(&self, receipt: &RuntimeExecutionReceipt) -> Result<String> {
        // Use the receipt's internal ID or generate one if missing (though RuntimeExecutionReceipt should have `id`)
        let receipt_id = &receipt.id;
        let key = Self::receipt_key(receipt_id);
        tracing::debug!(key = %key, "Storing Receipt");
        let data = bincode::serialize(receipt).context("Failed to serialize receipt")?;
        self.db.insert(&key, data)?;
        // self.db.flush_async().await?;
        Ok(receipt_id.clone()) // Return the ID used as the key
    }

    // Added load_receipt for completeness, though not strictly in the current trait usage
    async fn load_receipt(&self, receipt_id: &str) -> Result<RuntimeExecutionReceipt> {
        let key = Self::receipt_key(receipt_id);
        tracing::debug!(key = %key, "Loading Receipt");
        let ivec = self
            .db
            .get(&key)?
            .ok_or_else(|| anyhow!("Receipt not found for ID {} (key: {})", receipt_id, key))?;
        let receipt = bincode::deserialize::<RuntimeExecutionReceipt>(&ivec)
            .context("Failed to deserialize receipt")?;
        Ok(receipt)
    }

    // --- Proposal Storage (Stubs - Requires Implementation) ---
    async fn load_proposal(&self, id: &str) -> Result<Proposal> {
        let key = Self::proposal_key(id);
        tracing::debug!(key = %key, "Loading proposal");
        let val = self
            .db
            .get(&key)?
            .ok_or_else(|| anyhow::anyhow!("Proposal {} not found (key: {})", id, key))?;
        let proposal =
            bincode::deserialize::<Proposal>(&val).context("Failed to deserialize proposal")?;
        Ok(proposal)
    }

    async fn update_proposal(&self, proposal: &Proposal) -> Result<()> {
        let key = Self::proposal_key(&proposal.id);
        tracing::debug!(key = %key, "Updating proposal");
        let data = bincode::serialize(proposal).context("Failed to serialize proposal")?;
        self.db.insert(key, data)?;
        // Consider flushing explicitly if immediate durability is critical
        // self.db.flush_async().await?;
        Ok(())
    }

    // --- DAG Anchoring (Stub - Belongs elsewhere) ---
    async fn anchor_to_dag(&self, _cid: &str) -> Result<String> {
        tracing::error!("anchor_to_dag called on SledStorage - this is not a DAG store!");
        // This method doesn't make sense for Sled itself. Anchoring should interact
        // with a separate DAG component (which might *use* Sled internally).
        Err(anyhow!("SledStorage does not support direct DAG anchoring"))
    }
}
