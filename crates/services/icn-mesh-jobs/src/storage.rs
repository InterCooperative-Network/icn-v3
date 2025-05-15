use anyhow::Result;
use async_trait::async_trait;
use cid::Cid;
use icn_identity::Did;
use crate::types::{Bid, JobRequest};
use icn_types::mesh::JobStatus;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc; // For Arc<InMemoryStore> if needed directly, but main.rs uses Arc<dyn MeshJobStore>
use tokio::sync::{broadcast, RwLock};
use multihash::{Code, Multihash};
use crate::error::AppError;

// Helper to generate CID for a JobRequest
fn generate_job_cid<T: Serialize>(req: &T) -> Result<Cid, AppError> {
    let serialized = serde_json::to_vec(req).map_err(|e| AppError::Serialization(e.to_string()))?;
    let hash = Sha256::digest(&serialized);
    let mh_result = Code::Sha2_256.digest(&hash);
    let mh = mh_result.map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create multihash for CID: {}", e)))?;
    Ok(Cid::new_v1(0x71, mh))
}

#[async_trait]
pub trait MeshJobStore: Send + Sync {
    /// Create a new job record; returns its CID.
    async fn insert_job(&self, job_request: JobRequest) -> Result<Cid, AppError>;

    /// Fetch a job request + status.
    async fn get_job(&self, job_id: &Cid) -> Result<Option<(JobRequest, JobStatus)>, AppError>;

    /// List all job IDs (optionally filtered by status).
    /// If status is None, list all jobs.
    async fn list_jobs(&self, status_filter: Option<JobStatus>) -> Result<Vec<Cid>, AppError>;
    
    /// Update job status
    async fn update_job_status(&self, job_id: &Cid, new_status: JobStatus) -> Result<(), AppError>;

    /// Store a bid for a given job.
    async fn insert_bid(&self, job_id: &Cid, bid: Bid) -> Result<(), AppError>;

    /// Fetch all bids for a given job.
    async fn list_bids(&self, job_id: &Cid) -> Result<Vec<Bid>, AppError>;

    /// Subscribe to bids for a given job.
    async fn subscribe_to_bids(&self, job_id: &Cid) -> Result<Option<broadcast::Receiver<Bid>>, AppError>;

    /// Assign a job to a bidder
    async fn assign_job(&self, job_id: &Cid, bidder_did: Did) -> Result<(), AppError>;

    /// List all jobs (CID, request, and status) for a specific worker DID (either assigned or running).
    async fn list_jobs_for_worker(&self, worker_did: &Did) -> Result<Vec<(Cid, JobRequest, JobStatus)>, AppError>;
}

// In-memory implementation for testing
pub struct InMemoryStore {
    jobs: Arc<RwLock<HashMap<String, (JobRequest, JobStatus)>>>,
    bids: Arc<RwLock<HashMap<String, Vec<Bid>>>>,
    bid_broadcasters: Arc<RwLock<HashMap<String, broadcast::Sender<Bid>>>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            bids: Arc::new(RwLock::new(HashMap::new())),
            bid_broadcasters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn get_or_create_broadcaster(&self, job_id: &Cid) -> broadcast::Sender<Bid> {
        let mut broadcasters_guard = self.bid_broadcasters.write().await;
        broadcasters_guard
            .entry(job_id.to_string())
            .or_insert_with(|| broadcast::channel(32).0)
            .clone()
    }

    async fn get_bid_receiver(&self, job_id: &Cid) -> Option<broadcast::Receiver<Bid>> {
        let broadcasters_guard = self.bid_broadcasters.read().await;
        broadcasters_guard.get(job_id.to_string().as_str()).map(|sender| sender.subscribe())
    }
}

#[async_trait]
impl MeshJobStore for InMemoryStore {
    async fn insert_job(&self, job_request: JobRequest) -> Result<Cid, AppError> {
        let job_cid = generate_job_cid(&job_request)?;
        let job_id = job_cid.to_string();
        let mut jobs_guard = self.jobs.write().await;
        jobs_guard.insert(job_id, (job_request, JobStatus::InProgress));
        Ok(job_cid)
    }

    async fn get_job(&self, job_id: &Cid) -> Result<Option<(JobRequest, JobStatus)>, AppError> {
        let jobs_guard = self.jobs.read().await;
        Ok(jobs_guard.get(job_id.to_string().as_str()).cloned())
    }

    async fn list_jobs(&self, status_filter: Option<JobStatus>) -> Result<Vec<Cid>, AppError> {
        let jobs_guard = self.jobs.read().await;
        let cids = jobs_guard
            .iter()
            .filter(|(_, (_, status))| {
                status_filter.as_ref().map_or(true, |filter| status == filter)
            })
            .map(|(job_id, _)| Cid::try_from(job_id.as_str()).map_err(|e| AppError::InvalidCid(format!("Stored job_id {} is not a valid CID: {}", job_id, e))))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(cids)
    }

    async fn update_job_status(&self, job_id: &Cid, new_status: JobStatus) -> Result<(), AppError> {
        let mut jobs_guard = self.jobs.write().await;
        if let Some((req, current_status)) = jobs_guard.get_mut(job_id.to_string().as_str()) {
            match (current_status, new_status) {
                (JobStatus::InProgress, _) => {
                    *current_status = new_status;
                    Ok(())
                }
                _ => Err(AppError::InvalidStatusTransition(format!("Invalid status transition from {} to {}", current_status, new_status)))
            }
        } else {
            Err(AppError::NotFound(format!("Job not found: {}", job_id)))
        }
    }

    async fn insert_bid(&self, job_id: &Cid, bid: Bid) -> Result<(), AppError> {
        let job_id_str = job_id.to_string();
        let mut bids_guard = self.bids.write().await;
        let bids = bids_guard.entry(job_id_str.clone()).or_insert_with(Vec::new);
        bids.push(bid.clone());

        let broadcaster = self.get_or_create_broadcaster(job_id).await;
        if broadcaster.send(bid).is_err() {
            tracing::debug!("No active subscribers for bids on job {}", job_id);
        }
        Ok(())
    }

    async fn list_bids(&self, job_id: &Cid) -> Result<Vec<Bid>, AppError> {
        let bids_guard = self.bids.read().await;
        Ok(bids_guard
            .get(job_id.to_string().as_str())
            .cloned()
            .unwrap_or_default())
    }

    async fn subscribe_to_bids(&self, job_id: &Cid) -> Result<Option<broadcast::Receiver<Bid>>, AppError> {
        Ok(self.get_bid_receiver(job_id).await)
    }

    async fn assign_job(&self, job_id: &Cid, bidder_did: Did) -> Result<(), AppError> {
        let mut jobs_guard = self.jobs.write().await;
        if let Some((_, current_status)) = jobs_guard.get_mut(job_id.to_string().as_str()) {
            match current_status {
                JobStatus::InProgress => {
                    *current_status = JobStatus::Assigned { bidder_did };
                    Ok(())
                }
                _ => Err(AppError::InvalidStatusTransition(format!("Job cannot be assigned in current state: {}", current_status)))
            }
        } else {
            Err(AppError::NotFound(format!("Job not found: {}", job_id)))
        }
    }

    async fn list_jobs_for_worker(&self, worker_did: &Did) -> Result<Vec<(Cid, JobRequest, JobStatus)>, AppError> {
        let jobs_guard = self.jobs.read().await;
        let worker_jobs = jobs_guard
            .iter()
            .filter_map(|(job_id_str, (req, status))| {
                match status {
                    JobStatus::Assigned { bidder } if bidder == worker_did => {
                        Cid::try_from(job_id_str.as_str())
                            .map(|cid| (cid, req.clone(), status.clone()))
                            .map_err(|e| AppError::InvalidCid(format!("Stored job_id {} is not a valid CID: {}", job_id_str, e)))
                            .ok()
                    }
                    _ => None,
                }
            })
            .collect();
        Ok(worker_jobs)
    }
} 