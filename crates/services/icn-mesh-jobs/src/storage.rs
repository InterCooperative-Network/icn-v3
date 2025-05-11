use anyhow::Result;
use async_trait::async_trait;
use cid::Cid;
use icn_identity::Did;
use icn_types::jobs::{JobRequest, JobStatus, Bid, ResourceRequirements, ResourceEstimate}; // Added ResourceRequirements, ResourceEstimate for default
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc; // For Arc<InMemoryStore> if needed directly, but main.rs uses Arc<dyn MeshJobStore>
use tokio::sync::{broadcast, RwLock};
use multihash::{Code, Multihash};

// Helper to generate CID for a JobRequest
// This is a basic implementation. In a production system, you'd use a canonical serialization format.
fn generate_job_cid<T: Serialize>(req: &T) -> Result<Cid> {
    let bytes = serde_json::to_vec(req)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash_bytes = hasher.finalize();
    let multihash = Multihash::wrap(Code::Sha2_256.into(), &hash_bytes)?;
    Ok(Cid::new_v1(0x55, multihash)) // 0x55 is raw binary
}

#[async_trait]
pub trait MeshJobStore: Send + Sync + 'static {
    /// Create a new job record; returns its CID.
    async fn insert_job(&self, req: JobRequest) -> Result<Cid>;

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

pub struct InMemoryStore {
    jobs: RwLock<HashMap<Cid, (JobRequest, JobStatus)>>,
    bids: RwLock<HashMap<Cid, Vec<Bid>>>,
    bid_broadcasters: RwLock<HashMap<Cid, broadcast::Sender<Bid>>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
            bids: RwLock::new(HashMap::new()),
            bid_broadcasters: RwLock::new(HashMap::new()),
        }
    }

    async fn get_or_create_broadcaster(&self, job_id: &Cid) -> broadcast::Sender<Bid> {
        let mut broadcasters_guard = self.bid_broadcasters.write().await;
        broadcasters_guard
            .entry(*job_id)
            .or_insert_with(|| broadcast::channel(32).0)
            .clone()
    }

    async fn get_bid_receiver(&self, job_id: &Cid) -> Option<broadcast::Receiver<Bid>> {
        let broadcasters_guard = self.bid_broadcasters.read().await;
        broadcasters_guard.get(job_id).map(|sender| sender.subscribe())
    }
}

#[async_trait]
impl MeshJobStore for InMemoryStore {
    async fn insert_job(&self, req: JobRequest) -> Result<Cid> {
        let job_cid = generate_job_cid(&req)?;
        let mut jobs_guard = self.jobs.write().await;
        if jobs_guard.contains_key(&job_cid) {
            // Or update, or return error, depending on desired semantics
            return Err(anyhow::anyhow!("Job with this CID already exists")); 
        }
        // New jobs start in Pending state
        jobs_guard.insert(job_cid, (req, JobStatus::Pending));
        Ok(job_cid)
    }

    async fn get_job(&self, job_id: &Cid) -> Result<Option<(JobRequest, JobStatus)>> {
        let jobs_guard = self.jobs.read().await;
        Ok(jobs_guard.get(job_id).cloned())
    }

    async fn list_jobs(&self, status_filter: Option<JobStatus>) -> Result<Vec<Cid>> {
        let jobs_guard = self.jobs.read().await;
        let cids = jobs_guard
            .iter()
            .filter(|(_, (_, status))| {
                status_filter.as_ref().map_or(true, |f| f == status)
            })
            .map(|(cid, _)| *cid)
            .collect();
        Ok(cids)
    }

    async fn update_job_status(&self, job_id: &Cid, new_status: JobStatus) -> Result<()> {
        let mut jobs_guard = self.jobs.write().await;
        if let Some((_req, status)) = jobs_guard.get_mut(job_id) {
            *status = new_status;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Job not found: {}", job_id))
        }
    }

    async fn insert_bid(&self, job_id: &Cid, bid: Bid) -> Result<()> {
        let jobs_guard = self.jobs.read().await;
        match jobs_guard.get(job_id) {
            Some((_, JobStatus::Pending)) | Some((_, JobStatus::Bidding)) => {
                drop(jobs_guard);
                let mut bids_guard = self.bids.write().await;
                bids_guard.entry(*job_id).or_default().push(bid.clone());
                
                let broadcaster = self.get_or_create_broadcaster(job_id).await;
                if let Err(e) = broadcaster.send(bid) {
                    tracing::debug!("Failed to broadcast bid for job {}: {}, no active subscribers?", job_id, e);
                }
                Ok(())
            }
            Some((_, other_status)) => Err(anyhow::anyhow!(
                "Job {} is in status {:?} and cannot accept bids",
                job_id,
                other_status
            )),
            None => Err(anyhow::anyhow!("Job not found: {}", job_id)),
        }
    }

    async fn list_bids(&self, job_id: &Cid) -> Result<Vec<Bid>> {
        let bids_guard = self.bids.read().await;
        Ok(bids_guard.get(job_id).cloned().unwrap_or_default())
    }

    async fn subscribe_to_bids(&self, job_id: &Cid) -> Result<Option<broadcast::Receiver<Bid>>> {
        Ok(self.get_bid_receiver(job_id).await)
    }

    async fn assign_job(&self, job_id: &Cid, bidder_did: Did) -> Result<()> {
        let mut jobs_guard = self.jobs.write().await;
        match jobs_guard.get_mut(job_id) {
            Some((_job_request, current_status)) => {
                // Ensure job is in a state that can be assigned (e.g., Bidding or Pending)
                match current_status {
                    JobStatus::Pending | JobStatus::Bidding => {
                        *current_status = JobStatus::Assigned { bidder: bidder_did };
                        Ok(())
                    }
                    _ => Err(anyhow::anyhow!(
                        "Job {} is in status {:?} and cannot be assigned.",
                        job_id,
                        current_status
                    )),
                }
            }
            None => Err(anyhow::anyhow!("Job not found: {}", job_id)),
        }
    }

    async fn list_jobs_for_worker(&self, worker_did: &Did) -> Result<Vec<(Cid, JobRequest, JobStatus)>> {
        let jobs_guard = self.jobs.read().await;
        let mut worker_jobs = Vec::new();

        for (cid, (job_req, status)) in jobs_guard.iter() {
            let is_assigned_to_worker = match status {
                JobStatus::Assigned { bidder } => bidder == worker_did,
                JobStatus::Running { runner } => runner == worker_did,
                _ => false,
            };

            if is_assigned_to_worker {
                worker_jobs.push((*cid, job_req.clone(), status.clone()));
            }
        }
        Ok(worker_jobs)
    }
} 