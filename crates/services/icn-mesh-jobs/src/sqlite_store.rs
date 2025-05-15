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
use icn_types::jobs::JobStatus;

use crate::storage::{MeshJobStore, generate_job_cid};
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
    async fn insert_job(&self, job_request: JobRequest) -> Result<Cid, AppError> {
        let job_cid = generate_job_cid(&job_request)?;
        let job_id_str = job_cid.to_string();
        let owner_did_str = job_request.originator.to_string();
        let requirements_json = serde_json::to_string(&job_request.params)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize job params: {}", e)))?;
        let status_type = "Pending";
        
        sqlx::query!(
            r#"
            INSERT INTO jobs (job_id, owner_did, requirements_json, status_type)
            VALUES ($1, $2, $3, $4) returning id
            "#,
            job_id_str,
            owner_did_str,
            requirements_json,
            status_type
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| AppError::Database(e))?;
        
        Ok(job_cid)
    }

    async fn get_job(&self, job_id: &Cid) -> Result<Option<(JobRequest, JobStatus)>, AppError> {
        #[derive(sqlx::FromRow)]
        struct JobRow {
            owner_did: String,
            requirements_json: String,
            status_type: String,
            status_bidder_did: Option<String>,
            status_node_id: Option<String>,
            status_result_cid: Option<String>,
            status_error_message: Option<String>,
        }

        let job_id_str = job_id.to_string();
        let job_row_opt = sqlx::query_as!(
            JobRow,
            r#"
            SELECT owner_did, requirements_json, status_type, 
                   status_bidder_did, status_node_id, status_result_cid, status_error_message
            FROM jobs
            WHERE job_id = $1
            "#,
            job_id_str
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(AppError::from)?;

        match job_row_opt {
            Some(row) => {
                let params: icn_types::mesh::MeshJobParams = serde_json::from_str(&row.requirements_json)
                    .map_err(|e| AppError::Serialization(format!("Failed to deserialize job params for job {}: {}", job_id, e)))?;
                
                let originator_did = Did::from_str(&row.owner_did)
                    .map_err(|e| AppError::InvalidInput(format!("Stored owner_did {} is not a valid DID: {}", row.owner_did, e)))?;

                let job_request = JobRequest {
                    job_id: job_id.clone(),
                    params,
                    originator: originator_did,
                    execution_policy: None,
                };

                let job_status = JobStatus::from_db_fields(
                    &row.status_type,
                    row.status_bidder_did.as_deref(),
                    row.status_node_id.as_deref(),
                    row.status_result_cid.as_deref(),
                    row.status_error_message.as_deref(),
                ).map_err(|e_str| AppError::Internal(anyhow::anyhow!("Invalid job status in DB for job {}: {}", job_id, e_str)))?;

                Ok(Some((job_request, job_status)))
            }
            None => Ok(None),
        }
    }

    async fn list_jobs(&self, status_filter: Option<JobStatus>) -> Result<Vec<Cid>, AppError> {
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

        let rows = query_builder.build_query_scalar().fetch_all(&*self.pool).await?;

        let cids = rows.into_iter()
            .map(|job_id_str: String| Cid::try_from(job_id_str.as_str()).map_err(|e| AppError::InvalidCid(format!("Invalid job_id {} from DB: {}", job_id_str, e))))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(cids)
    }

    async fn update_job_status(&self, job_id: &Cid, new_status: JobStatus) -> Result<(), AppError> {
        let job_id_str = job_id.to_string();
        let (status_type, bidder_did, node_id, result_cid, error_message) = new_status.to_db_fields();

        let result = sqlx::query!(
            r#"
            UPDATE jobs
            SET status_type = $1, status_bidder_did = $2, status_node_id = $3, 
                status_result_cid = $4, status_error_message = $5
            WHERE job_id = $6
            "#,
            status_type,
            bidder_did,
            node_id,
            result_cid,
            error_message,
            job_id_str
        )
        .execute(&*self.pool)
        .await?;

        if result.rows_affected() == 0 {
            Err(AppError::NotFound(format!("Job with ID {} not found for status update", job_id)))
        } else {
            Ok(())
        }
    }

    async fn insert_bid(&self, job_id: &Cid, bid: Bid) -> Result<(), AppError> {
        let job_id_str = job_id.to_string();
        let bidder_did_str = bid.bidder.to_string();
        let resources_json = serde_json::to_string(&bid.data)
            .map_err(|e| AppError::Serialization(format!("Failed to serialize bid data: {}", e)))?;
        let price = bid.price_atto_icn as i64;

        sqlx::query!(
            r#"
            INSERT INTO bids (job_id, bidder_did, price, resources_json)
            VALUES ($1, $2, $3, $4) returning id
            "#,
            job_id_str,
            bidder_did_str,
            price,
            resources_json
        )
        .fetch_one(&*self.pool)
        .await?;

        if let Some(sender) = self.bid_broadcasters.read().unwrap().get(&job_id_str) {
            if sender.send(bid.clone()).is_err() {
                tracing::debug!("No active subscribers for bids on job {}", job_id);
            }
        }
        Ok(())
    }

    async fn list_bids(&self, job_id: &Cid) -> Result<Vec<Bid>, AppError> {
        let job_id_str = job_id.to_string();
        #[derive(sqlx::FromRow, Debug)]
        struct DbBidRow {
            id: i64,
            bidder_did: String,
            price: i64,
            resources_json: String,
            reputation_score: Option<f64>,
        }

        let bid_rows = sqlx::query_as!(DbBidRow, "SELECT id, bidder_did, price, resources_json, reputation_score FROM bids WHERE job_id = $1", job_id_str)
            .fetch_all(&*self.pool)
            .await?;

        let bids = bid_rows.into_iter().map(|row| {
            let bidder = Did::from_str(&row.bidder_did)
                .map_err(|e| AppError::InvalidInput(format!("Stored bidder_did {} is not a valid DID: {}", row.bidder_did, e)))?;
            let data : icn_types::jobs::ResourceEstimate = serde_json::from_str(&row.resources_json)
                 .map_err(|e| AppError::Serialization(format!("Failed to deserialize bid data for bid_id {}: {}", row.id, e)))?;
            Ok(Bid {
                id: Some(row.id as u64),
                job_id: job_id.clone(),
                bidder,
                price_atto_icn: row.price as u64,
                data,
                reputation_score: row.reputation_score,
            })
        }).collect();

        Ok(bids)
    }

    async fn subscribe_to_bids(&self, job_id: &Cid) -> Result<Option<broadcast::Receiver<Bid>>, AppError> {
        let job_id_str = job_id.to_string();
        if self.bid_broadcasters.read().unwrap().contains_key(&job_id_str) {
             Ok(Some(self.bid_broadcasters.read().unwrap().get(&job_id_str).unwrap().subscribe()))
        } else {
            Ok(None) 
        }
    }

    async fn assign_job(&self, job_id: &Cid, bidder_did: Did) -> Result<(), AppError> {
        self.update_job_status(job_id, JobStatus::Assigned { bidder: bidder_did }).await
    }

    async fn list_jobs_for_worker(&self, worker_did: &Did) -> Result<Vec<(Cid, JobRequest, JobStatus)>, AppError> {
        let worker_did_str = worker_did.to_string();

        #[derive(sqlx::FromRow)]
        struct WorkerJobRow {
            job_id: String,
            owner_did: String,
            cid: String,
            requirements_json: String,
            status_type: String,
            status_bidder_did: Option<String>,
            status_node_id: Option<String>,
            status_result_cid: Option<String>,
            status_error_message: Option<String>,
        }

        let rows = sqlx::query_as!(
            WorkerJobRow,
            r#"
            SELECT job_id, owner_did, cid, requirements_json, status_type, status_bidder_did, status_node_id, status_result_cid, status_error_message
            FROM jobs
            WHERE status_type = 'Assigned' AND status_bidder_did = $1
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
                .map_err(|e| AppError::Serialization(format!("Failed to deserialize requirements for job {}: {}", row.job_id, e)))?;
            
            let owner_did = Did::new_ed25519(row.owner_did);
            let cid = Cid::try_from(row.cid.as_str())
                .map_err(|e| AppError::InvalidCid(format!("Failed to parse CID for job {}: {}", row.job_id, e)))?;
            let job_cid = Cid::try_from(row.job_id.as_str())
                .map_err(|e| AppError::InvalidCid(format!("Failed to parse job CID: {}", e)))?;

            let job_request = JobRequest {
                id: row.job_id,
                owner_did,
                cid,
                requirements,
            };

            let job_status = JobStatus::from_db_fields(
                &row.status_type,
                row.status_bidder_did.as_deref(),
                row.status_node_id.as_deref(),
                row.status_result_cid.as_deref(),
                row.status_error_message.as_deref(),
            ).map_err(|e_str| AppError::Internal(anyhow::anyhow!("Invalid job status in DB for job {}: {}", job_id, e_str)))?;

            worker_jobs.push((job_cid, job_request, job_status));
        }
        Ok(worker_jobs)
    }
} 