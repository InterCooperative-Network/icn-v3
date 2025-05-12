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

// Helper to generate CID for a JobRequest
// This is a basic implementation. In a production system, you'd use a canonical serialization format.
fn generate_job_cid<T: Serialize>(req: &T) -> Result<Cid> {
    let serialized = serde_json::to_vec(req)?;
    let hash = Sha256::digest(&serialized);
    let mh = Code::Sha2_256.digest(&hash);
    Ok(Cid::new_v1(0x71, mh))
}

#[async_trait]
pub trait MeshJobStore: Send + Sync {
    /// Create a new job record; returns its CID.
    async fn insert_job(&self, job_request: JobRequest) -> Result<Cid>;

    /// Fetch a job request + status.
    async fn get_job(&self, job_id: &Cid) -> Result<Option<(JobRequest, JobStatus)>>;

    /// List all job IDs (optionally filtered by status).
    /// If status is None, list all jobs.
    async fn list_jobs(&self, status_filter: Option<JobStatus>) -> Result<Vec<Cid>>;
    
    /// Update job status
    async fn update_job_status(&self, job_id: &Cid, new_status: JobStatus) -> Result<()>;

    /// Store a bid for a given job.
    async fn insert_bid(&self, job_id: &Cid, bid: Bid) -> Result<()>;

    /// Fetch all bids for a given job.
    async fn list_bids(&self, job_id: &Cid) -> Result<Vec<Bid>>;

    /// Subscribe to bids for a given job.
    async fn subscribe_to_bids(&self, job_id: &Cid) -> Result<Option<broadcast::Receiver<Bid>>>;

    /// Assign a job to a bidder
    async fn assign_job(&self, job_id: &Cid, bidder_did: Did) -> Result<()>;

    /// List all jobs (CID, request, and status) for a specific worker DID (either assigned or running).
    async fn list_jobs_for_worker(&self, worker_did: &Did) -> Result<Vec<(Cid, JobRequest, JobStatus)>>;
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
    async fn insert_job(&self, job_request: JobRequest) -> Result<Cid> {
        let job_cid = generate_job_cid(&job_request)?;
        let job_id = job_cid.to_string();
        let mut jobs_guard = self.jobs.write().await;
        jobs_guard.insert(job_id, (job_request, JobStatus::InProgress));
        Ok(job_cid)
    }

    async fn get_job(&self, job_id: &Cid) -> Result<Option<(JobRequest, JobStatus)>> {
        let jobs_guard = self.jobs.read().await;
        Ok(jobs_guard.get(job_id.to_string().as_str()).cloned())
    }

    async fn list_jobs(&self, status_filter: Option<JobStatus>) -> Result<Vec<Cid>> {
        let jobs_guard = self.jobs.read().await;
        let cids = jobs_guard
            .iter()
            .filter(|(_, (_, status))| {
                status_filter.as_ref().map_or(true, |filter| status == filter)
            })
            .map(|(job_id, _)| Cid::try_from(job_id.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(cids)
    }

    async fn update_job_status(&self, job_id: &Cid, new_status: JobStatus) -> Result<()> {
        let mut jobs_guard = self.jobs.write().await;
        if let Some((req, current_status)) = jobs_guard.get_mut(job_id.to_string().as_str()) {
            match (current_status, new_status) {
                (JobStatus::InProgress, _) => {
                    *current_status = new_status;
                    Ok(())
                }
                _ => Err(anyhow::anyhow!("Invalid status transition"))
            }
        } else {
            Err(anyhow::anyhow!("Job not found"))
        }
    }

    async fn insert_bid(&self, job_id: &Cid, bid: Bid) -> Result<()> {
        let job_id_str = job_id.to_string();
        let mut bids_guard = self.bids.write().await;
        let bids = bids_guard.entry(job_id_str.clone()).or_insert_with(Vec::new);
        bids.push(bid.clone());

        // Broadcast the bid to any subscribers
        let broadcaster = self.get_or_create_broadcaster(job_id).await;
        if let Err(e) = broadcaster.send(bid) {
            tracing::debug!("Failed to broadcast bid for job {}: {}, no active subscribers?", job_id, e);
        }
        Ok(())
    }

    async fn list_bids(&self, job_id: &Cid) -> Result<Vec<Bid>> {
        let bids_guard = self.bids.read().await;
        Ok(bids_guard
            .get(job_id.to_string().as_str())
            .cloned()
            .unwrap_or_default())
    }

    async fn subscribe_to_bids(&self, job_id: &Cid) -> Result<Option<broadcast::Receiver<Bid>>> {
        Ok(self.get_bid_receiver(job_id).await)
    }

    async fn assign_job(&self, job_id: &Cid, bidder_did: Did) -> Result<()> {
        let mut jobs_guard = self.jobs.write().await;
        if let Some((_, current_status)) = jobs_guard.get_mut(job_id.to_string().as_str()) {
            match current_status {
                JobStatus::InProgress => {
                    *current_status = JobStatus::InProgress;
                    Ok(())
                }
                _ => Err(anyhow::anyhow!("Job cannot be assigned in current state"))
            }
        } else {
            Err(anyhow::anyhow!("Job not found"))
        }
    }

    async fn list_jobs_for_worker(&self, worker_did: &Did) -> Result<Vec<(Cid, JobRequest, JobStatus)>> {
        let jobs_guard = self.jobs.read().await;
        let worker_jobs = jobs_guard
            .iter()
            .filter(|(_, (_, status))| matches!(status, JobStatus::InProgress))
            .map(|(job_id, (req, status))| {
                Ok((
                    Cid::try_from(job_id.as_str())?,
                    req.clone(),
                    status.clone(),
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(worker_jobs)
    }
} 