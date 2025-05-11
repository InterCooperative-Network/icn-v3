use tokio::sync::mpsc::{self, Receiver, Sender};
use icn_types::mesh::MeshJob;
use crate::protocol::{Bid, MeshProtocolMessage};
use libp::gossipsub::IdentTopic as Topic;
use icn_types::mesh::{JobStatus as StandardJobStatus, ExecutionReceipt};
use icn_types::reputation::{ReputationRecord, ReputationUpdateEvent};
use cid::Cid;
use icn_identity::Did;

#[derive(Debug)]
pub enum NodeCommand {
    AnnounceJob(MeshJob),
    SubmitBid(Bid),
}

pub struct MeshNode {
    pub(crate) local_keypair: IcnKeyPair,
    pub(crate) swarm: Swarm<MeshBehaviour>,
    pub(crate) runtime_job_queue: Arc<Mutex<VecDeque<(MeshJob, Option<libp2p::kad::PeerRecord>)>>>,
    pub(crate) local_runtime_context: Option<Arc<RuntimeContext>>,
    pub announced_originated_jobs: Arc<RwLock<HashMap<IcnJobId, (JobManifest, MeshJob)>>>,
    pub available_jobs_on_mesh: Arc<RwLock<HashMap<IcnJobId, JobManifest>>>,
    pub bids: Arc<RwLock<HashMap<IcnJobId, Vec<Bid>>>>,
    pub assigned_jobs: Arc<RwLock<HashMap<IcnJobId, (JobManifest, Bid)>>>,
    pub assigned_by_originator: Arc<RwLock<HashSet<IcnJobId>>>,
    pub completed_job_receipt_cids: Arc<RwLock<HashMap<IcnJobId, HashSet<Cid>>>>,
    pub(crate) pending_kad_fetches: Arc<RwLock<HashMap<libp2p::kad::QueryId, oneshot::Sender<Result<Vec<u8>, String>>>>>,
    pub(crate) internal_action_tx: Sender<InternalNodeAction>,
    pub http_client: reqwest::Client,
    pub reputation_service_url: Option<String>,
    pub known_receipt_cids: Arc<RwLock<HashMap<Cid, KnownReceiptInfo>>>,
    pub(crate) command_rx: Receiver<NodeCommand>,
    pub test_observed_reputation_submissions: Arc<RwLock<Vec<TestObservedReputationSubmission>>>,
}

#[derive(Clone, Debug)]
pub struct KnownReceiptInfo {
    pub job_id: IcnJobId,
    pub executor_did: Did,
    pub announced_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestObservedReputationSubmission {
    pub job_id: IcnJobId,
    pub executor_did: Did,
    pub outcome: StandardJobStatus,
    pub anchor_cid: Cid,
    pub timestamp: i64,
}

impl MeshNode {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        local_keypair: IcnKeyPair,
        listen_addr_str: Option<String>,
        runtime_job_queue: Arc<Mutex<VecDeque<(MeshJob, Option<libp2p::kad::PeerRecord>)>>>,
        local_runtime_context: Option<Arc<RuntimeContext>>,
        reputation_service_url: Option<String>,
        command_rx: Receiver<NodeCommand>,
    ) -> Result<(Self, Receiver<InternalNodeAction>), anyhow::Error> {
        let (internal_action_tx, internal_action_rx) = mpsc::channel(100);

        let swarm = Swarm::new(...);

        Ok((
            MeshNode {
                local_keypair,
                swarm,
                runtime_job_queue,
                local_runtime_context,
                announced_originated_jobs: Arc::new(RwLock::new(HashMap::new())),
                available_jobs_on_mesh: Arc::new(RwLock::new(HashMap::new())),
                bids: Arc::new(RwLock::new(HashMap::new())),
                assigned_jobs: Arc::new(RwLock::new(HashMap::new())),
                assigned_by_originator: Arc::new(RwLock::new(HashSet::new())),
                completed_job_receipt_cids: Arc::new(RwLock::new(HashMap::new())),
                pending_kad_fetches: Arc::new(RwLock::new(HashMap::new())),
                internal_action_tx: internal_action_tx.clone(),
                http_client: reqwest::Client::new(),
                reputation_service_url,
                known_receipt_cids: Arc::new(RwLock::new(HashMap::new())),
                command_rx,
                test_observed_reputation_submissions: Arc::new(RwLock::new(Vec::new())),
            },
            internal_action_rx,
        ))
    }

    async fn publish_bid_message(&mut self, bid: Bid) -> Result<(), anyhow::Error> {
        tracing::info!("Publishing bid for job_id: {} from executor: {}", bid.job_id, bid.executor_did);
        let topic = Topic::new(format!("job-bids/{}", bid.job_id));
        let message = MeshProtocolMessage::JobBidV1(bid);
        let cbor_payload = serde_cbor::to_vec(&message)?;
        
        self.swarm.behaviour_mut().gossipsub.publish(topic, cbor_payload)?;
        Ok(())
    }

    pub async fn run_event_loop(&mut self, mut internal_action_rx: Receiver<InternalNodeAction>) -> Result<(), anyhow::Error> {
        let mut job_announcement_interval = tokio::time::interval(Duration::from_secs(self.config.job_announcement_interval_secs));
        let mut executor_selection_interval = tokio::time::interval(Duration::from_secs(self.config.executor_selection_interval_secs));
        let mut kad_maintenance_interval = tokio::time::interval(Duration::from_secs(self.config.kad_maintenance_interval_secs));

        loop {
            tokio::select! {
                Some(command) = self.command_rx.recv() => {
                    match command {
                        NodeCommand::AnnounceJob(job) => {
                            tracing::info!("Received AnnounceJob command for job_id: {}", job.job_id);
                            if let Err(e) = self.announce_job(job).await {
                                tracing::error!("Error announcing job from command: {:?}", e);
                            }
                        }
                        NodeCommand::SubmitBid(bid) => {
                            tracing::info!("Received SubmitBid command for job_id: {} by {}", bid.job_id, bid.executor_did);
                            if let Err(e) = self.publish_bid_message(bid).await {
                                tracing::error!("Error submitting bid from command: {:?}", e);
                            }
                        }
                    }
                },

                Some(internal_action) = internal_action_rx.recv() => {
                    if let Err(e) = self.handle_internal_action(internal_action).await {
                        tracing::error!("Error handling internal action: {:?}", e);
                    }
                },

                event = self.swarm.select_next_some() => {
                    if let Err(e) = self.handle_swarm_event(event).await {
                        tracing::error!("Error handling swarm event: {:?}", e);
                    }
                },
                
                _ = job_announcement_interval.tick() => {
                    if let Err(e) = self.process_runtime_job_queue().await {
                        tracing::error!("Error processing runtime job queue: {:?}", e);
                    }
                },

                _ = executor_selection_interval.tick() => {
                    if let Err(e) = self.select_executor_for_originated_jobs().await {
                        tracing::error!("Error in executor selection: {:?}", e);
                    }
                },

                _ = kad_maintenance_interval.tick() => {
                    self.perform_kad_maintenance();
                }
            }
        }
    }

    async fn trigger_reputation_update(
        &mut self,
        job_id: &IcnJobId,
        receipt: &Arc<ExecutionReceipt>,
    ) -> Result<(), anyhow::Error> {
        tracing::info!("Triggering reputation update for job_id: {}, executor: {}", job_id, receipt.executor);
        
        let event_type = match receipt.status {
            StandardJobStatus::Completed | StandardJobStatus::Succeeded => ReputationUpdateEvent::JobCompletedSuccessfully {
                cid: receipt.cid,
                job_id: job_id.clone(),
                worker_did: receipt.executor.clone(),
            },
            StandardJobStatus::Failed => ReputationUpdateEvent::JobFailed {
                cid: receipt.cid,
                job_id: job_id.clone(),
                worker_did: receipt.executor.clone(),
                reason: "Execution reported as failed".to_string(),
            },
            _ => {
                tracing::warn!("Reputation update skipped for job {} due to unhandled status: {:?}", job_id, receipt.status);
                return Ok(());
            }
        };

        let reputation_record = ReputationRecord {
            version: "1.0".to_string(),
            issuer: self.local_keypair.did.clone(),
            subject: receipt.executor.clone(),
            issued_at: Utc::now(),
            event: event_type,
            anchor: Some(receipt.cid),
            expires_at: None,
            signature: None,
        };

        let payload_to_sign = match icn_types::reputation::get_reputation_record_signing_payload(&reputation_record) {
            Ok(payload) => payload,
            Err(e) => {
                tracing::error!("Failed to serialize reputation record for signing: {:?}", e);
                return Err(e.into());
            }
        };
        let signature = self.local_keypair.sign(&payload_to_sign);
        let final_reputation_record = ReputationRecord {
            signature: Some(signature),
            ..reputation_record
        };

        if let Some(url) = &self.reputation_service_url {
            let client = self.http_client.clone();
            let url_str = url.clone();
            let record_to_send = final_reputation_record.clone();
            
            match client.post(&url_str).json(&record_to_send).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        tracing::info!("Successfully submitted reputation record for job_id: {} to {}", job_id, url_str);
                    } else {
                        tracing::warn!("Reputation service returned error for job_id: {}: {} - {}", job_id, response.status(), response.text().await.unwrap_or_default());
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to send reputation record for job_id: {}: {:?}", job_id, e);
                }
            }
        } else {
            tracing::warn!("No reputation_service_url configured. Skipping HTTP submission for job_id: {}", job_id);
        }

        let test_submission = TestObservedReputationSubmission {
            job_id: job_id.clone(),
            executor_did: receipt.executor.clone(),
            outcome: receipt.status.clone(),
            anchor_cid: receipt.cid,
            timestamp: receipt.timestamp,
        };

        if let Err(e) = self.internal_action_tx.send(InternalNodeAction::ReputationSubmittedForTest(test_submission)).await {
            tracing::warn!("Failed to send ReputationSubmittedForTest internal action for job_id: {}: {:?}", job_id, e);
        }
        
        Ok(())
    }

    async fn handle_internal_action(&mut self, action: InternalNodeAction) -> Result<(), anyhow::Error> {
        match action {
            InternalNodeAction::ReputationSubmittedForTest(submission_data) => {
                tracing::debug!("Test: Recording observed reputation submission: {:?}", submission_data);
                self.test_observed_reputation_submissions.write().unwrap().push(submission_data);
            }
            _ => {
                tracing::trace!("Unhandled or placeholder internal action: {:?}", action);
            }
        }
        Ok(())
    }
} 