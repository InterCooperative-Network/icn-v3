use std::sync::Arc;
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use std::collections::HashMap;
use std::sync::RwLock;
use cid::Cid;
use async_trait::async_trait;
use serde::Serialize;
use sha2::{Digest, Sha256};
use multihash::{Code, Multihash};
use anyhow::Result;
use sqlx::QueryBuilder;
use std::str::FromStr;
use serde_json;
use icn_identity::Did;
use sqlx::Acquire;
use icn_types::mesh::JobStatus;

use crate::storage::MeshJobStore;
use crate::types::{Bid, JobRequest, JobRequirements};
use crate::error::AppError;

// Helper struct for fetching bid rows
#[derive(sqlx::FromRow, Debug)]
struct DbBidRow {
    id: i64,
    job_id: String,
    bidder_did: String,
    price: i64,
    resources_json: String,
}

pub struct SqliteStore {
    pub pool: Arc<SqlitePool>,
    pub bid_broadcasters: RwLock<HashMap<String, broadcast::Sender<Bid>>>,
}

impl SqliteStore {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
            bid_broadcasters: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl MeshJobStore for SqliteStore {
    async fn insert_job(&self, job_request: JobRequest) -> Result<String, AppError> {
        let job_id_str = job_request.id;
        let owner_did_str = job_request.owner_did.to_string();
        let cid_str = job_request.cid.to_string();
        let requirements_json = serde_json::to_string(&job_request.requirements)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize requirements: {}", e)))?;
        let status_type = "InProgress";
        
        sqlx::query!(
            r#"
            INSERT INTO jobs (job_id, owner_did, cid, requirements_json, status_type)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            job_id_str,
            owner_did_str,
            cid_str,
            requirements_json,
            status_type
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to insert job into database: {}. Job ID: {}", e, job_id_str)))?;
        
        Ok(job_id_str)
    }

    async fn get_job(&self, job_id: &str) -> Result<Option<(JobRequest, JobStatus)>, AppError> {
        #[derive(sqlx::FromRow)]
        struct JobRow {
            owner_did: String,
            cid: String,
            requirements_json: String,
            status_type: String,
            status_did: Option<String>,
            status_reason: Option<String>,
        }

        let job_row_opt = sqlx::query_as!(
            JobRow,
            r#"
            SELECT owner_did, cid, requirements_json, status_type, status_did, status_reason
            FROM jobs
            WHERE job_id = $1
            "#,
            job_id
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch job from database: {}", e)))?;

        match job_row_opt {
            Some(row) => {
                let requirements = serde_json::from_str(&row.requirements_json)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to deserialize requirements for job {}: {}", job_id, e)))?;
                
                let owner_did = Did::new_ed25519(row.owner_did);
                let cid = Cid::try_from(row.cid.as_str())
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse CID for job {}: {}", job_id, e)))?;

                let job_request = JobRequest {
                    id: job_id.to_string(),
                    owner_did,
                    cid,
                    requirements,
                };

                let job_status = match row.status_type.as_str() {
                    "InProgress" => JobStatus::InProgress,
                    "Completed" => JobStatus::Completed,
                    "Failed" => JobStatus::Failed,
                    "Cancelled" => JobStatus::Cancelled,
                    _ => return Err(AppError::Internal(anyhow::anyhow!("Unknown job status type '{}' for job {}", row.status_type, job_id))),
                };
                Ok(Some((job_request, job_status)))
            }
            None => Ok(None),
        }
    }

    async fn list_jobs(&self, status_filter: Option<JobStatus>) -> Result<Vec<String>, AppError> {
        #[derive(sqlx::FromRow)]
        struct JobIdRow {
            job_id: String,
        }

        let mut query_builder = QueryBuilder::new("SELECT job_id FROM jobs");

        if let Some(filter) = status_filter {
            query_builder.push(" WHERE ");
            match filter {
                JobStatus::InProgress => {
                    query_builder.push("status_type = 'InProgress'");
                }
                JobStatus::Completed => {
                    query_builder.push("status_type = 'Completed'");
                }
                JobStatus::Failed => {
                    query_builder.push("status_type = 'Failed'");
                }
                JobStatus::Cancelled => {
                    query_builder.push("status_type = 'Cancelled'");
                }
            }
        }

        let jobs_query = query_builder.build_query_as::<JobIdRow>();

        let rows = jobs_query
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to list jobs from database: {}", e)))?;

        Ok(rows.into_iter().map(|row| row.job_id).collect())
    }

    async fn update_job_status(&self, job_id: &str, new_status: JobStatus) -> Result<(), AppError> {
        let status_type = match new_status {
            JobStatus::InProgress => "InProgress",
            JobStatus::Completed => "Completed",
            JobStatus::Failed => "Failed",
            JobStatus::Cancelled => "Cancelled",
        };

        let result = sqlx::query!(
            r#"
            UPDATE jobs
            SET status_type = $1
            WHERE job_id = $2
            "#,
            status_type,
            job_id
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to update job status in database: {}", e)))?;

        if result.rows_affected() == 0 {
            Err(AppError::NotFound(format!("Job with ID {} not found for status update", job_id)))
        } else {
            Ok(())
        }
    }

    async fn insert_bid(&self, job_id: &str, bid: Bid) -> Result<(), AppError> {
        let bidder_did_str = bid.bidder_did.to_string();
        let resources_json = serde_json::to_string(&bid.resources)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize resources: {}", e)))?;

        sqlx::query!(
            r#"
            INSERT INTO bids (job_id, bidder_did, price, resources_json)
            VALUES ($1, $2, $3, $4)
            "#,
            job_id,
            bidder_did_str,
            bid.price as i64,
            resources_json
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to insert bid into database: {}", e)))?;

        // Broadcast the bid to any subscribers
        if let Some(sender) = self.bid_broadcasters.read().unwrap().get(job_id) {
            let _ = sender.send(bid.clone());
        }

        Ok(())
    }

    async fn list_bids(&self, job_id: &str) -> Result<Vec<Bid>, AppError> {
        let bid_rows = sqlx::query_as!(
            DbBidRow,
            r#"
            SELECT id, job_id, bidder_did, price, resources_json
            FROM bids
            WHERE job_id = $1
            "#,
            job_id
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch bids from database: {}", e)))?;

        let mut bids = Vec::new();
        for row in bid_rows {
            let resources = serde_json::from_str(&row.resources_json)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to deserialize resources for bid {}: {}", row.id, e)))?;
            
            let bidder_did = Did::new_ed25519(row.bidder_did);

            bids.push(Bid {
                job_id: row.job_id,
                bidder_did,
                price: row.price as u64,
                resources,
            });
        }

        Ok(bids)
    }

    async fn subscribe_to_bids(&self, job_id: &str) -> Result<Option<broadcast::Receiver<Bid>>, AppError> {
        let mut broadcasters = self.bid_broadcasters.write().unwrap();
        let sender = broadcasters.entry(job_id.to_string()).or_insert_with(|| {
            let (tx, _) = broadcast::channel(32);
            tx
        });
        Ok(Some(sender.subscribe()))
    }

    async fn assign_job(&self, job_id: &str, winning_bid_id: i64) -> Result<(), AppError> {
        let mut tx = self.pool.begin().await.map_err(|e| {
            tracing::error!("Failed to begin database transaction: {:?}", e);
            AppError::Internal(anyhow::Error::new(e).context("Failed to begin database transaction"))
        })?;

        // Get the winning bid to get the bidder's DID
        let winning_bid = sqlx::query_as!(
            DbBidRow,
            r#"
            SELECT id, job_id, bidder_did, price, resources_json
            FROM bids
            WHERE id = $1 AND job_id = $2
            "#,
            winning_bid_id,
            job_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch winning bid: {:?}", e);
            AppError::Internal(anyhow::Error::new(e).context("Failed to fetch winning bid"))
        })?
        .ok_or_else(|| AppError::NotFound(format!("Winning bid {} not found for job {}", winning_bid_id, job_id)))?;

        let bidder_did = Did::new_ed25519(winning_bid.bidder_did);

        // Update job status to InProgress
        let update_job_result = sqlx::query!(
            r#"
            UPDATE jobs
            SET status_type = 'InProgress',
                status_did = $1,
                winning_bid_id = $2,
                updated_at = CURRENT_TIMESTAMP
            WHERE job_id = $3 AND status_type = 'Pending'
            "#,
            bidder_did.to_string(),
            winning_bid_id,
            job_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update job status: {:?}", e);
            AppError::Internal(anyhow::Error::new(e).context("Failed to update job status"))
        })?;

        if update_job_result.rows_affected() == 0 {
            tx.rollback().await.map_err(|e| AppError::Internal(anyhow::Error::new(e).context("Failed to rollback transaction")))?;
            return Err(AppError::NotFound(format!(
                "Job {} not found or not in a state that can be assigned",
                job_id
            )));
        }

        // Update bid statuses
        sqlx::query!(
            r#"
            UPDATE bids
            SET status = CASE 
                WHEN id = $1 THEN 'Won'
                ELSE 'Lost'
            END,
            updated_at = CURRENT_TIMESTAMP
            WHERE job_id = $2
            "#,
            winning_bid_id,
            job_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update bid statuses: {:?}", e);
            AppError::Internal(anyhow::Error::new(e).context("Failed to update bid statuses"))
        })?;

        tx.commit().await.map_err(|e| {
            tracing::error!("Failed to commit transaction: {:?}", e);
            AppError::Internal(anyhow::Error::new(e).context("Failed to commit transaction"))
        })?;

        Ok(())
    }

    async fn list_jobs_for_worker(&self, worker_did: &Did) -> Result<Vec<(JobRequest, JobStatus)>, AppError> {
        let worker_did_str = worker_did.to_string();

        #[derive(sqlx::FromRow)]
        struct WorkerJobRow {
            job_id: String,
            owner_did: String,
            cid: String,
            requirements_json: String,
            status_type: String,
            status_did: Option<String>,
            status_reason: Option<String>,
        }

        let rows = sqlx::query_as!(
            WorkerJobRow,
            r#"
            SELECT job_id, owner_did, cid, requirements_json, status_type, status_did, status_reason
            FROM jobs
            WHERE status_type = 'InProgress' AND status_did = $1
            ORDER BY created_at DESC
            "#,
            worker_did_str
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch jobs for worker from database: {}", e)))?;

        let mut worker_jobs = Vec::new();
        for row in rows {
            let requirements = serde_json::from_str(&row.requirements_json)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to deserialize requirements for job {}: {}", row.job_id, e)))?;
            
            let owner_did = Did::new_ed25519(row.owner_did);
            let cid = Cid::try_from(row.cid.as_str())
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse CID for job {}: {}", row.job_id, e)))?;

            let job_request = JobRequest {
                id: row.job_id,
                owner_did,
                cid,
                requirements,
            };

            let job_status = JobStatus::InProgress;
            worker_jobs.push((job_request, job_status));
        }
        Ok(worker_jobs)
    }
} 