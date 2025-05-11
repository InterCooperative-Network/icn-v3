use tokio::sync::mpsc::{self, Receiver, Sender};
use icn_types::mesh::MeshJob;
use crate::protocol::{Bid, MeshProtocolMessage};
use libp::gossipsub::IdentTopic as Topic;

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
}

#[derive(Clone, Debug)]
pub struct KnownReceiptInfo {
    pub job_id: IcnJobId,
    pub executor_did: Did,
    pub announced_at: i64,
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
} 