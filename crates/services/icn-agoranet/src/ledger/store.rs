use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::collections::HashMap;

use crate::models::{EntityRef, Transfer, TransferRequest};

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("insufficient balance")]
    InsufficientBalance,
    
    #[error("invalid amount")]
    InvalidAmount,
    
    #[error("entity not found: {0}")]
    EntityNotFound(String),
    
    #[error("transfer not found: {0}")]
    TransferNotFound(Uuid),
    
    #[error("federation mismatch")]
    FederationMismatch,
    
    #[error("database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TransferQuery {
    pub federation_id: Option<String>,
    pub entity_id: Option<String>,
    pub entity_type: Option<String>,
    pub from_only: Option<bool>,
    pub to_only: Option<bool>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub min_amount: Option<u64>,
    pub max_amount: Option<u64>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, serde::Serialize)]
pub struct LedgerStats {
    pub total_transfers: usize,
    pub total_volume: u64,
    pub total_fees: u64,
    pub total_entities: usize,
    pub active_entities: usize,
    pub highest_balance_entity: Option<EntityRef>,
    pub highest_balance: u64,
    pub transfers_last_24h: usize,
    pub volume_last_24h: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct BatchTransferResponse {
    pub successful: usize,
    pub failed: usize,
    pub successful_ids: Vec<Uuid>,
    pub failed_transfers: Vec<(usize, String)>,
    pub total_transferred: u64,
    pub total_fees: u64,
}

#[async_trait]
pub trait LedgerStore: Send + Sync {
    /// Get the current balance for an entity
    async fn get_balance(&self, entity: &EntityRef) -> Result<u64, LedgerError>;

    /// Process a single transfer, updating balances atomically
    async fn process_transfer(&self, transfer: Transfer) -> Result<Transfer, LedgerError>;
    
    /// Process multiple transfers as a batch
    async fn process_batch_transfer(&self, transfers: Vec<Transfer>) -> Result<BatchTransferResponse, LedgerError>;
    
    /// Find a transfer by ID
    async fn find_transfer(&self, tx_id: &Uuid) -> Result<Option<Transfer>, LedgerError>;
    
    /// Query transfers based on filters
    async fn query_transfers(&self, query: &TransferQuery) -> Result<Vec<Transfer>, LedgerError>;
    
    /// Get ledger statistics
    async fn get_stats(&self) -> Result<LedgerStats, LedgerError>;
    
    /// Get federation-specific statistics
    async fn get_federation_stats(&self, federation_id: &str) -> Result<Option<LedgerStats>, LedgerError>;
    
    /// Create a transfer from a request
    async fn create_transfer(
        &self,
        request: &TransferRequest,
        federation_id: String,
        initiator: String,
        fee: u64,
    ) -> Result<Transfer, LedgerError>;
    
    /// Ensure an entity exists in the ledger
    async fn ensure_entity_exists(&self, entity: &EntityRef, federation_id: &str) -> Result<(), LedgerError>;
} 