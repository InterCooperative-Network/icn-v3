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


// Helper to generate CID for a JobRequest
// This is a basic implementation. In a production system, you'd use a canonical serialization format.
fn generate_job_cid(req: &JobRequest) -> Result<Cid> {
    let bytes = serde_json::to_vec(req).map_err(|e| anyhow::anyhow!("Failed to serialize JobRequest for CID generation: {}", e))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash_bytes = hasher.finalize();
    let multihash = Multihash::wrap(Code::Sha2_256.into(), &hash_bytes)?;
    Ok(Cid::new_v1(0x55, multihash)) // 0x55 is raw binary
}

pub struct SqliteStore {
    pub pool: Arc<SqlitePool>,
    /// In-memory broadcasters for real-time bid subscriptions (non-persistent)
    pub bid_broadcasters: RwLock<HashMap<Cid, broadcast::Sender<Bid>>>,
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
    async fn insert_job(&self, job_request: JobRequest) -> Result<Cid, AppError> {
        let job_request_json = serde_json::to_string(&job_request)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize job request: {}", e)))?;

        let job_cid = generate_job_cid(&job_request)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to generate job CID: {}", e)))?;
        
        let job_cid_str = job_cid.to_string();

        // Initial status for a new job is Pending
        let status_type = "Pending"; // From JobStatus::Pending

        // Convert Option<DateTime<Utc>> to Option<i64> (Unix timestamp)
        let deadline_timestamp = job_request.deadline.map(|dt| dt.timestamp());

        // Store wasm_cid separately for potential indexing, though JobRequest JSON also has it.
        let wasm_cid_str = job_request.wasm_cid.to_string();
        let description = job_request.description.clone();


        sqlx::query!(
            r#"
            INSERT INTO jobs (job_id, request_json, status_type, deadline, wasm_cid, description)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            job_cid_str,
            job_request_json,
            status_type,
            deadline_timestamp,
            wasm_cid_str,
            description
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to insert job into database: {}", e)))?;

        Ok(job_cid)
    }

    async fn get_job(&self, job_id: &Cid) -> Result<Option<(JobRequest, JobStatus)>, AppError> {
        let job_id_str = job_id.to_string();

        // Define a struct that matches the expected row structure from the database
        struct JobRow {
            request_json: String,
            status_type: String,
            status_did: Option<String>,
            status_reason: Option<String>,
        }

        let job_row_opt = sqlx::query_as!(
            JobRow,
            r#"
            SELECT request_json, status_type, status_did, status_reason
            FROM jobs
            WHERE job_id = $1
            "#,
            job_id_str
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch job from database: {}", e)))?;

        match job_row_opt {
            Some(row) => {
                let job_request: JobRequest = serde_json::from_str(&row.request_json)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to deserialize job request: {}", e)))?;

                let job_status = match row.status_type.as_str() {
                    "Pending" => JobStatus::Pending,
                    "Bidding" => JobStatus::Bidding,
                    "Assigned" => {
                        let bidder_did_str = row.status_did.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Missing bidder_did for Assigned status in job {}", job_id_str)))?;
                        JobStatus::Assigned { bidder: icn_identity::Did(bidder_did_str) }
                    }
                    "Running" => {
                        let runner_did_str = row.status_did.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Missing runner_did for Running status in job {}", job_id_str)))?;
                        JobStatus::Running { runner: icn_identity::Did(runner_did_str) }
                    }
                    "Completed" => JobStatus::Completed,
                    "Failed" => {
                        let reason = row.status_reason.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Missing reason for Failed status in job {}", job_id_str)))?;
                        JobStatus::Failed { reason }
                    }
                    _ => return Err(AppError::Internal(anyhow::anyhow!("Unknown job status type '{}' for job {}", row.status_type, job_id_str))),
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

    async fn insert_bid(&self, job_id_param: &Cid, bid: Bid) -> Result<(), AppError> {
        // 1. Fetch Job and Validate Status
        let (_job_request, current_status) = self.get_job(job_id_param).await?
            .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id_param)))?;

        match current_status {
            JobStatus::Pending | JobStatus::Bidding => { /* Allowed */ }
            _ => {
                return Err(AppError::BadRequest(format!(
                    "Job {} is in status {:?} and cannot accept bids",
                    job_id_param,
                    current_status
                )));
            }
        }

        // 2. Validate Bid's Job ID
        if &bid.job_id != job_id_param {
            return Err(AppError::BadRequest(
                "Job ID in bid payload does not match job_id in path".to_string(),
            ));
        }

        // 3. Serialize Bid Data
        let estimate_json = serde_json::to_string(&bid.estimate)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to serialize bid estimate: {}", e)))?;
        
        let job_id_str = bid.job_id.to_string(); // or job_id_param.to_string()
        let bidder_did_str = bid.bidder.0.clone(); // Clone because bid might be consumed by broadcaster
        let price = bid.price; // u64, compatible with INTEGER in SQLite
        let reputation_score = bid.reputation_score; // Option<f64>, compatible with REAL NULL in SQLite

        // 4. Database Insertion
        sqlx::query!(
            r#"
            INSERT INTO bids (job_id, bidder_did, price, estimate_json, reputation_score)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            job_id_str,
            bidder_did_str,
            price,
            estimate_json,
            reputation_score
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to insert bid into database: {}", e)))?;

        // 5. Broadcast Bid
        let broadcaster_maybe = {
            // Scope for the read lock
            let broadcasters_read_guard = self.bid_broadcasters.read()
                .map_err(|_| AppError::Internal(anyhow::anyhow!("Bid broadcaster read lock poisoned")))?;
            broadcasters_read_guard.get(job_id_param).cloned() // Clone the sender if it exists
        };

        if let Some(broadcaster) = broadcaster_maybe {
            if let Err(_send_error) = broadcaster.send(bid) {
                // Log this in a real scenario, e.g., using tracing::debug!
                // For now, we'll ignore if no subscribers, as the DB insert succeeded.
                // tracing::debug!("Failed to broadcast bid for job {}: {}, no active subscribers?", job_id_param, send_error.to_string());
            }
        } else {
            // No broadcaster existed, meaning no one was subscribed yet. This is fine.
            // The bid is in the DB. If someone subscribes later, they won't get this old bid via this live channel,
            // but would see it if they list_bids. This matches InMemoryStore behavior where send is best-effort.
            // If the bid was consumed by a send attempt above, ensure it's handled. (bid is consumed by send())
            // Since we only send if broadcaster exists, and `bid` is taken by `send`, this is okay.
        }
        
        // If the intention is to create the broadcaster if it doesn't exist, like InMemoryStore:
        // let broadcaster = {
        //     let mut broadcasters_write_guard = self.bid_broadcasters.write()
        //         .map_err(|_| AppError::Internal(anyhow::anyhow!("Bid broadcaster write lock poisoned")))?;
        //     broadcasters_write_guard.entry(*job_id_param).or_insert_with(|| broadcast::channel(32).0).clone()
        // };
        // if let Err(_send_error) = broadcaster.send(bid) { ... }
        // For now, sticking to the simpler read-lock version unless explicit creation is required.
        // The InMemoryStore *does* create it: `self.get_or_create_broadcaster(job_id).await;`
        // Let's adjust to match that for consistency:

        let broadcaster = {
            let mut broadcasters_write_guard = self.bid_broadcasters.write()
                 .map_err(|_| AppError::Internal(anyhow::anyhow!("Bid broadcaster lock poisoned")))?;
            broadcasters_write_guard.entry(*job_id_param).or_insert_with(|| {
                let (tx, _) = broadcast::channel(32);
                tx
            }).clone()
        };

        if broadcaster.send(bid).is_err() {
            // Log this in a real scenario using tracing::debug!
            // e.g., tracing::debug!("Failed to broadcast bid for job {}: no active subscribers?", job_id_param);
        }

        Ok(())
    }

    async fn list_bids(&self, job_id: &Cid) -> Result<Vec<Bid>, AppError> {
        let job_id_str = job_id.to_string();

        #[derive(sqlx::FromRow)]
        struct DbBidRow {
            // job_id_from_db: String, // Renaming to avoid confusion with the Cid::try_from source
            bidder_did: String,
            price: i64, // SQLite INTEGER typically maps to i64
            estimate_json: String,
            reputation_score: Option<f64>,
            // We also need job_id column from bids table to construct the Bid struct if it differs, but usually it's the same one we query by.
            // Let's select it to be explicit, even if it's the same as job_id_str.
            // The Bid struct requires job_id: Cid.
            job_id_col: String, // Naming it job_id_col to make it clear it's from the column
        }

        let rows = sqlx::query_as!(
            DbBidRow,
            r#"
            SELECT job_id as job_id_col, bidder_did, price, estimate_json, reputation_score
            FROM bids
            WHERE job_id = $1
            "#,
            job_id_str
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch bids from database: {}", e)))?;

        let mut bids = Vec::new();
        for row in rows {
            let bid_job_id = Cid::try_from(row.job_id_col.as_str())
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse bid's job_id as Cid: {} for job_id {}", e, row.job_id_col)))?;
            
            let estimate: ResourceEstimate = serde_json::from_str(&row.estimate_json)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to deserialize bid estimate: {}", e)))?;

            // Ensure price is non-negative before converting to u64
            if row.price < 0 {
                return Err(AppError::Internal(anyhow::anyhow!("Invalid negative price found in database for bid by {} on job {}", row.bidder_did, bid_job_id)));
            }

            bids.push(Bid {
                job_id: bid_job_id,
                bidder: icn_identity::Did(row.bidder_did),
                price: row.price as u64, // TokenAmount is u64
                estimate,
                reputation_score: row.reputation_score,
            });
        }

        Ok(bids)
    }

    async fn subscribe_to_bids(&self, job_id: &Cid) -> Result<Option<broadcast::Receiver<Bid>>, AppError> {
        // Placeholder - this will likely remain mostly in-memory logic
        let mut broadcasters = self.bid_broadcasters.write().unwrap(); // Handle potential poison
        let sender = broadcasters.entry(*job_id).or_insert_with(|| {
            let (tx, _) = broadcast::channel(32); // Default capacity
            tx
        });
        Ok(Some(sender.subscribe()))
    }

    async fn assign_job(&self, job_id: &Cid, bidder_did: icn_identity::Did) -> Result<(), AppError> {
        // 1. Fetch the job and check its current status
        let (_job_request, current_status) = self.get_job(job_id).await?
            .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))?;

        // 2. Validate that the job can be assigned
        match current_status {
            JobStatus::Pending | JobStatus::Bidding => {
                // Proceed to update status
            }
            _ => {
                return Err(AppError::BadRequest(format!(
                    "Job {} is in status {:?} and cannot be assigned.",
                    job_id,
                    current_status
                )));
            }
        }

        // 3. Update the job status to Assigned
        self.update_job_status(job_id, JobStatus::Assigned { bidder: bidder_did }).await
    }

    async fn list_jobs_for_worker(&self, worker_did: &icn_identity::Did) -> Result<Vec<(Cid, JobRequest, JobStatus)>, AppError> {
        let worker_did_str = worker_did.0.clone();

        #[derive(sqlx::FromRow)]
        struct WorkerJobRow {
            job_id: String,
            request_json: String,
            status_type: String,
            status_did: Option<String>,
            status_reason: Option<String>,
        }

        let rows = sqlx::query_as!( // Using query_as! directly if the struct fields match column names and types
            WorkerJobRow,
            r#"
            SELECT job_id, request_json, status_type, status_did, status_reason
            FROM jobs
            WHERE (status_type = 'Assigned' AND status_did = $1)
               OR (status_type = 'Running' AND status_did = $1)
            "#,
            worker_did_str
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch jobs for worker from database: {}", e)))?;

        let mut worker_jobs = Vec::new();
        for row in rows {
            let job_cid = Cid::try_from(row.job_id.as_str())
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse job_id as Cid: {}", e)))?;
            
            let job_request: JobRequest = serde_json::from_str(&row.request_json)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to deserialize job request: {}", e)))?;

            let job_status = match row.status_type.as_str() {
                "Pending" => JobStatus::Pending, // Should not occur based on WHERE clause but good to be exhaustive
                "Bidding" => JobStatus::Bidding, // Should not occur
                "Assigned" => {
                    let bidder_did_str = row.status_did.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Missing bidder_did for Assigned status")))?;
                    // We already filtered by worker_did, so this should match.
                    JobStatus::Assigned { bidder: icn_identity::Did(bidder_did_str) }
                }
                "Running" => {
                    let runner_did_str = row.status_did.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Missing runner_did for Running status")))?;
                    // We already filtered by worker_did, so this should match.
                    JobStatus::Running { runner: icn_identity::Did(runner_did_str) }
                }
                "Completed" => JobStatus::Completed, // Should not occur
                "Failed" => { // Should not occur
                    let reason = row.status_reason.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Missing reason for Failed status")))?;
                    JobStatus::Failed { reason }
                }
                _ => return Err(AppError::Internal(anyhow::anyhow!("Unknown job status type '{}' for job {}", row.status_type, row.job_id))),
            };
            worker_jobs.push((job_cid, job_request, job_status));
        }
        Ok(worker_jobs)
    }
} 