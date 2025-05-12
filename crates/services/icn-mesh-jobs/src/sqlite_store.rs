use std::sync::Arc;
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use std::collections::HashMap;
use std::sync::RwLock; // Changed from Mutex to RwLock as per user's struct definition
use cid::Cid;
use async_trait::async_trait;
use icn_types::jobs::{JobRequest, JobStatus, Bid, ResourceRequirements, ResourceEstimate};
use serde::Serialize;
use sha2::{Digest, Sha256};
use multihash::{Code, Multihash};
use anyhow::Result; // For internal helper functions
use sqlx::QueryBuilder;
use std::str::FromStr; // Required for Cid::from_str if that's how try_from is implemented for String
use serde_json; // Ensure this is imported
use icn_identity::Did; // Ensure Did is imported
use sqlx::Acquire; // For transactions
use icn_types::mesh::MeshJobParams; // Added for clarity, though JobRequest imports it

// Assuming AppError is defined in lib.rs or a common error module for the crate
// and has From implementations for anyhow::Error, sqlx::Error, serde_json::Error
use crate::storage::MeshJobStore; 
// It seems AppError is defined in main.rs, let's adjust the path if this file is a module of main.
// For now, assuming `crate::AppError` or that `main.rs` makes `AppError` accessible.
// If `storage.rs` is in `src/storage.rs` and this is `src/sqlite_store.rs`, then `crate::storage::MeshJobStore` is correct.
// If `AppError` is in `main.rs`, we might need `crate::main::AppError` or make it public in `lib.rs`.
// Given the previous context, AppError is in main.rs. Let's assume it's made accessible at crate level or we adjust this.
// For now, let's assume `crate::AppError` might not be directly available if `main.rs` is a binary root.
// The trait signature provided by user is `Result<Cid, AppError>`. Let's stick to that.
// We will need a proper error type. For now, let's assume `anyhow::Error` can be mapped to `AppError`.
// The user provided: use crate::AppError; I will keep this, assuming it's correctly pathed from the crate root.
use crate::AppError;

use crate::types::{Bid, JobRequest, JobRequirements};
use icn_types::mesh::JobStatus;

// Helper to generate CID for a JobRequest
// This is a basic implementation. In a production system, you'd use a canonical serialization format.
// fn generate_job_cid(req: &JobRequest) -> Result<Cid> { ... }

pub struct SqliteStore {
    pub pool: Arc<SqlitePool>,
    /// In-memory broadcasters for real-time bid subscriptions (non-persistent)
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

// Helper struct for fetching bid rows
#[derive(sqlx::FromRow, Debug)]
struct DbBidRow {
    id: i64,
    job_id: String,
    bidder_did: String,
    price: i64,
    resources_json: String,
}

#[async_trait]
impl MeshJobStore for SqliteStore {
    async fn insert_job(&self, job_request: JobRequest) -> Result<String, AppError> {
        let job_id_str = job_request.id;
        let owner_did_str = job_request.owner_did.to_string();
        let cid_str = job_request.cid.to_string();
        let requirements_json = serde_json::to_string(&job_request.requirements)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize requirements: {}", e)))?;
        let status_type = "Pending";
        
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
                    "Pending" => JobStatus::Pending,
                    "Bidding" => JobStatus::Bidding,
                    "Assigned" => {
                        let bidder_did_str = row.status_did.ok_or_else(|| 
                            AppError::Internal(anyhow::anyhow!("Missing bidder_did for Assigned status in job {}", job_id)))?;
                        JobStatus::Assigned { bidder: Did::new_ed25519(bidder_did_str) }
                    }
                    "Running" => {
                        let runner_did_str = row.status_did.ok_or_else(|| 
                            AppError::Internal(anyhow::anyhow!("Missing runner_did for Running status in job {}", job_id)))?;
                        JobStatus::Running { runner: Did::new_ed25519(runner_did_str) }
                    }
                    "Completed" => JobStatus::Completed,
                    "Failed" => {
                        let reason = row.status_reason.ok_or_else(|| 
                            AppError::Internal(anyhow::anyhow!("Missing reason for Failed status in job {}", job_id)))?;
                        JobStatus::Failed { reason }
                    }
                    _ => return Err(AppError::Internal(anyhow::anyhow!("Unknown job status type '{}' for job {}", row.status_type, job_id))),
                };
                Ok(Some((job_request, job_status)))
            }
            None => Ok(None),
        }
    }

    async fn list_jobs(&self, status_filter: Option<JobStatus>) -> Result<Vec<Cid>, AppError> {
        #[derive(sqlx::FromRow)]
        struct JobIdRow {
            job_id: String,
        }

        let mut query_builder = QueryBuilder::new("SELECT job_id FROM jobs");

        if let Some(filter) = status_filter {
            query_builder.push(" WHERE ");
            match filter {
                JobStatus::Pending => {
                    query_builder.push("status_type = ");
                    query_builder.push_bind(JobStatus::Pending.to_string()); // Assuming JobStatus can be stringified to its variant name
                    query_builder.push(" AND status_did IS NULL AND status_reason IS NULL");
                }
                JobStatus::Bidding => {
                    query_builder.push("status_type = ");
                    query_builder.push_bind(JobStatus::Bidding.to_string());
                    query_builder.push(" AND status_did IS NULL AND status_reason IS NULL");
                }
                JobStatus::Assigned { bidder } => {
                    query_builder.push("status_type = ");
                    query_builder.push_bind("Assigned");
                    query_builder.push(" AND status_did = ");
                    query_builder.push_bind(bidder.0);
                    query_builder.push(" AND status_reason IS NULL");
                }
                JobStatus::Running { runner } => {
                    query_builder.push("status_type = ");
                    query_builder.push_bind("Running");
                    query_builder.push(" AND status_did = ");
                    query_builder.push_bind(runner.0);
                    query_builder.push(" AND status_reason IS NULL");
                }
                JobStatus::Completed => {
                    query_builder.push("status_type = ");
                    query_builder.push_bind(JobStatus::Completed.to_string());
                    query_builder.push(" AND status_did IS NULL AND status_reason IS NULL");
                }
                JobStatus::Failed { reason } => {
                    query_builder.push("status_type = ");
                    query_builder.push_bind("Failed");
                    query_builder.push(" AND status_did IS NULL AND status_reason = ");
                    query_builder.push_bind(reason);
                }
            }
        }

        let jobs_query = query_builder.build_query_as::<JobIdRow>();

        let rows = jobs_query
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to list jobs from database: {}", e)))?;

        let mut cids = Vec::new();
        for row in rows {
            let cid = Cid::try_from(row.job_id.as_str())
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse job_id as Cid: {} for job_id {}", e, row.job_id)))?;
            cids.push(cid);
        }

        Ok(cids)
    }
    
    async fn update_job_status(&self, job_id: &Cid, new_status: JobStatus) -> Result<(), AppError> {
        let job_id_str = job_id.to_string();

        let (status_type, status_did, status_reason) = match new_status {
            JobStatus::Pending => ("Pending".to_string(), None, None),
            JobStatus::Bidding => ("Bidding".to_string(), None, None),
            JobStatus::Assigned { bidder } => ("Assigned".to_string(), Some(bidder.0), None),
            JobStatus::Running { runner } => ("Running".to_string(), Some(runner.0), None),
            JobStatus::Completed => ("Completed".to_string(), None, None),
            JobStatus::Failed { reason } => ("Failed".to_string(), None, Some(reason)),
        };

        let result = sqlx::query!(
            r#"
            UPDATE jobs
            SET status_type = $1, status_did = $2, status_reason = $3
            WHERE job_id = $4
            "#,
            status_type,
            status_did,
            status_reason,
            job_id_str
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to update job status in database: {}", e)))?;

        if result.rows_affected() == 0 {
            Err(AppError::NotFound(format!("Job with ID {} not found for status update", job_id_str)))
        } else {
            Ok(())
        }
    }

    async fn insert_bid(&self, bid: Bid) -> Result<(), AppError> {
        let job_id_str = bid.job_id;
        let bidder_did_str = bid.bidder_did.to_string();
        let resources_json = serde_json::to_string(&bid.resources)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize resources: {}", e)))?;

        sqlx::query!(
            r#"
            INSERT INTO bids (job_id, bidder_did, price, resources_json)
            VALUES ($1, $2, $3, $4)
            "#,
            job_id_str,
            bidder_did_str,
            bid.price as i64,
            resources_json
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to insert bid into database: {}", e)))?;

        // Broadcast the bid to any subscribers
        if let Some(sender) = self.bid_broadcasters.read().unwrap().get(&bid.job_id) {
            let _ = sender.send(bid.clone());
        }

        Ok(())
    }

    async fn get_bids_for_job(&self, job_id: &str) -> Result<Vec<Bid>, AppError> {
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

    async fn subscribe_to_bids(&self, job_id: &Cid) -> Result<Option<broadcast::Receiver<Bid>>, AppError> {
        // Placeholder - this will likely remain mostly in-memory logic
        let mut broadcasters = self.bid_broadcasters.write().unwrap(); // Handle potential poison
        let sender = broadcasters.entry(job_id.to_string()).or_insert_with(|| {
            let (tx, _) = broadcast::channel(32); // Default capacity
            tx
        });
        Ok(Some(sender.subscribe()))
    }

    async fn assign_job(
        &self,
        job_id_param: &Cid,
        winning_bid_id: i64, // ID of the winning bid
        winning_bidder_did: Did, // DID of the winning bidder
    ) -> Result<(), AppError> {
        let job_id_str = job_id_param.to_string();
        let winning_bidder_did_str = winning_bidder_did.0; // Get the String from Did

        tracing::info!(
            job_id = %job_id_str,
            winning_bid_id = winning_bid_id,
            winning_bidder_did = %winning_bidder_did_str,
            "Attempting to assign job"
        );

        // Start a database transaction
        let mut tx = self.pool.begin().await.map_err(|e| {
            tracing::error!("Failed to begin database transaction: {:?}", e);
            AppError::Internal(anyhow::Error::new(e).context("Failed to begin database transaction"))
        })?;

        // 1. Update the jobs table: set status to Assigned, store winning_bidder_did and winning_bid_id
        let update_job_result = sqlx::query!(
            r#"
            UPDATE jobs
            SET status_type = 'Assigned',
                status_did = $1,
                winning_bid_id = $2,
                updated_at = CURRENT_TIMESTAMP
            WHERE job_id = $3 AND (status_type = 'Pending' OR status_type = 'Bidding')
            "#,
            winning_bidder_did_str, // $1
            winning_bid_id,         // $2
            job_id_str              // $3
        )
        .execute(&mut *tx) // Use &mut *tx for executing queries within a transaction
        .await
        .map_err(|e| {
            tracing::error!("Failed to update job status to Assigned in DB: {:?}", e);
            AppError::Internal(anyhow::Error::new(e).context("Failed to update job to Assigned"))
        })?;

        if update_job_result.rows_affected() == 0 {
            tracing::warn!(
                job_id = %job_id_str,
                "Failed to assign job: either job not found or not in Pending/Bidding state"
            );
            // Rollback transaction before returning error
            tx.rollback().await.map_err(|e_rb| AppError::Internal(anyhow::Error::new(e_rb).context("Failed to rollback transaction after job assignment failure")))?;
            return Err(AppError::NotFound(format!(
                "Job with ID {} not found or not in a state that can be assigned (Pending/Bidding)",
                job_id_str
            )));
        }

        // 2. Update the winning bid's status to 'Won'
        let update_winning_bid_result = sqlx::query!(
            r#"
            UPDATE bids
            SET status = 'Won',
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $1 AND job_id = $2
            "#,
            winning_bid_id, // $1
            job_id_str      // $2
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update winning bid status in DB: {:?}", e);
            AppError::Internal(anyhow::Error::new(e).context("Failed to update winning bid status"))
        })?;

        if update_winning_bid_result.rows_affected() == 0 {
             tracing::warn!(
                job_id = %job_id_str,
                winning_bid_id = winning_bid_id,
                "Failed to mark bid as Won: winning bid not found or does not belong to the job"
            );
            tx.rollback().await.map_err(|e_rb| AppError::Internal(anyhow::Error::new(e_rb).context("Failed to rollback transaction after winning bid update failure")))?;
            return Err(AppError::Internal(anyhow::anyhow!(
                "Failed to mark winning bid {} for job {} as Won", winning_bid_id, job_id_str
            )));
        }


        // 3. Update all other bids for this job to 'Lost'
        sqlx::query!(
            r#"
            UPDATE bids
            SET status = 'Lost',
                updated_at = CURRENT_TIMESTAMP
            WHERE job_id = $1 AND id != $2 AND status = 'Pending' -- Only update other 'Pending' bids
            "#,
            job_id_str,     // $1
            winning_bid_id  // $2
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update other bids to Lost in DB: {:?}", e);
            AppError::Internal(anyhow::Error::new(e).context("Failed to update other bids to Lost"))
        })?;
        // Note: rows_affected for this query can be 0 if there are no other pending bids, which is fine.

        // Commit the transaction
        tx.commit().await.map_err(|e| {
            tracing::error!("Failed to commit database transaction: {:?}", e);
            AppError::Internal(anyhow::Error::new(e).context("Failed to commit database transaction"))
        })?;

        tracing::info!(
            job_id = %job_id_str,
            winning_bid_id = winning_bid_id,
            "Job successfully assigned"
        );
        Ok(())
    }

    async fn list_jobs_for_worker(&self, worker_did: &icn_identity::Did) -> Result<Vec<(JobRequest, JobStatus)>, AppError> {
        let worker_did_str = worker_did.0.clone();

        #[derive(sqlx::FromRow)]
        struct WorkerJobRow {
            job_id: String,         // To be parsed as Cid for JobRequest.id
            originator_did: String, // To be parsed as Did for JobRequest.originator_did
            params_json: String,    // To be deserialized into MeshJobParams for JobRequest.params
            status_type: String,
            status_did: Option<String>,
            status_reason: Option<String>,
        }

        let rows = sqlx::query_as!(
            WorkerJobRow,
            r#"
            SELECT job_id, originator_did, params_json, status_type, status_did, status_reason
            FROM jobs
            WHERE (status_type = 'Assigned' AND status_did = $1)
               OR (status_type = 'Running' AND status_did = $1)
            ORDER BY created_at DESC -- Or some other relevant ordering
            "#,
            worker_did_str
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch jobs for worker from database: {}", e)))?;

        let mut worker_jobs = Vec::new();
        for row in rows {
            let job_cid = Cid::try_from(row.job_id.as_str())
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse job_id as Cid for worker job {}: {}", row.job_id, e)))?;
            
            let params: MeshJobParams = serde_json::from_str(&row.params_json)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to deserialize MeshJobParams for worker job {}: {}", row.job_id, e)))?;

            let originator_did = Did::new_ed25519(row.originator_did);

            let job_request = JobRequest {
                id: job_cid,
                params,
                originator_did,
            };

            let job_status = match row.status_type.as_str() {
                "Assigned" => {
                    let bidder_did_str = row.status_did.ok_or_else(|| {
                        AppError::Internal(anyhow::anyhow!("Missing bidder_did for Assigned status in worker job {}", job_request.id))
                    })?;
                    JobStatus::Assigned { bidder: Did::new_ed25519(bidder_did_str) }
                }
                "Running" => {
                    let runner_did_str = row.status_did.ok_or_else(|| {
                        AppError::Internal(anyhow::anyhow!("Missing runner_did for Running status in worker job {}", job_request.id))
                    })?;
                    JobStatus::Running { runner: Did::new_ed25519(runner_did_str) }
                }
                _ => return Err(AppError::Internal(anyhow::anyhow!(
                    "Unexpected job status type '{}' for worker job {}", 
                    row.status_type, 
                    job_request.id
                )))
            };
            worker_jobs.push((job_request, job_status));
        }
        Ok(worker_jobs)
    }
} 