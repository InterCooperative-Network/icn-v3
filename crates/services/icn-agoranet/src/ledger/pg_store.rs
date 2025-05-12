use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use sqlx::{PgPool, Postgres, Transaction};
use sqlx::postgres::PgQueryResult;
use uuid::Uuid;
use std::sync::Arc;
use sqlx::Row;

use crate::models::{EntityRef, EntityType, Transfer, TransferRequest};
use super::store::{LedgerStore, LedgerError, TransferQuery, LedgerStats, BatchTransferResponse};

/// PostgreSQL implementation of the LedgerStore trait
#[derive(Clone, Debug)]
pub struct PostgresLedgerStore {
    pool: PgPool,
}

impl PostgresLedgerStore {
    /// Create a new PostgreSQL ledger store with the given connection pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Begin a new transaction
    async fn begin_tx(&self) -> Result<Transaction<'_, Postgres>, LedgerError> {
        self.pool.begin().await.map_err(LedgerError::DatabaseError)
    }

    /// Ensure an entity exists within a transaction
    async fn ensure_entity_exists_in_tx(
        &self, 
        tx: &mut Transaction<'_, Postgres>,
        entity: &EntityRef, 
        federation_id: &str
    ) -> Result<(), LedgerError> {
        // Check if entity exists
        let exists = sqlx::query!(
            r#"
            SELECT 1
            FROM entities
            WHERE entity_type = $1 AND entity_id = $2
            "#,
            entity.entity_type.to_string(),
            entity.id
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(LedgerError::DatabaseError)?
        .is_some();

        if !exists {
            // Create entity
            sqlx::query!(
                r#"
                INSERT INTO entities (entity_type, entity_id, federation_id)
                VALUES ($1, $2, $3)
                "#,
                entity.entity_type.to_string(),
                entity.id,
                federation_id
            )
            .execute(&mut **tx)
            .await
            .map_err(LedgerError::DatabaseError)?;

            // Initialize balance
            sqlx::query!(
                r#"
                INSERT INTO balances (entity_type, entity_id, balance)
                VALUES ($1, $2, 0)
                "#,
                entity.entity_type.to_string(),
                entity.id
            )
            .execute(&mut **tx)
            .await
            .map_err(LedgerError::DatabaseError)?;
        }

        Ok(())
    }

    /// Update federation statistics after a transfer
    async fn update_federation_stats(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        federation_id: &str,
        amount: u64,
        fee: u64
    ) -> Result<(), LedgerError> {
        sqlx::query!(
            r#"
            INSERT INTO federation_stats (
                federation_id, total_transfers, total_volume, total_fees
            ) VALUES (
                $1, 1, $2, $3
            )
            ON CONFLICT (federation_id) DO UPDATE SET
                total_transfers = federation_stats.total_transfers + 1,
                total_volume = federation_stats.total_volume + $2,
                total_fees = federation_stats.total_fees + $3,
                last_updated = NOW()
            "#,
            federation_id,
            amount as i64,
            fee as i64
        )
        .execute(&mut **tx)
        .await
        .map_err(LedgerError::DatabaseError)?;

        Ok(())
    }
}

#[async_trait]
impl LedgerStore for PostgresLedgerStore {
    async fn get_balance(&self, entity: &EntityRef) -> Result<u64, LedgerError> {
        let result = sqlx::query!(
            r#"
            SELECT balance FROM balances
            WHERE entity_type = $1 AND entity_id = $2
            "#,
            entity.entity_type.to_string(),
            entity.id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?;

        match result {
            Some(row) => Ok(row.balance as u64),
            None => Ok(0) // Return 0 if entity doesn't exist yet
        }
    }

    async fn process_transfer(&self, transfer: Transfer) -> Result<Transfer, LedgerError> {
        let mut tx = self.begin_tx().await?;

        // Ensure entities exist
        self.ensure_entity_exists_in_tx(&mut tx, &transfer.from, &transfer.federation_id).await?;
        self.ensure_entity_exists_in_tx(&mut tx, &transfer.to, &transfer.federation_id).await?;

        // Get current balance
        let from_balance = sqlx::query!(
            r#"
            SELECT balance FROM balances
            WHERE entity_type = $1 AND entity_id = $2
            FOR UPDATE
            "#,
            transfer.from.entity_type.to_string(),
            transfer.from.id
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(LedgerError::DatabaseError)?
        .balance as u64;

        let total_deduction = transfer.amount + transfer.fee;
        if from_balance < total_deduction {
            return Err(LedgerError::InsufficientBalance);
        }

        // Insert transfer record
        sqlx::query!(
            r#"
            INSERT INTO transfers (
                tx_id, federation_id, from_type, from_id, to_type, to_id,
                amount, fee, initiator, timestamp, memo, metadata
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12
            )
            "#,
            transfer.tx_id,
            transfer.federation_id,
            transfer.from.entity_type.to_string(),
            transfer.from.id,
            transfer.to.entity_type.to_string(),
            transfer.to.id,
            transfer.amount as i64,
            transfer.fee as i64,
            transfer.initiator,
            transfer.timestamp,
            transfer.memo,
            serde_json::Value::Object(serde_json::Map::new()) // Empty JSON object for now
        )
        .execute(&mut *tx)
        .await
        .map_err(LedgerError::DatabaseError)?;

        // Update balances
        sqlx::query!(
            r#"
            UPDATE balances
            SET balance = balance - $3, last_updated = NOW()
            WHERE entity_type = $1 AND entity_id = $2
            "#,
            transfer.from.entity_type.to_string(),
            transfer.from.id,
            total_deduction as i64
        )
        .execute(&mut *tx)
        .await
        .map_err(LedgerError::DatabaseError)?;

        sqlx::query!(
            r#"
            UPDATE balances
            SET balance = balance + $3, last_updated = NOW()
            WHERE entity_type = $1 AND entity_id = $2
            "#,
            transfer.to.entity_type.to_string(),
            transfer.to.id,
            transfer.amount as i64
        )
        .execute(&mut *tx)
        .await
        .map_err(LedgerError::DatabaseError)?;

        // Update federation stats
        self.update_federation_stats(&mut tx, &transfer.federation_id, transfer.amount, transfer.fee).await?;

        // Commit transaction
        tx.commit().await.map_err(LedgerError::DatabaseError)?;

        Ok(transfer)
    }

    async fn process_batch_transfer(&self, transfers: Vec<Transfer>) -> Result<BatchTransferResponse, LedgerError> {
        if transfers.is_empty() {
            return Ok(BatchTransferResponse {
                successful: 0,
                failed: 0,
                successful_ids: Vec::new(),
                failed_transfers: Vec::new(),
                total_transferred: 0,
                total_fees: 0,
            });
        }

        let mut successful = 0;
        let mut failed = 0;
        let mut successful_ids = Vec::new();
        let mut failed_transfers = Vec::new();
        let mut total_transferred = 0;
        let mut total_fees = 0;

        for (idx, transfer) in transfers.iter().enumerate() {
            match self.process_transfer(transfer.clone()).await {
                Ok(processed) => {
                    successful += 1;
                    successful_ids.push(processed.tx_id);
                    total_transferred += processed.amount;
                    total_fees += processed.fee;
                },
                Err(e) => {
                    failed += 1;
                    failed_transfers.push((idx, e.to_string()));
                }
            }
        }

        Ok(BatchTransferResponse {
            successful,
            failed,
            successful_ids,
            failed_transfers,
            total_transferred,
            total_fees,
        })
    }

    async fn find_transfer(&self, tx_id: &Uuid) -> Result<Option<Transfer>, LedgerError> {
        let result = sqlx::query!(
            r#"
            SELECT 
                tx_id, federation_id, from_type, from_id, to_type, to_id,
                amount, fee, initiator, timestamp, memo
            FROM transfers
            WHERE tx_id = $1
            "#,
            tx_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?;

        match result {
            Some(row) => {
                let transfer = Transfer {
                    tx_id: row.tx_id,
                    federation_id: row.federation_id,
                    from: EntityRef {
                        entity_type: entity_type_from_string(&row.from_type)?,
                        id: row.from_id,
                    },
                    to: EntityRef {
                        entity_type: entity_type_from_string(&row.to_type)?,
                        id: row.to_id,
                    },
                    amount: row.amount as u64,
                    fee: row.fee as u64,
                    initiator: row.initiator,
                    timestamp: row.timestamp,
                    memo: row.memo,
                    metadata: None,
                };
                Ok(Some(transfer))
            },
            None => Ok(None)
        }
    }

    async fn query_transfers(&self, query: &TransferQuery) -> Result<Vec<Transfer>, LedgerError> {
        // Build the query dynamically based on filters
        let mut sql = String::from(
            "SELECT 
                tx_id, federation_id, from_type, from_id, to_type, to_id,
                amount, fee, initiator, timestamp, memo
            FROM transfers
            WHERE 1=1"
        );

        let mut params: Vec<String> = Vec::new();
        let mut param_values: Vec<Box<dyn sqlx::Type<Postgres> + Send + Sync>> = Vec::new();
        let mut param_idx = 1;

        // Apply filters
        if let Some(fed_id) = &query.federation_id {
            sql.push_str(&format!(" AND federation_id = ${}", param_idx));
            params.push(fed_id.clone());
            param_values.push(Box::new(fed_id.clone()));
            param_idx += 1;
        }

        if let Some(entity_id) = &query.entity_id {
            if let Some(true) = query.from_only {
                sql.push_str(&format!(" AND from_id = ${}", param_idx));
                params.push(entity_id.clone());
                param_values.push(Box::new(entity_id.clone()));
                param_idx += 1;
            } else if let Some(true) = query.to_only {
                sql.push_str(&format!(" AND to_id = ${}", param_idx));
                params.push(entity_id.clone());
                param_values.push(Box::new(entity_id.clone()));
                param_idx += 1;
            } else {
                sql.push_str(&format!(" AND (from_id = ${0} OR to_id = ${0})", param_idx));
                params.push(entity_id.clone());
                param_values.push(Box::new(entity_id.clone()));
                param_idx += 1;
            }
        }

        if let Some(entity_type) = &query.entity_type {
            if let Some(true) = query.from_only {
                sql.push_str(&format!(" AND from_type = ${}", param_idx));
                params.push(entity_type.clone());
                param_values.push(Box::new(entity_type.clone()));
                param_idx += 1;
            } else if let Some(true) = query.to_only {
                sql.push_str(&format!(" AND to_type = ${}", param_idx));
                params.push(entity_type.clone());
                param_values.push(Box::new(entity_type.clone()));
                param_idx += 1;
            } else {
                sql.push_str(&format!(" AND (from_type = ${0} OR to_type = ${0})", param_idx));
                params.push(entity_type.clone());
                param_values.push(Box::new(entity_type.clone()));
                param_idx += 1;
            }
        }

        if let Some(start_date) = &query.start_date {
            sql.push_str(&format!(" AND timestamp >= ${}", param_idx));
            params.push(start_date.to_string());
            param_values.push(Box::new(*start_date));
            param_idx += 1;
        }

        if let Some(end_date) = &query.end_date {
            sql.push_str(&format!(" AND timestamp <= ${}", param_idx));
            params.push(end_date.to_string());
            param_values.push(Box::new(*end_date));
            param_idx += 1;
        }

        if let Some(min_amount) = &query.min_amount {
            sql.push_str(&format!(" AND amount >= ${}", param_idx));
            params.push(min_amount.to_string());
            param_values.push(Box::new(*min_amount as i64));
            param_idx += 1;
        }

        if let Some(max_amount) = &query.max_amount {
            sql.push_str(&format!(" AND amount <= ${}", param_idx));
            params.push(max_amount.to_string());
            param_values.push(Box::new(*max_amount as i64));
            param_idx += 1;
        }

        // Order by timestamp descending (newest first)
        sql.push_str(" ORDER BY timestamp DESC");

        // Apply pagination
        if let Some(limit) = &query.limit {
            sql.push_str(&format!(" LIMIT ${}", param_idx));
            params.push(limit.to_string());
            param_values.push(Box::new(*limit as i64));
            param_idx += 1;
        }

        if let Some(offset) = &query.offset {
            sql.push_str(&format!(" OFFSET ${}", param_idx));
            params.push(offset.to_string());
            param_values.push(Box::new(*offset as i64));
        }

        // Handle query building in a simpler way for this implementation
        // This is a simplified approach, a proper implementation would use sqlx's query builder
        let mut transfers = Vec::new();
        let query_result = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(LedgerError::DatabaseError)?;

        for row in query_result {
            let tx_id: Uuid = row.get("tx_id");
            let federation_id: String = row.get("federation_id");
            let from_type: String = row.get("from_type");
            let from_id: String = row.get("from_id");
            let to_type: String = row.get("to_type");
            let to_id: String = row.get("to_id");
            let amount: i64 = row.get("amount");
            let fee: i64 = row.get("fee");
            let initiator: String = row.get("initiator");
            let timestamp: DateTime<Utc> = row.get("timestamp");
            let memo: Option<String> = row.get("memo");

            transfers.push(Transfer {
                tx_id,
                federation_id,
                from: EntityRef {
                    entity_type: entity_type_from_string(&from_type)?,
                    id: from_id,
                },
                to: EntityRef {
                    entity_type: entity_type_from_string(&to_type)?,
                    id: to_id,
                },
                amount: amount as u64,
                fee: fee as u64,
                initiator,
                timestamp,
                memo,
                metadata: None,
            });
        }

        Ok(transfers)
    }

    async fn get_stats(&self) -> Result<LedgerStats, LedgerError> {
        let total_transfers = sqlx::query!(
            "SELECT COUNT(*) as count FROM transfers"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?
        .count
        .unwrap_or(0) as usize;

        let volume_fees = sqlx::query!(
            "SELECT SUM(amount) as total_volume, SUM(fee) as total_fees FROM transfers"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?;

        let total_volume = volume_fees.total_volume.unwrap_or(0) as u64;
        let total_fees = volume_fees.total_fees.unwrap_or(0) as u64;

        let entity_counts = sqlx::query!(
            "SELECT COUNT(*) as total, COUNT(*) FILTER (WHERE balance > 0) as active FROM balances"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?;

        let total_entities = entity_counts.total.unwrap_or(0) as usize;
        let active_entities = entity_counts.active.unwrap_or(0) as usize;

        let highest_balance_entity = sqlx::query!(
            r#"
            SELECT entity_type, entity_id, balance
            FROM balances
            ORDER BY balance DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?;

        let (highest_balance_entity, highest_balance) = match highest_balance_entity {
            Some(row) => {
                let entity = EntityRef {
                    entity_type: entity_type_from_string(&row.entity_type)?,
                    id: row.entity_id,
                };
                (Some(entity), row.balance as u64)
            },
            None => (None, 0)
        };

        let transfers_last_24h = sqlx::query!(
            "SELECT COUNT(*) as count FROM transfers WHERE timestamp > NOW() - INTERVAL '24 hours'"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?
        .count
        .unwrap_or(0) as usize;

        let volume_last_24h = sqlx::query!(
            "SELECT SUM(amount) as sum FROM transfers WHERE timestamp > NOW() - INTERVAL '24 hours'"
        )
        .fetch_one(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?
        .sum
        .unwrap_or(0) as u64;

        Ok(LedgerStats {
            total_transfers,
            total_volume,
            total_fees,
            total_entities,
            active_entities,
            highest_balance_entity,
            highest_balance,
            transfers_last_24h,
            volume_last_24h,
        })
    }

    async fn get_federation_stats(&self, federation_id: &str) -> Result<Option<LedgerStats>, LedgerError> {
        // Check if federation exists
        let exists = sqlx::query!(
            "SELECT 1 FROM entities WHERE entity_type = 'Federation' AND entity_id = $1",
            federation_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?
        .is_some();

        if !exists {
            return Ok(None);
        }

        // Get federation-specific stats
        let total_transfers = sqlx::query!(
            "SELECT COUNT(*) as count FROM transfers WHERE federation_id = $1",
            federation_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?
        .count
        .unwrap_or(0) as usize;

        let volume_fees = sqlx::query!(
            "SELECT SUM(amount) as total_volume, SUM(fee) as total_fees FROM transfers WHERE federation_id = $1",
            federation_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?;

        let total_volume = volume_fees.total_volume.unwrap_or(0) as u64;
        let total_fees = volume_fees.total_fees.unwrap_or(0) as u64;

        let entity_counts = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total, 
                COUNT(*) FILTER (WHERE b.balance > 0) as active
            FROM entities e
            JOIN balances b ON e.entity_type = b.entity_type AND e.entity_id = b.entity_id
            WHERE e.federation_id = $1
            "#,
            federation_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?;

        let total_entities = entity_counts.total.unwrap_or(0) as usize;
        let active_entities = entity_counts.active.unwrap_or(0) as usize;

        let highest_balance_entity = sqlx::query!(
            r#"
            SELECT b.entity_type, b.entity_id, b.balance
            FROM balances b
            JOIN entities e ON b.entity_type = e.entity_type AND b.entity_id = e.entity_id
            WHERE e.federation_id = $1
            ORDER BY b.balance DESC
            LIMIT 1
            "#,
            federation_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?;

        let (highest_balance_entity, highest_balance) = match highest_balance_entity {
            Some(row) => {
                let entity = EntityRef {
                    entity_type: entity_type_from_string(&row.entity_type)?,
                    id: row.entity_id,
                };
                (Some(entity), row.balance as u64)
            },
            None => (None, 0)
        };

        let transfers_last_24h = sqlx::query!(
            r#"
            SELECT COUNT(*) as count 
            FROM transfers 
            WHERE federation_id = $1 AND timestamp > NOW() - INTERVAL '24 hours'
            "#,
            federation_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?
        .count
        .unwrap_or(0) as usize;

        let volume_last_24h = sqlx::query!(
            r#"
            SELECT SUM(amount) as sum 
            FROM transfers 
            WHERE federation_id = $1 AND timestamp > NOW() - INTERVAL '24 hours'
            "#,
            federation_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(LedgerError::DatabaseError)?
        .sum
        .unwrap_or(0) as u64;

        Ok(Some(LedgerStats {
            total_transfers,
            total_volume,
            total_fees,
            total_entities,
            active_entities,
            highest_balance_entity,
            highest_balance,
            transfers_last_24h,
            volume_last_24h,
        }))
    }

    async fn create_transfer(
        &self,
        request: &TransferRequest,
        federation_id: String,
        initiator: String,
        fee: u64,
    ) -> Result<Transfer, LedgerError> {
        // Create a Transfer from a TransferRequest
        let transfer = Transfer {
            tx_id: Uuid::new_v4(),
            federation_id,
            from: request.from.clone(),
            to: request.to.clone(),
            amount: request.amount,
            fee,
            initiator,
            timestamp: Utc::now(),
            memo: request.memo.clone(),
            metadata: None,
        };

        // Process the transfer
        self.process_transfer(transfer).await
    }

    async fn ensure_entity_exists(&self, entity: &EntityRef, federation_id: &str) -> Result<(), LedgerError> {
        let mut tx = self.begin_tx().await?;
        self.ensure_entity_exists_in_tx(&mut tx, entity, federation_id).await?;
        tx.commit().await.map_err(LedgerError::DatabaseError)?;
        Ok(())
    }
}

// Helper function to convert string to EntityType
fn entity_type_from_string(entity_type: &str) -> Result<EntityType, LedgerError> {
    match entity_type {
        "Federation" => Ok(EntityType::Federation),
        "Cooperative" => Ok(EntityType::Cooperative),
        "Community" => Ok(EntityType::Community),
        "User" => Ok(EntityType::User),
        _ => Err(LedgerError::Internal(format!("Invalid entity type: {}", entity_type))),
    }
} 