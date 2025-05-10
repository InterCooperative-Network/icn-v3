#[async_trait]
impl LedgerStore for PostgresLedgerStore {
    async fn ensure_entity_exists(&self, entity: &EntityRef, federation_id: &str) -> Result<(), LedgerError> {
        // Use the metrics timer macro to record operation latency and counts
        crate::time_ledger_op!(
            crate::metrics::operations::ENSURE_ENTITY,
            federation_id,
            entity.entity_type.to_string(),
            {
                // Create an entity record if it doesn't exist
                sqlx::query(
                    r#"
                    INSERT INTO entities (id, entity_type, federation_id, created_at)
                    VALUES ($1, $2, $3, NOW())
                    ON CONFLICT (id, entity_type, federation_id) DO NOTHING
                    "#,
                )
                .bind(&entity.id)
                .bind(&entity.entity_type.to_string())
                .bind(federation_id)
                .execute(&self.pool)
                .await
                .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                
                // Report total entity count by type as a gauge metric
                self.update_entity_counts(federation_id).await?;
                
                Ok(())
            }
        )
    }

    async fn process_transfer(&self, transfer: Transfer) -> Result<(u64, u64), LedgerError> {
        // Use metrics timer macro
        crate::time_ledger_op!(
            crate::metrics::operations::TRANSFER,
            &transfer.federation_id,
            transfer.from.entity_type.to_string(),
            {
                // First, ensure both entities exist
                self.ensure_entity_exists(&transfer.from, &transfer.federation_id).await?;
                self.ensure_entity_exists(&transfer.to, &transfer.federation_id).await?;
                
                // Then conduct the transfer in a transaction to ensure atomicity
                let result = sqlx::query_as::<_, (i64, i64)>(
                    r#"
                    WITH
                    source_update AS (
                        UPDATE entities
                        SET balance = balance - $4
                        WHERE id = $1 AND entity_type = $2 AND federation_id = $3 AND balance >= $4
                        RETURNING balance
                    ),
                    destination_update AS (
                        UPDATE entities
                        SET balance = balance + $8
                        WHERE id = $5 AND entity_type = $6 AND federation_id = $7
                        RETURNING balance
                    ),
                    tx_record AS (
                        INSERT INTO transfers (
                            tx_id, federation_id, from_id, from_type, to_id, to_type,
                            amount, fee, initiator, memo, metadata, timestamp
                        )
                        VALUES ($9, $10, $1, $2, $5, $6, $4, $11, $12, $13, $14, $15)
                    )
                    SELECT s.balance, d.balance
                    FROM source_update s, destination_update d
                    "#,
                )
                .bind(&transfer.from.id)
                .bind(&transfer.from.entity_type.to_string())
                .bind(&transfer.federation_id)
                .bind(transfer.amount as i64)
                .bind(&transfer.to.id)
                .bind(&transfer.to.entity_type.to_string())
                .bind(&transfer.federation_id)
                .bind((transfer.amount - transfer.fee) as i64)
                .bind(transfer.tx_id)
                .bind(&transfer.federation_id)
                .bind(transfer.fee as i64)
                .bind(&transfer.initiator)
                .bind(&transfer.memo)
                .bind(&transfer.metadata)
                .bind(&transfer.timestamp)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| LedgerError::DatabaseError(e.to_string()));
                
                match result {
                    Ok(Some((from_balance, to_balance))) => {
                        // Update metrics for successful transfer
                        let counter = metrics::counter!(
                            format!("{}_transfer_volume_total", crate::metrics::METRICS_PREFIX),
                            crate::metrics::labels::FEDERATION => &transfer.federation_id,
                            crate::metrics::labels::ENTITY_TYPE => transfer.from.entity_type.to_string()
                        );
                        counter.increment(1); // Just count the transfer, not the amount
                        
                        // Return the new balances
                        Ok((from_balance as u64, to_balance as u64))
                    },
                    Ok(None) => {
                        // This occurs when source balance is insufficient
                        Err(LedgerError::InsufficientBalance {
                            entity: transfer.from.clone(),
                            amount: transfer.amount,
                        })
                    },
                    Err(e) => Err(e),
                }
            }
        )
    }

    async fn batch_process_transfers(&self, transfers: Vec<Transfer>) -> Result<Vec<(u64, u64)>, LedgerError> {
        crate::time_ledger_op!(
            crate::metrics::operations::BATCH_TRANSFER,
            &transfers.first().map(|t| t.federation_id.as_str()).unwrap_or("unknown"),
            transfers.first().map(|t| t.from.entity_type.to_string()).unwrap_or_else(|| "unknown".to_string()),
            {
                // Use a SQL transaction to ensure atomicity of the batch
                let mut tx = self.pool.begin().await
                    .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                
                let mut results = Vec::with_capacity(transfers.len());
                
                // Process each transfer within the transaction
                for transfer in transfers {
                    // First, ensure both entities exist
                    self.ensure_entity_exists_in_tx(&transfer.from, &transfer.federation_id, &mut tx).await?;
                    self.ensure_entity_exists_in_tx(&transfer.to, &transfer.federation_id, &mut tx).await?;
                    
                    let result = sqlx::query_as::<_, (i64, i64)>(
                        r#"
                        WITH
                        source_update AS (
                            UPDATE entities
                            SET balance = balance - $4
                            WHERE id = $1 AND entity_type = $2 AND federation_id = $3 AND balance >= $4
                            RETURNING balance
                        ),
                        destination_update AS (
                            UPDATE entities
                            SET balance = balance + $8
                            WHERE id = $5 AND entity_type = $6 AND federation_id = $7
                            RETURNING balance
                        ),
                        tx_record AS (
                            INSERT INTO transfers (
                                tx_id, federation_id, from_id, from_type, to_id, to_type,
                                amount, fee, initiator, memo, metadata, timestamp
                            )
                            VALUES ($9, $10, $1, $2, $5, $6, $4, $11, $12, $13, $14, $15)
                        )
                        SELECT s.balance, d.balance
                        FROM source_update s, destination_update d
                        "#,
                    )
                    .bind(&transfer.from.id)
                    .bind(&transfer.from.entity_type.to_string())
                    .bind(&transfer.federation_id)
                    .bind(transfer.amount as i64)
                    .bind(&transfer.to.id)
                    .bind(&transfer.to.entity_type.to_string())
                    .bind(&transfer.federation_id)
                    .bind((transfer.amount - transfer.fee) as i64)
                    .bind(transfer.tx_id)
                    .bind(&transfer.federation_id)
                    .bind(transfer.fee as i64)
                    .bind(&transfer.initiator)
                    .bind(&transfer.memo)
                    .bind(&transfer.metadata)
                    .bind(&transfer.timestamp)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                    
                    match result {
                        Some((from_balance, to_balance)) => {
                            // Update metrics for successful transfer
                            let counter = metrics::counter!(
                                format!("{}_transfer_volume_total", crate::metrics::METRICS_PREFIX),
                                crate::metrics::labels::FEDERATION => &transfer.federation_id,
                                crate::metrics::labels::ENTITY_TYPE => transfer.from.entity_type.to_string()
                            );
                            counter.increment(1); // Just count the transfer, not the amount
                            
                            results.push((from_balance as u64, to_balance as u64));
                        },
                        None => {
                            // Roll back transaction and return error
                            tx.rollback().await
                                .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                            
                            return Err(LedgerError::InsufficientBalance {
                                entity: transfer.from.clone(),
                                amount: transfer.amount,
                            });
                        }
                    }
                }
                
                // Commit the transaction
                tx.commit().await
                    .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                
                Ok(results)
            }
        )
    }

    async fn query_transfers(&self, query: &TransferQuery) -> Result<Vec<Transfer>, LedgerError> {
        crate::time_ledger_op!(
            crate::metrics::operations::QUERY,
            query.federation_id.as_deref().unwrap_or("all"),
            query.entity_type.as_deref().unwrap_or("all").to_string(),
            {
                // Basic query template
                let mut sql = String::from(
                    r#"
                    SELECT 
                        tx_id, federation_id, from_id, from_type, 
                        to_id, to_type, amount, fee, initiator, 
                        memo, metadata, timestamp
                    FROM transfers
                    WHERE 1=1
                    "#,
                );
                
                // Build query conditionally based on provided filters
                let mut params: Vec<sqlx::postgres::PgArguments> = Vec::new();
                let mut param_idx = 1;
                
                // Add federation_id filter if provided
                if let Some(ref fed_id) = query.federation_id {
                    sql.push_str(&format!(" AND federation_id = ${}", param_idx));
                    param_idx += 1;
                    let mut args = sqlx::postgres::PgArguments::default();
                    args.add(fed_id);
                    params.push(args);
                }
                
                // Add entity ID and type filters if provided
                if let Some(ref entity_id) = query.entity_id {
                    sql.push_str(&format!(" AND (from_id = ${} OR to_id = ${})", 
                        param_idx, param_idx));
                    param_idx += 1;
                    let mut args = sqlx::postgres::PgArguments::default();
                    args.add(entity_id);
                    params.push(args);
                }
                
                if let Some(ref entity_type) = query.entity_type {
                    sql.push_str(&format!(" AND (from_type = ${} OR to_type = ${})", 
                        param_idx, param_idx));
                    param_idx += 1;
                    let mut args = sqlx::postgres::PgArguments::default();
                    args.add(entity_type);
                    params.push(args);
                }
                
                // Add time range filters if provided
                if let Some(ref start_time) = query.start_time {
                    sql.push_str(&format!(" AND timestamp >= ${}", param_idx));
                    param_idx += 1;
                    let mut args = sqlx::postgres::PgArguments::default();
                    args.add(start_time);
                    params.push(args);
                }
                
                if let Some(ref end_time) = query.end_time {
                    sql.push_str(&format!(" AND timestamp <= ${}", param_idx));
                    param_idx += 1;
                    let mut args = sqlx::postgres::PgArguments::default();
                    args.add(end_time);
                    params.push(args);
                }
                
                // Add limit and ordering
                sql.push_str(" ORDER BY timestamp DESC");
                if let Some(limit) = query.limit {
                    sql.push_str(&format!(" LIMIT {}", limit));
                }
                
                // Execute the query (simplified here - proper implementation would need to handle params)
                let transfers = sqlx::query_as::<_, TransferRecord>(&sql)
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                
                // Convert records to Transfer objects
                let transfers = transfers.into_iter()
                    .map(|record| record.into())
                    .collect();
                
                Ok(transfers)
            }
        )
    }

    fn get_balance(&self, entity: &EntityRef) -> u64 {
        let federation_id = "unknown"; // This needs to be provided or deduced in a real implementation
        let entity_type = entity.entity_type.to_string();
        
        // We'll measure this operation but not time it since it's synchronous
        metrics::counter!(
            format!("{}_operations_total", crate::metrics::METRICS_PREFIX),
            crate::metrics::labels::OPERATION => crate::metrics::operations::BALANCE,
            crate::metrics::labels::FEDERATION => federation_id,
            crate::metrics::labels::ENTITY_TYPE => entity_type,
            crate::metrics::labels::STATUS => crate::metrics::status::SUCCESS
        )
        .increment(1);
        
        // Simply delegate to the async version and run it in a blocking manner
        futures::executor::block_on(self.get_balance_async(entity))
            .unwrap_or(0)
    }

    async fn get_balance_async(&self, entity: &EntityRef) -> Result<u64, LedgerError> {
        crate::time_ledger_op!(
            crate::metrics::operations::BALANCE,
            "unknown", // Federation ID isn't provided in this method
            entity.entity_type.to_string(),
            {
                let result = sqlx::query_as::<_, (i64,)>(
                    r#"
                    SELECT balance FROM entities
                    WHERE id = $1 AND entity_type = $2
                    LIMIT 1
                    "#,
                )
                .bind(&entity.id)
                .bind(&entity.entity_type.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                
                match result {
                    Some((balance,)) => Ok(balance as u64),
                    None => Ok(0), // Entity doesn't exist, so balance is 0
                }
            }
        )
    }

    async fn get_federation_stats(&self, federation_id: &str) -> Result<LedgerStats, LedgerError> {
        crate::time_ledger_op!(
            "federation_stats",
            federation_id,
            "federation",
            {
                // Query total number of transfers
                let transfers_count: (i64,) = sqlx::query_as(
                    r#"
                    SELECT COUNT(*) FROM transfers
                    WHERE federation_id = $1
                    "#,
                )
                .bind(federation_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                
                // Query total volume
                let total_volume: (i64,) = sqlx::query_as(
                    r#"
                    SELECT COALESCE(SUM(amount), 0) FROM transfers
                    WHERE federation_id = $1
                    "#,
                )
                .bind(federation_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                
                // Query total entities
                let entities_count: (i64,) = sqlx::query_as(
                    r#"
                    SELECT COUNT(*) FROM entities
                    WHERE federation_id = $1
                    "#,
                )
                .bind(federation_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                
                // Query entities by type
                let entities_by_type: Vec<(String, i64)> = sqlx::query_as(
                    r#"
                    SELECT entity_type, COUNT(*) 
                    FROM entities
                    WHERE federation_id = $1
                    GROUP BY entity_type
                    "#,
                )
                .bind(federation_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                
                // Convert to a HashMap
                let entities_by_type = entities_by_type
                    .into_iter()
                    .map(|(entity_type, count)| (entity_type, count as u64))
                    .collect();
                
                // Query total balances by entity type
                let balances_by_type: Vec<(String, i64)> = sqlx::query_as(
                    r#"
                    SELECT entity_type, SUM(balance) 
                    FROM entities
                    WHERE federation_id = $1
                    GROUP BY entity_type
                    "#,
                )
                .bind(federation_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
                
                // Convert to a HashMap
                let balances_by_type = balances_by_type
                    .into_iter()
                    .map(|(entity_type, balance)| (entity_type, balance as u64))
                    .collect();
                
                // Update metrics
                let transfers_gauge = metrics::gauge!(
                    format!("{}_federation_transfers_count", crate::metrics::METRICS_PREFIX),
                    crate::metrics::labels::FEDERATION => federation_id
                );
                transfers_gauge.set(transfers_count.0 as f64);

                let volume_gauge = metrics::gauge!(
                    format!("{}_federation_volume_total", crate::metrics::METRICS_PREFIX),
                    crate::metrics::labels::FEDERATION => federation_id
                );
                volume_gauge.set(total_volume.0 as f64);

                let entities_gauge = metrics::gauge!(
                    format!("{}_federation_entities_count", crate::metrics::METRICS_PREFIX),
                    crate::metrics::labels::FEDERATION => federation_id
                );
                entities_gauge.set(entities_count.0 as f64);
                
                // Return the stats
                Ok(LedgerStats {
                    transfers_count: transfers_count.0 as u64,
                    total_volume: total_volume.0 as u64,
                    entities_count: entities_count.0 as u64,
                    entities_by_type,
                    balances_by_type,
                })
            }
        )
    }
}

// Add helper methods for the PostgresLedgerStore
impl PostgresLedgerStore {
    // Helper method to ensure an entity exists within a transaction
    async fn ensure_entity_exists_in_tx<'a>(
        &self, 
        entity: &EntityRef, 
        federation_id: &str,
        tx: &mut sqlx::Transaction<'a, sqlx::Postgres>
    ) -> Result<(), LedgerError> {
        // Create an entity record if it doesn't exist
        sqlx::query(
            r#"
            INSERT INTO entities (id, entity_type, federation_id, created_at)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (id, entity_type, federation_id) DO NOTHING
            "#,
        )
        .bind(&entity.id)
        .bind(&entity.entity_type.to_string())
        .bind(federation_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
        
        Ok(())
    }
    
    // Update entity count metrics
    async fn update_entity_counts(&self, federation_id: &str) -> Result<(), LedgerError> {
        // Query entities by type
        let entities_by_type: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT entity_type, COUNT(*) 
            FROM entities
            WHERE federation_id = $1
            GROUP BY entity_type
            "#,
        )
        .bind(federation_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| LedgerError::DatabaseError(e.to_string()))?;
        
        // Update metrics for each entity type
        for (entity_type, count) in entities_by_type {
            crate::metrics::update_resource_gauge(
                "entities_count",
                count as u64,
                federation_id,
                &[(crate::metrics::labels::ENTITY_TYPE, &entity_type)]
            );
        }
        
        Ok(())
    }
} 