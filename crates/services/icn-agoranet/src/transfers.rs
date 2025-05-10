use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::models::{EntityRef, Transfer, EntityType};

/// Error types for the transfer system
#[derive(Debug, Error)]
pub enum TransferError {
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
    
    #[error("internal error: {0}")]
    Internal(String),
}

/// Query parameters for fetching transfers
#[derive(Debug, Deserialize)]
pub struct TransferQuery {
    /// Federation ID to filter by
    pub federation_id: Option<String>,
    /// Entity ID to filter by (from or to)
    pub entity_id: Option<String>,
    /// Entity type to filter by
    pub entity_type: Option<EntityType>,
    /// Only include transfers where the entity is the source
    pub from_only: Option<bool>,
    /// Only include transfers where the entity is the destination
    pub to_only: Option<bool>,
    /// Start date for filtering
    pub start_date: Option<DateTime<Utc>>,
    /// End date for filtering
    pub end_date: Option<DateTime<Utc>>,
    /// Minimum amount to include
    pub min_amount: Option<u64>,
    /// Maximum amount to include
    pub max_amount: Option<u64>,
    /// Limit the number of results
    pub limit: Option<u32>,
    /// Offset for pagination
    pub offset: Option<u32>,
}

/// Ledger statistics
#[derive(Debug, Serialize)]
pub struct LedgerStats {
    /// Total number of transfers
    pub total_transfers: usize,
    /// Total volume transferred
    pub total_volume: u64,
    /// Total fees collected
    pub total_fees: u64,
    /// Total number of entities in the ledger
    pub total_entities: usize,
    /// Number of active entities (with non-zero balance)
    pub active_entities: usize,
    /// Entity with highest balance
    pub highest_balance_entity: Option<EntityRef>,
    /// Highest balance amount
    pub highest_balance: u64,
    /// Total transfers in the last 24 hours
    pub transfers_last_24h: usize,
    /// Volume in the last 24 hours
    pub volume_last_24h: u64,
}

/// Response for a batch transfer operation
#[derive(Debug, Serialize)]
pub struct BatchTransferResponse {
    /// Number of successful transfers
    pub successful: usize,
    /// Number of failed transfers
    pub failed: usize,
    /// IDs of successful transfers
    pub successful_ids: Vec<Uuid>,
    /// Failed transfers with error messages
    pub failed_transfers: Vec<(usize, String)>,
    /// Total amount transferred successfully
    pub total_transferred: u64,
    /// Total fees collected
    pub total_fees: u64,
}

/// In-memory ledger for balance tracking and transfer history
#[derive(Debug)]
pub struct Ledger {
    /// Entity balances by ID and entity type
    balances: HashMap<(String, EntityType), u64>,
    /// Historical transfers
    transfers: Vec<Transfer>,
    /// Federation balances (federation_id -> total balance)
    federation_stats: HashMap<String, u64>,
}

impl Ledger {
    /// Create a new empty ledger
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
            transfers: Vec::new(),
            federation_stats: HashMap::new(),
        }
    }
    
    /// Initialize with some example data
    pub fn with_example_data() -> Self {
        let mut ledger = Self::new();
        
        // Add some initial balances
        let entities = vec![
            (EntityRef { entity_type: EntityType::Federation, id: "federation1".to_string() }, 1_000_000),
            (EntityRef { entity_type: EntityType::Cooperative, id: "coop-econA".to_string() }, 250_000),
            (EntityRef { entity_type: EntityType::Cooperative, id: "coop-econB".to_string() }, 150_000),
            (EntityRef { entity_type: EntityType::Community, id: "comm-govX".to_string() }, 50_000),
            (EntityRef { entity_type: EntityType::Community, id: "comm-govY".to_string() }, 25_000),
            (EntityRef { entity_type: EntityType::User, id: "did:icn:user1".to_string() }, 5_000),
            (EntityRef { entity_type: EntityType::User, id: "did:icn:user2".to_string() }, 3_000),
        ];
        
        for (entity, balance) in entities {
            ledger.set_balance(&entity, balance);
            
            // Update federation stats
            if entity.entity_type == EntityType::Federation {
                ledger.federation_stats.insert(entity.id.clone(), balance);
            } else {
                // Assume all entities belong to federation1 for this example
                let fed_entry = ledger.federation_stats.entry("federation1".to_string()).or_insert(0);
                *fed_entry += balance;
            }
        }
        
        ledger
    }
    
    /// Get an entity's balance
    pub fn get_balance(&self, entity: &EntityRef) -> u64 {
        self.balances.get(&(entity.id.clone(), entity.entity_type.clone()))
            .copied()
            .unwrap_or(0)
    }
    
    /// Set an entity's balance directly
    pub fn set_balance(&mut self, entity: &EntityRef, balance: u64) {
        self.balances.insert((entity.id.clone(), entity.entity_type.clone()), balance);
    }
    
    /// Process a transfer between entities
    pub fn process_transfer(&mut self, transfer: Transfer) -> Result<Transfer, TransferError> {
        // Validate the transfer
        if transfer.amount == 0 {
            return Err(TransferError::InvalidAmount);
        }
        
        // Check if source has sufficient balance
        let from_balance = self.get_balance(&transfer.from);
        if from_balance < transfer.amount + transfer.fee {
            return Err(TransferError::InsufficientBalance);
        }
        
        // Update balances
        let new_from_balance = from_balance - transfer.amount - transfer.fee;
        self.set_balance(&transfer.from, new_from_balance);
        
        let to_balance = self.get_balance(&transfer.to);
        let new_to_balance = to_balance + transfer.amount;
        self.set_balance(&transfer.to, new_to_balance);
        
        // Record the transfer
        self.transfers.push(transfer.clone());
        
        // Update federation stats
        if let Some(stats) = self.federation_stats.get_mut(&transfer.federation_id) {
            // Fees remain in the federation as a whole
            *stats += transfer.fee;
        }
        
        Ok(transfer)
    }
    
    /// Process multiple transfers in one operation
    pub fn process_batch_transfer(
        &mut self, 
        transfers: Vec<Transfer>
    ) -> BatchTransferResponse {
        let mut response = BatchTransferResponse {
            successful: 0,
            failed: 0,
            successful_ids: Vec::new(),
            failed_transfers: Vec::new(),
            total_transferred: 0,
            total_fees: 0,
        };
        
        for (index, transfer) in transfers.into_iter().enumerate() {
            match self.process_transfer(transfer) {
                Ok(processed) => {
                    response.successful += 1;
                    response.successful_ids.push(processed.tx_id);
                    response.total_transferred += processed.amount;
                    response.total_fees += processed.fee;
                },
                Err(err) => {
                    response.failed += 1;
                    response.failed_transfers.push((index, err.to_string()));
                }
            }
        }
        
        response
    }
    
    /// Find a transfer by ID
    pub fn find_transfer(&self, tx_id: &Uuid) -> Option<&Transfer> {
        self.transfers.iter().find(|t| &t.tx_id == tx_id)
    }
    
    /// Query transfers based on filters
    pub fn query_transfers(&self, query: &TransferQuery) -> Vec<&Transfer> {
        let mut results: Vec<&Transfer> = self.transfers.iter()
            .filter(|t| {
                // Filter by federation
                if let Some(fed_id) = &query.federation_id {
                    if t.federation_id != *fed_id {
                        return false;
                    }
                }
                
                // Filter by entity
                if let Some(entity_id) = &query.entity_id {
                    let from_match = t.from.id == *entity_id;
                    let to_match = t.to.id == *entity_id;
                    
                    match (query.from_only, query.to_only) {
                        (Some(true), _) => if !from_match { return false; },
                        (_, Some(true)) => if !to_match { return false; },
                        _ => if !from_match && !to_match { return false; }
                    }
                    
                    // Filter by entity type if both ID and type provided
                    if let Some(entity_type) = &query.entity_type {
                        if (from_match && t.from.entity_type != *entity_type) ||
                           (to_match && t.to.entity_type != *entity_type) {
                            return false;
                        }
                    }
                } 
                // If only entity type provided without ID
                else if let Some(entity_type) = &query.entity_type {
                    if t.from.entity_type != *entity_type && t.to.entity_type != *entity_type {
                        return false;
                    }
                }
                
                // Filter by date range
                if let Some(start) = query.start_date {
                    if t.timestamp < start {
                        return false;
                    }
                }
                
                if let Some(end) = query.end_date {
                    if t.timestamp > end {
                        return false;
                    }
                }
                
                // Filter by amount
                if let Some(min) = query.min_amount {
                    if t.amount < min {
                        return false;
                    }
                }
                
                if let Some(max) = query.max_amount {
                    if t.amount > max {
                        return false;
                    }
                }
                
                true
            })
            .collect();
        
        // Apply sorting - newest first
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        // Apply pagination
        if let Some(offset) = query.offset {
            let offset = offset as usize;
            if offset < results.len() {
                results = results.into_iter().skip(offset).collect();
            } else {
                results = Vec::new();
            }
        }
        
        if let Some(limit) = query.limit {
            let limit = limit as usize;
            if results.len() > limit {
                results.truncate(limit);
            }
        }
        
        results
    }
    
    /// Get ledger statistics
    pub fn get_stats(&self) -> LedgerStats {
        // Calculate total volume and fees
        let (total_volume, total_fees) = self.transfers.iter()
            .fold((0, 0), |(vol, fees), t| (vol + t.amount, fees + t.fee));
        
        // Find active entities
        let active_entities = self.balances.values().filter(|&b| *b > 0).count();
        
        // Find entity with highest balance
        let highest_balance_entry = self.balances.iter()
            .max_by_key(|(_, balance)| *balance);
        
        let (highest_balance_entity, highest_balance) = match highest_balance_entry {
            Some(((id, entity_type), balance)) => {
                let entity = EntityRef {
                    entity_type: entity_type.clone(),
                    id: id.clone(),
                };
                (Some(entity), *balance)
            },
            None => (None, 0),
        };
        
        // Calculate activity in the last 24 hours
        let day_ago = Utc::now() - chrono::Duration::days(1);
        let recent_transfers: Vec<_> = self.transfers.iter()
            .filter(|t| t.timestamp > day_ago)
            .collect();
        
        let transfers_last_24h = recent_transfers.len();
        let volume_last_24h = recent_transfers.iter()
            .fold(0, |sum, t| sum + t.amount);
        
        LedgerStats {
            total_transfers: self.transfers.len(),
            total_volume,
            total_fees,
            total_entities: self.balances.len(),
            active_entities,
            highest_balance_entity,
            highest_balance,
            transfers_last_24h,
            volume_last_24h,
        }
    }
    
    /// Get federation-specific statistics
    pub fn get_federation_stats(&self, federation_id: &str) -> Option<LedgerStats> {
        // Check if federation exists
        if !self.federation_stats.contains_key(federation_id) {
            return None;
        }
        
        // Filter transfers for this federation
        let fed_transfers: Vec<_> = self.transfers.iter()
            .filter(|t| t.federation_id == federation_id)
            .collect();
        
        // Calculate total volume and fees
        let (total_volume, total_fees) = fed_transfers.iter()
            .fold((0, 0), |(vol, fees), t| (vol + t.amount, fees + t.fee));
        
        // Filter active entities in this federation
        let fed_entities: Vec<_> = self.balances.iter()
            .filter(|((id, _), balance)| {
                // For simplicity, we're assuming entities with balance belong to the federation
                // In a real implementation, we'd have explicit federation membership
                **balance > 0
            })
            .collect();
        
        let active_entities = fed_entities.len();
        
        // Find entity with highest balance
        let highest_balance_entry = fed_entities.into_iter()
            .max_by_key(|(_, balance)| *balance);
        
        let (highest_balance_entity, highest_balance) = match highest_balance_entry {
            Some(((id, entity_type), balance)) => {
                let entity = EntityRef {
                    entity_type: entity_type.clone(),
                    id: id.clone(),
                };
                (Some(entity), *balance)
            },
            None => (None, 0),
        };
        
        // Calculate activity in the last 24 hours
        let day_ago = Utc::now() - chrono::Duration::days(1);
        let recent_transfers: Vec<_> = fed_transfers.iter()
            .filter(|t| t.timestamp > day_ago)
            .collect();
        
        let transfers_last_24h = recent_transfers.len();
        let volume_last_24h = recent_transfers.iter()
            .fold(0, |sum, t| sum + t.amount);
        
        Some(LedgerStats {
            total_transfers: fed_transfers.len(),
            total_volume,
            total_fees,
            total_entities: self.balances.len(), // Simplifying for now
            active_entities,
            highest_balance_entity,
            highest_balance,
            transfers_last_24h,
            volume_last_24h,
        })
    }
}

/// Thread-safe ledger with read-write locking
pub type LedgerStore = Arc<RwLock<Ledger>>;

/// Create a new ledger store with example data
pub fn create_example_ledger() -> LedgerStore {
    Arc::new(RwLock::new(Ledger::with_example_data()))
} 