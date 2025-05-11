use crate::behaviour::{MeshBehaviour, MeshBehaviourEvent, CAPABILITY_TOPIC, JOB_ANNOUNCEMENT_TOPIC, RECEIPT_AVAILABILITY_TOPIC_HASH};
use crate::protocol::{MeshProtocolMessage, NodeCapability};
use futures::StreamExt;
use icn_identity::{Did, KeyPair as IcnKeyPair};
use libp2p::gossipsub::IdentTopic as Topic;
use libp2p::identity::{Keypair as Libp2pKeypair, ed25519::SecretKey as Libp2pSecretKey};
use libp2p::swarm::{Swarm, SwarmEvent};
use libp2p::{PeerId, Transport};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::time;
use icn_economics::ResourceType;
use icn_types::mesh::{MeshJob, MeshJobParams, QoSProfile, JobId as IcnJobId, JobStatus as StandardJobStatus, OrganizationScopeIdentifier};
use icn_mesh_receipts::{ExecutionReceipt, sign_receipt_in_place, ReceiptError, SignError as ReceiptSignError, DagNode};
use icn_types::reputation::{ReputationProfile, ReputationRecord, ReputationUpdateEvent}; // Added Reputation types
use cid::Cid; // For storing receipt CIDs
use chrono::{TimeZone, Utc}; // For timestamp conversion
use tokio::sync::mpsc; // <<< ADD IMPORT FOR MPSC

// Access to RuntimeContext for anchoring receipts locally
use icn_runtime::context::RuntimeContext; 
use icn_runtime::execute_mesh_job; // <<< ADD IMPORT
use icn_runtime::host_environment::ConcreteHostEnvironment; // For calling anchor_receipt

use libp2p::gossipsub::TopicHash;
use libp2p::gossipsub::{GossipsubEvent, GossipsubMessage, IdentTopic as Topic, MessageAuthenticity, ValidationMode};
use libp2p::swarm::{Swarm, SwarmEvent};
use libp2p::{Multiaddr, PeerId};
// ADDITION: For the test listener channel
use tokio::sync::broadcast as tokio_broadcast;
use libp2p::kad::{store::MemoryStore, Kademlia, KademliaEvent, QueryResult, GetRecordOk, Record, Key as KadKey, QueryId};
use icn_mesh_receipts::{verify_embedded_signature}; // Ensure verify_embedded_signature is imported
use serde_cbor; // For deserializing the receipt CBOR
use tokio::sync::oneshot; // Added for Kademlia query response

// If reqwest is added as a dependency for submitting reputation records
use reqwest;

// Helper to create job-specific interest topic strings
fn job_interest_topic_string(job_id: &IcnJobId) -> String {
    format!("/icn/mesh/jobs/{}/interest/v1", job_id)
}

// 1. Define the internal action enum
#[derive(Debug)]
enum NodeInternalAction {
    AnnounceReceipt {
        job_id: IcnJobId,
        receipt_cid: Cid,
        executor_did: Did,
    },
}

// Define a simple error type for fetching
#[derive(Debug, Error)]
enum FetchError {
    #[error("Kademlia: Record not found for CID {0}")]
    KadRecordNotFound(Cid),
    #[error("Kademlia: GetRecord query failed for CID {0} with error: {1}")]
    KadQueryError(Cid, String),
    #[error("Kademlia: GetRecord query timed out for CID {0}")]
    KadQueryTimeout(Cid),
    #[error("CBOR deserialization error: {0}")]
    CborDeserialization(String),
    #[error("Signature verification failed: {0}")]
    SignatureVerification(String),
}

#[derive(Clone)]
pub struct MeshNode {
    swarm: Swarm<MeshBehaviour>,
    local_peer_id: PeerId,
    local_node_did: Did,
    local_keypair: IcnKeyPair, // Store keypair for signing receipts
    capability_gossip_topic: Topic,
    job_announcement_topic: Topic,
    receipt_announcement_topic: Topic,
    job_interest_base_topic_prefix: String,
    pub available_jobs_on_mesh: Arc<RwLock<HashMap<IcnJobId, MeshJob>>>,
    pub runtime_job_queue: Arc<Mutex<VecDeque<MeshJob>>>,
    pub job_interests_received: Arc<RwLock<HashMap<IcnJobId, Vec<Did>>>>,
    pub announced_originated_jobs: Arc<RwLock<HashMap<IcnJobId, super::JobManifest>>>,
    pub assigned_jobs: Arc<RwLock<HashMap<IcnJobId, MeshJob>>>,
    pub executing_jobs: Arc<RwLock<HashMap<IcnJobId, super::JobManifest>>>,
    pub completed_job_receipt_cids: Arc<RwLock<HashMap<IcnJobId, Cid>>>,
    pub local_runtime_context: Option<Arc<RuntimeContext>>,
    pub discovered_receipt_announcements: Arc<RwLock<HashMap<IcnJobId, (Cid, Did)>>>,
    // ADDITION: Test hook for listening to JobStatusUpdateV1 messages received by this node
    pub test_job_status_listener_tx: Option<tokio_broadcast::Sender<super::protocol::MeshProtocolMessage>>,
    // 2. Add to MeshNode struct:
    pub internal_action_tx: mpsc::Sender<NodeInternalAction>,
    // For Kademlia receipt queries
    #[allow(clippy::type_complexity)] // To allow complex type for HashMap value with oneshot sender
    receipt_queries: Arc<Mutex<HashMap<QueryId, oneshot::Sender<Result<Vec<u8>, FetchError>>>>,
    reputation_service_url: Option<String>, // Added for reputation service URL
    http_client: reqwest::Client, // Added http_client
    pub bids: Arc<RwLock<HashMap<IcnJobId, Vec<crate::protocol::Bid>>>>, // Added for storing bids
}

impl MeshNode {
    pub async fn new(
        icn_keypair: IcnKeyPair,
        listen_address_str: Option<String>,
        runtime_job_queue: Arc<Mutex<VecDeque<MeshJob>>>,
        local_runtime_context: Option<Arc<RuntimeContext>>,
        // ADDITION: Test listener sender parameter
        test_job_status_listener_tx: Option<tokio_broadcast::Sender<super::protocol::MeshProtocolMessage>>,
        reputation_service_url: Option<String>, // Added parameter
    ) -> Result<(Self, mpsc::Receiver<NodeInternalAction>), Box<dyn Error>> {
        let local_libp2p_keypair = libp2p::identity::Keypair::generate_ed25519(); // Or convert from IcnKeyPair if compatible
        let local_peer_id = PeerId::from(local_libp2p_keypair.public());
        tracing::info!("Local Peer ID: {}", local_peer_id);
        let local_node_did = icn_keypair.did.clone();
        tracing::info!("Local Node DID (from ICN KeyPair): {}", local_node_did);

        let (internal_action_tx, internal_action_rx_for_event_loop) = mpsc::channel::<NodeInternalAction>(32);

        let transport = libp2p::development_transport(local_libp2p_keypair.clone()).await?;

        let mut gossipsub_config = gossipsub::GossipsubConfigBuilder::default();
        gossipsub_config.validation_mode(ValidationMode::Strict);
        let gossipsub_config = gossipsub_config.build()
            .map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut behaviour = MeshBehaviour {
            gossipsub: Gossipsub::new(MessageAuthenticity::Signed(local_libp2p_keypair.clone()), gossipsub_config)
                .map_err(|e| Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)))?,
            kademlia: Kademlia::new(local_peer_id, MemoryStore::new(local_peer_id)),
            mdns: Mdns::new(MdnsConfig::default()).await?,
        };

        let capability_gossip_topic = Topic::new(CAPABILITY_TOPIC);
        behaviour.gossipsub.subscribe(&capability_gossip_topic)?;
        let job_announcement_topic = Topic::new(JOB_ANNOUNCEMENT_TOPIC);
        behaviour.gossipsub.subscribe(&job_announcement_topic)?;
        let receipt_announcement_topic = Topic::new(RECEIPT_AVAILABILITY_TOPIC_HASH);
        behaviour.gossipsub.subscribe(&receipt_announcement_topic)?;
        let job_interest_base_topic_prefix = JOB_INTEREST_TOPIC_PREFIX.to_string();

        let mut swarm = Swarm::new(transport, behaviour, local_peer_id);
        if let Some(addr_str) = listen_address_str {
            let listen_address: Multiaddr = addr_str.parse()?;
            swarm.listen_on(listen_address.clone())?;
            tracing::info!("Listening on specified address: {}", addr_str);
        } else {
            swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
            swarm.listen_on("/ip6/::/tcp/0".parse()?)?;
            tracing::info!("Listening on default TCP IPv4 and IPv6 any port / any interface.");
        }

        Ok((Self {
            swarm,
            local_peer_id,
            local_node_did,
            local_keypair: icn_keypair,
            capability_gossip_topic,
            job_announcement_topic,
            receipt_announcement_topic,
            job_interest_base_topic_prefix,
            available_jobs_on_mesh: Arc::new(RwLock::new(HashMap::new())),
            runtime_job_queue,
            job_interests_received: Arc::new(RwLock::new(HashMap::new())),
            announced_originated_jobs: Arc::new(RwLock::new(HashMap::new())),
            assigned_jobs: Arc::new(RwLock::new(HashMap::new())),
            executing_jobs: Arc::new(RwLock::new(HashMap::new())),
            completed_job_receipt_cids: Arc::new(RwLock::new(HashMap::new())),
            local_runtime_context,
            discovered_receipt_announcements: Arc::new(RwLock::new(HashMap::new())),
            // ADDITION: Store the test listener sender
            test_job_status_listener_tx,
            // Assign `internal_action_tx` to the struct
            internal_action_tx,
            // For Kademlia receipt queries
            #[allow(clippy::type_complexity)] // To allow complex type for HashMap value with oneshot sender
            receipt_queries: Arc::new(Mutex::new(HashMap::new())),
            reputation_service_url, // Store the URL
            http_client: reqwest::Client::new(), // Initialize the client
            bids: Arc::new(RwLock::new(HashMap::new())), // Initialize bids
        }, internal_action_rx_for_event_loop))
    }

    fn construct_capability(&self) -> NodeCapability {
        // For now, use mock/static data. In a real node, this would be dynamic.
        let mut available_resources = HashMap::new();
        available_resources.insert(ResourceType::Cpu, 4000); // e.g., 4 cores * 1000 factor
        available_resources.insert(ResourceType::Memory, 8192); // 8GB RAM

        NodeCapability {
            node_did: self.local_node_did.clone(),
            available_resources,
            supported_wasm_engines: vec!["wasmtime_v0.53".to_string()],
            current_load_factor: 0.1, // Mock load
            reputation_score: Some(1000), // Mock reputation
            geographical_region: Some("local-dev-machine".to_string()),
            custom_features: HashMap::new(),
        }
    }

    async fn broadcast_capabilities(&mut self) -> Result<(), libp2p::gossipsub::PublishError> {
        let capability = self.construct_capability();
        let message = MeshProtocolMessage::CapabilityAdvertisementV1(capability);
        
        match serde_cbor::to_vec(&message) {
            Ok(serialized_message) => {
                println!("Broadcasting capabilities for PeerID: {}...", self.local_peer_id);
                self.swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(self.capability_gossip_topic.clone(), serialized_message)?;
            }
            Err(e) => {
                eprintln!("Error serializing capability message: {:?}", e);
            }
        }
        Ok(())
    }

    pub async fn announce_job(&mut self, job: MeshJob) -> Result<(), Box<dyn Error>> {
        // Create a JobManifest from the MeshJob
        // This is a simplified conversion; a real one would need more robust parsing and default handling.
        let compute_requirements = serde_json::from_str::<super::ComputeRequirements>(&job.params.required_resources_json)
            .unwrap_or_else(|e| {
                eprintln!(
                    "Failed to parse required_resources_json for job {}: {}. Using default requirements.",
                    job.job_id,
                    e
                );
                // Provide some default ComputeRequirements
                super::ComputeRequirements {
                    min_memory_mb: 0,
                    min_cpu_cores: 0,
                    min_storage_mb: 0,
                    max_execution_time_secs: job.params.max_execution_time_secs.unwrap_or(300), // Default from MeshJob or a const
                    required_features: Vec::new(),
                }
            });

        let manifest = super::JobManifest {
            id: job.job_id.clone(),
            submitter_did: job.originator_did.clone(),
            description: job.params.description.clone().unwrap_or_else(|| "N/A".to_string()),
            created_at: chrono::Utc::now(), // Or convert from job.submitted_at if it exists and types match
            expires_at: None, // MeshJob doesn't have this directly
            wasm_cid: job.params.wasm_cid.clone(),
            ccl_cid: job.params.ccl_cid.clone(),
            input_data_cid: job.params.input_data_cid.clone(),
            output_location: job.params.output_location.clone(),
            requirements: compute_requirements,
            priority: super::JobPriority::Medium, // Default priority
            resource_token: icn_economics::ScopedResourceToken::default(), // Placeholder default
            trust_requirements: job.params.trust_requirements.clone(),
            status: super::JobStatus::Submitted, // Initial status for a newly announced job
        };

        let message = MeshProtocolMessage::JobAnnouncementV1(job.clone()); // Network message still uses MeshJob
        match serde_cbor::to_vec(&message) {
            Ok(serialized_message) => {
                println!(
                    "Broadcasting JobAnnouncementV1 for JobID: {} from PeerID: {}...",
                    job.job_id, self.local_peer_id
                );
                self.swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(self.job_announcement_topic.clone(), serialized_message)?;

                let interest_topic_string = job_interest_topic_string(&job.job_id);
                let interest_topic = Topic::new(interest_topic_string.clone());
                match self.swarm.behaviour_mut().gossipsub.subscribe(&interest_topic) {
                    Ok(_) => println!("Subscribed to interest topic: {}", interest_topic_string),
                    Err(e) => eprintln!("Failed to subscribe to interest topic {}: {:?}", interest_topic_string, e),
                }

                // Store the JobManifest in announced_originated_jobs
                if let Ok(mut announced_jobs_map) = self.announced_originated_jobs.write() {
                    announced_jobs_map.insert(job.job_id.clone(), manifest.clone()); // Store the manifest
                    println!("Added job manifest {} to announced_originated_jobs.", job.job_id);
                } else {
                    eprintln!("Failed to get write lock for announced_originated_jobs while adding job {}.
", job.job_id);
                }
            }
            Err(e) => {
                eprintln!("Error serializing job announcement message: {:?}", e);
                return Err(Box::new(e));
            }
        }
        Ok(())
    }

    // Method to evaluate a job and express interest if suitable
    async fn evaluate_and_express_interest(&mut self, job: &MeshJob) -> Result<(), Box<dyn Error>> {
        // 1. Suitability Check (Simplified)
        // For now, let's assume we need to parse job.params.required_resources_json
        // and compare with local capabilities. This is a placeholder for more complex logic.
        let local_caps = self.construct_capability();
        let required_resources: Result<HashMap<String, u64>, _> = serde_json::from_str(&job.params.required_resources_json);
        
        let is_suitable = match required_resources {
            Ok(req_res) => {
                let mut suitable = true;
                // Example: Check CPU (assuming key "min_cpu_cores" in JSON and ResourceType::Cpu in local_caps)
                if let Some(required_cpu_cores) = req_res.get("min_cpu_cores") {
                    if let Some(available_cpu) = local_caps.available_resources.get(&ResourceType::Cpu) {
                        if *required_cpu_cores > *available_cpu { // direct comparison, assuming units match
                            suitable = false;
                        }
                    } else {
                        suitable = false; // Local node doesn't advertise CPU
                    }
                }
                // Example: Check Memory (assuming key "min_memory_mb" and ResourceType::Memory)
                if let Some(required_memory_mb) = req_res.get("min_memory_mb") {
                     if let Some(available_memory) = local_caps.available_resources.get(&ResourceType::Memory) {
                        if *required_memory_mb > *available_memory { // direct comparison
                            suitable = false;
                        }
                    } else {
                        suitable = false; // Local node doesn't advertise Memory
                    }
                }
                // Add more resource checks as needed
                suitable
            }
            Err(e) => {
                eprintln!("Failed to parse required_resources_json for job {}: {:?}", job.job_id, e);
                false // Not suitable if parsing fails
            }
        };

        if is_suitable {
            println!("Job {} is suitable. Expressing interest.", job.job_id);
            let interest_message = MeshProtocolMessage::JobInterestV1 {
                job_id: job.job_id.clone(),
                executor_did: self.local_node_did.clone(),
            };
            match serde_cbor::to_vec(&interest_message) {
                Ok(serialized_interest_message) => {
                    let interest_topic_string = job_interest_topic_string(&job.job_id);
                    let interest_topic = Topic::new(interest_topic_string.clone());
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(interest_topic, serialized_interest_message)?;
                    println!("Published JobInterestV1 for JobID: {} to topic: {}", job.job_id, interest_topic_string);
                }
                Err(e) => {
                    eprintln!("Error serializing job interest message for job {}: {:?}", job.job_id, e);
                }
            }
        }
        Ok(())
    }

    pub async fn simulate_execution_and_anchor_receipt(&mut self, job: MeshJob) -> Result<(), Box<dyn Error>> {
        let job_id = job.job_id.clone();
        println!("Attempting to take job for execution: {}", job_id);

        // Move to executing_jobs to prevent re-taking (simple lock then move)
        {
            let mut executing = self.executing_jobs.write().map_err(|e| format!("Lock error on executing_jobs: {}", e))?;
            if executing.contains_key(&job_id) || self.completed_job_receipt_cids.read().unwrap().contains_key(&job_id) {
                // Already processing or completed
                return Ok(()); 
            }
            executing.insert(job_id.clone(), job.clone());
        }
        
        println!("Simulating execution for JobId: {}", job_id);

        // Send a "Running" status update
        let status_update_running = MeshProtocolMessage::JobStatusUpdateV1 {
            job_id: job_id.clone(),
            executor_did: self.local_node_did.clone(),
            status: super::JobStatus::Running { // Using the detailed JobStatus from lib.rs
                node_id: self.local_node_did.clone(), // In this context, node_id is the executor's DID string
                current_stage_index: Some(0),
                current_stage_id: Some("execution_simulation".to_string()),
                progress_percent: Some(10),
                status_message: Some("Execution started".to_string()),
            },
        };
        if let Ok(serialized_status_update) = serde_cbor::to_vec(&status_update_running) {
            // Determine the topic for job status updates.
            // For now, let's use the job-specific interest topic, as the originator is subscribed.
            // A dedicated job-specific status topic could also be an option.
            let status_topic_string = job_interest_topic_string(&job_id);
            let status_topic = Topic::new(status_topic_string.clone());
            if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(status_topic.clone(), serialized_status_update) {
                eprintln!("Failed to publish JobStatusUpdateV1 (Running) for {}: {:?}", job_id, e);
            } else {
                println!("Published JobStatusUpdateV1 (Running) for JobID: {} to topic: {}", job_id, status_topic_string);
            }
        } else {
            eprintln!("Failed to serialize JobStatusUpdateV1 (Running) for {}", job_id);
        }

        tokio::time::sleep(Duration::from_secs(2)).await; // Simulate work
        println!("Execution complete for JobId: {}", job_id);

        // Construct ExecutionReceipt
        let execution_start_time = chrono::Utc::now().timestamp() as u64 - 2;
        let execution_end_time_dt = chrono::Utc::now();
        let execution_end_time = execution_end_time_dt.timestamp() as u64;

        // Mock resource usage (ideally derive from job.params.required_resources_json)
        let mut resource_usage_actual = HashMap::new(); 
        resource_usage_actual.insert(ResourceType::Cpu, 50); // mock value
        resource_usage_actual.insert(ResourceType::Memory, 128); // mock value

        let mut receipt = ExecutionReceipt {
            job_id: job_id.clone(),
            executor: self.local_node_did.clone(), 
            status: StandardJobStatus::CompletedSuccess, 
            result_data_cid: Some("bafybeigdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string()), // mock
            logs_cid: Some("bafybeigcafecafebeeffeedbeeffeedbeeffeedbeeffeedbeeffeedbeeffeed".to_string()), // mock
            resource_usage: resource_usage_actual,
            execution_start_time,
            execution_end_time,
            execution_end_time_dt,
            signature: Vec::new(), // Will be filled by sign_receipt_in_place
            coop_id: job.originator_org_scope.as_ref().and_then(|s| s.coop_id.clone()),
            community_id: job.originator_org_scope.as_ref().and_then(|s| s.community_id.clone()),
        };

        // Sign the receipt
        sign_receipt_in_place(&mut receipt, &self.local_keypair)
            .map_err(|e| format!("Failed to sign receipt for job {}: {:?}", job_id, e))?;
        println!("Receipt signed for JobId: {}", job_id);

        // Anchor receipt via local runtime context
        if let Some(rt_ctx) = &self.local_runtime_context {
            let host_env = ConcreteHostEnvironment::new(rt_ctx.clone(), self.local_node_did.clone());
            // anchor_receipt expects the receipt by value
            match host_env.anchor_receipt(receipt.clone()).await {
                Ok(_) => {
                    let anchored_receipt_cid = receipt.cid().map_err(|e| format!("Failed to get CID of anchored receipt: {}", e))?;
                    println!("Receipt successfully anchored for JobId: {}, Receipt CID: {}", job_id, anchored_receipt_cid);
                    self.completed_job_receipt_cids.write().unwrap().insert(job_id.clone(), anchored_receipt_cid);
                    // final_receipt_cid_for_announcement = Some(cid_of_executed_receipt); // Store CID for later announcement
                    // TODO NEXT: Announce ExecutionReceiptAvailableV1 (will require &mut self.swarm or a channel) - THIS IS BEING ADDRESSED NOW
                    // For now, we've stored the CID.

                    // 4. In `trigger_execution_for_job`
                    if let Err(e) = self.internal_action_tx.send(NodeInternalAction::AnnounceReceipt {
                        job_id: job_id.clone(),
                        receipt_cid: anchored_receipt_cid.clone(),
                        executor_did: self.local_node_did.clone(),
                    }).await {
                        tracing::error!(\\\"[ExecutionTrigger] Failed to enqueue receipt announcement for job {}: {:?}\\\", job_id, e);
                    }

                }
                Err(e) => {
                    eprintln!("Failed to anchor receipt for JobId {}: {:?}", job_id, e);
                    // TODO: Consider error handling, e.g., retrying or marking job as failed to anchor
                }
            }
        } else {
            eprintln!("No runtime_context available to anchor receipt for JobID: {}. Skipping anchoring.", job_id);
            // final_receipt_cid_for_announcement = Some(cid_of_executed_receipt); // Still have a CID, just not anchored by us
            // TODO NEXT: Announce ExecutionReceiptAvailableV1 (will require &mut self.swarm or a channel) - THIS IS BEING ADDRESSED NOW
            // If not anchored, but execution was successful, we might still want to announce the receipt CID
            // if the use case supports unanchored (but signed) receipts.
            // For now, let's also send it for announcement if we have a CID.
            if let Err(e) = self.internal_action_tx.send(NodeInternalAction::AnnounceReceipt {
                job_id: job_id.clone(),
                receipt_cid: anchored_receipt_cid.clone(), // cid_of_executed_receipt is in scope here
                executor_did: self.local_node_did.clone(),
            }).await {
                tracing::error!(\\\"[ExecutionTrigger] Failed to enqueue receipt announcement (unanchored) for job {}: {:?}\\\", job_id, e);
            }
        }

        // Clean up from executing_jobs after attempting anchor
        self.executing_jobs.write().unwrap().remove(&job_id);

        // Announce receipt availability
        let announcement = MeshProtocolMessage::ExecutionReceiptAvailableV1 {
            job_id: job_id.clone(),
            receipt_cid: anchored_receipt_cid.to_string(),
            executor_did: self.local_node_did.clone(),
        };

        match serde_json::to_vec(&announcement) {
            Ok(bytes) => {
                if let Err(e) = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(RECEIPT_AVAILABILITY_TOPIC_HASH, bytes)
                {
                    eprintln!("Failed to publish receipt availability for JobId {}: {:?}", job_id, e);
                } else {
                    println!("Published ExecutionReceiptAvailableV1 for JobId: {}, Receipt CID: {}", job_id, anchored_receipt_cid);
                }
            }
            Err(e) => {
                eprintln!("Failed to serialize ExecutionReceiptAvailableV1 for JobId {}: {:?}", job_id, e);
            }
        }

        // After successful anchoring and before returning Ok(())
        // Send a "Completed" status update (if successful, otherwise a "Failed" one)
        let final_status = if self.completed_job_receipt_cids.read().unwrap().contains_key(&job_id) {
            super::JobStatus::Completed {
                node_id: self.local_node_did.clone(),
                receipt_cid: self.completed_job_receipt_cids.read().unwrap().get(&job_id).unwrap().to_string(),
            }
        } else {
            super::JobStatus::Failed {
                node_id: Some(self.local_node_did.clone()),
                error: "Execution simulated but receipt anchoring might have failed or was skipped.".to_string(),
                stage_index: Some(1), // Assuming anchoring is the next stage
                stage_id: Some("anchoring".to_string()),
            }
        };

        let status_update_final = MeshProtocolMessage::JobStatusUpdateV1 {
            job_id: job_id.clone(),
            executor_did: self.local_node_did.clone(),
            status: final_status,
        };
        if let Ok(serialized_final_update) = serde_cbor::to_vec(&status_update_final) {
            let status_topic_string = job_interest_topic_string(&job_id);
            let status_topic = Topic::new(status_topic_string.clone());
            if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(status_topic, serialized_final_update) {
                eprintln!("Failed to publish final JobStatusUpdateV1 for {}: {:?}", job_id, e);
            } else {
                 println!("Published final JobStatusUpdateV1 for JobID: {} to topic: {}", job_id, status_topic_string);
            }
        } else {
            eprintln!("Failed to serialize final JobStatusUpdateV1 for {}", job_id);
        }

        Ok(())
    }

    pub async fn assign_job_to_executor(
        &mut self,
        job_id: &IcnJobId,
        target_executor_did: Did,
        job_details: MeshJob,
        originator_did: Did,
    ) -> Result<(), Box<dyn Error>> {
        tracing::info!(
            "Attempting to publish AssignJobV1 for job_id: {}, target_executor: {}, originator: {}",
            job_id,
            target_executor_did,
            originator_did
        );

        let assignment_message = MeshProtocolMessage::AssignJobV1 {
            job_id: job_id.clone(),
            target_executor_did: target_executor_did.clone(),
            job_details: job_details.clone(),
            originator_did: originator_did.clone(),
        };

        let serialized_message = serde_cbor::to_vec(&assignment_message)?;
        
        let topic_str = crate::utils::direct_message_topic_string(&target_executor_did);
        let topic = Topic::new(topic_str.clone());

        match self.swarm.behaviour_mut().gossipsub.publish(topic.clone(), serialized_message) {
            Ok(message_id) => {
                tracing::info!(
                    "Published AssignJobV1 to topic '{}' (for executor {}). Message ID: {:?}",
                    topic_str,
                    target_executor_did,
                    message_id
                );

                // ADDITION: Subscribe to the job's interest topic to listen for status updates
                let job_interest_topic_str = crate::utils::job_interest_topic_string(job_id);
                let job_interest_topic = Topic::new(job_interest_topic_str.clone());
                match self.swarm.behaviour_mut().gossipsub.subscribe(&job_interest_topic) {
                    Ok(subscribed) => {
                        if subscribed {
                            tracing::info!("Node {} successfully subscribed to job interest topic '{}' for status updates.", self.local_node_did, job_interest_topic_str);
                        } else {
                            tracing::info!("Node {} already subscribed or no change for job interest topic '{}'.", self.local_node_did, job_interest_topic_str);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Node {} error subscribing to job interest topic '{}': {:?}", self.local_node_did, job_interest_topic_str, e);
                        // Not returning error, as assignment was published. This is best-effort.
                    }
                }

                // ADDITION: Trigger execution
                let job_id_clone_for_trigger = job_id.clone();
                // Ensure MeshNode is Clone to allow this.
                let self_clone = self.clone(); // Use self.clone() as MeshNode derives Clone
                tokio::spawn(async move {
                    // Note: trigger_execution_for_job now takes &self
                    if let Err(e) = self_clone.trigger_execution_for_job(&job_id_clone_for_trigger).await {
                        tracing::error!("[ExecutionTrigger] Failed to trigger execution for job {}: {:?}\", job_id_clone_for_trigger, e);
                    }
                });

                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to publish AssignJobV1 to topic '{}': {:?}", topic_str, e);
                Err(Box::new(e))
            }
        }
    }

    pub async fn trigger_execution_for_job(&self, job_id: &IcnJobId) -> Result<(), String> {
        tracing::info!("[ExecutionTrigger] Attempting to trigger execution for job {}", job_id);

        let job_details_opt: Option<MeshJob>;
        {
            // Changed to .read().await since we now have &self
            let assigned_jobs_guard = self.assigned_jobs.read().unwrap();
            job_details_opt = assigned_jobs_guard.get(job_id).cloned();
        }

        if let Some(job_details) = job_details_opt {
            tracing::info!("[ExecutionTrigger] Preparing to execute job: {}", job_id);

            match execute_mesh_job(
                job_details.clone(), 
                &self.local_keypair,
                self.local_runtime_context.clone(),
            ).await {
                Ok(executed_receipt) => { 
                    tracing::info!("[ExecutionTrigger] Execution successful for job {}. Receipt: {:?}", job_id, executed_receipt);
                    
                    match executed_receipt.cid() {
                        Ok(cid_of_executed_receipt) => {
                            tracing::info!("[ExecutionTrigger] Calculated CID {} for executed receipt of job {}. Attempting to anchor...", cid_of_executed_receipt, job_id);

                            if let Some(rt_ctx) = &self.local_runtime_context {
                                let host_env = ConcreteHostEnvironment::new(rt_ctx.clone(), self.local_node_did.clone());
                                match host_env.anchor_receipt(executed_receipt.clone()).await {
                                    Ok(()) => {
                                        tracing::info!("[ExecutionTrigger] Successfully anchored receipt CID {} for job {}.", cid_of_executed_receipt, job_id);
                                        self.completed_job_receipt_cids.write().unwrap().insert(job_id.clone(), cid_of_executed_receipt.clone());
                                        
                                        let announce_action = NodeInternalAction::AnnounceReceipt {
                                            job_id: job_id.clone(),
                                            receipt_cid: cid_of_executed_receipt.clone(),
                                            executor_did: self.local_node_did.clone(),
                                        };
                                        if let Err(e) = self.internal_action_tx.send(announce_action).await {
                                            tracing::error!("[ExecutionTrigger] Failed to send AnnounceReceipt for job {}: {:?}", job_id, e);
                                        }
                                    }
                                    Err(anchor_err) => {
                                        tracing::error!("[ExecutionTrigger] Failed to anchor receipt for job {}: {:?}. Original CID was {}.", job_id, anchor_err, cid_of_executed_receipt);
                                    }
                                }
                            } else {
                                tracing::warn!("[ExecutionTrigger] No local_runtime_context available. Skipping anchoring of receipt CID {} for job {}.", cid_of_executed_receipt, job_id);
                                let announce_action = NodeInternalAction::AnnounceReceipt {
                                    job_id: job_id.clone(),
                                    receipt_cid: cid_of_executed_receipt.clone(),
                                    executor_did: self.local_node_did.clone(),
                                };
                                if let Err(e) = self.internal_action_tx.send(announce_action).await {
                                    tracing::error!("[ExecutionTrigger] Failed to send AnnounceReceipt (unanchored) for job {}: {:?}", job_id, e);
                                }
                            }
                        }
                        Err(cid_err) => {
                            tracing::warn!("[ExecutionTrigger] Failed to compute receipt CID for job {}: {}.", job_id, cid_err);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("[ExecutionTrigger] Failed to execute job {}: {:?}", job_id, e);
                    return Err(format!("Execution failed for job {}: {}", job_id, e));
                }
            };
            
        } else {
            tracing::warn!("[ExecutionTrigger] No assigned job found with ID {}", job_id);
            return Err(format!("Job {} not found for execution", job_id));
        }

        Ok(())
    }

    pub async fn fetch_receipt_cbor_via_kad(&mut self, receipt_cid: &Cid) -> Result<Vec<u8>, FetchError> {
        let key = KadKey::new(&receipt_cid.to_bytes());
        tracing::info!("[MeshNode] Initiating Kademlia get_record for receipt CID: {}", receipt_cid);

        let (tx, rx) = oneshot::channel::<Result<Vec<u8>, FetchError>>();
        let query_id = self.swarm.behaviour_mut().kademlia.get_record(key);

        {
            let mut queries = self.receipt_queries.lock().unwrap();
            queries.insert(query_id, tx);
            tracing::debug!("[MeshNode] Stored Kademlia query_id {:?} for receipt CID: {}", query_id, receipt_cid);
        }

        // Wait for the Kademlia query to complete or timeout
        match tokio::time::timeout(Duration::from_secs(30), rx).await { // 30 second timeout
            Ok(Ok(Ok(data))) => {
                tracing::info!("[MeshNode] Kademlia get_record successful for receipt CID: {}. Data length: {}", receipt_cid, data.len());
                Ok(data)
            }
            Ok(Ok(Err(fetch_err))) => {
                tracing::warn!("[MeshNode] Kademlia get_record failed for receipt CID {}: {:?}", receipt_cid, fetch_err);
                Err(fetch_err)
            }
            Ok(Err(_recv_err)) => {
                // Oneshot channel was dropped, likely because Kademlia handler couldn't send a result (e.g. panic or unexpected shutdown)
                tracing::error!("[MeshNode] Kademlia get_record query oneshot channel dropped for receipt CID {}. This is unexpected.", receipt_cid);
                Err(FetchError::KadQueryError(*receipt_cid, "Oneshot channel receiver error".to_string()))
            }
            Err(_timeout_err) => {
                tracing::warn!("[MeshNode] Kademlia get_record timed out for receipt CID: {}", receipt_cid);
                // Remove the query from the map to prevent stale entries if Kademlia eventually responds
                {
                    let mut queries = self.receipt_queries.lock().unwrap();
                    queries.remove(&query_id);
                }
                Err(FetchError::KadQueryTimeout(*receipt_cid))
            }
        }
    }

    // Placeholder for triggering economic settlement
    async fn trigger_economic_settlement(&self, job_id: &IcnJobId, receipt: &ExecutionReceipt) {
        tracing::info!("[MeshNode] Attempting economic settlement for JobID: {}, Executor: {}, Receipt CID: {}",
                 job_id, receipt.executor, receipt.cid().map_or_else(|e| format!("Error: {}", e), |c| c.to_string()));

        // TODO: BIDDING SYSTEM INTEGRATION REQUIRED FOR ACTUAL PRICE.
        // The current MOCK_BID_PRICE is a placeholder.
        // Once the bidding protocol (e.g., JobBidV1 message, Bid struct, and bid storage)
        // is implemented, this function must be updated to:
        // 1. Retrieve all bids for the given `job_id`.
        // 2. Find the specific bid where `bid.bidder == receipt.executor`.
        // 3. Use `bid.price` (or equivalent field from the implemented Bid struct) for the transfer amount.
        const MOCK_BID_PRICE: u64 = 100; 
        tracing::warn!(
            "[MeshNode] Using MOCK_BID_PRICE: {} for job {}. This is a placeholder until bidding system is implemented.",
            MOCK_BID_PRICE, job_id
        );

        let originator_did_opt: Option<Did> = {
            let originated_jobs_guard = self.announced_originated_jobs.read().unwrap();
            originated_jobs_guard.get(job_id).map(|manifest| manifest.submitter_did.clone())
        };

        if originator_did_opt.is_none() {
            tracing::error!("[MeshNode] Economic settlement failed: Could not find originator DID for JobID: {}. Job manifest might be missing.", job_id);
            return;
        }
        let originator_did = originator_did_opt.unwrap();
        let executor_did = &receipt.executor;

        if originator_did == *executor_did {
            tracing::info!("[MeshNode] Economic settlement skipped: Originator and executor are the same ({}). No payment needed.", originator_did);
            return;
        }

        if let Some(rt_ctx) = &self.local_runtime_context {
            tracing::info!("[MeshNode] Attempting to transfer {} ICN from {} to {} for job {}",
                MOCK_BID_PRICE, originator_did, executor_did, job_id
            );

            let transfer_result = rt_ctx.economics.transfer_balance_direct(
                &originator_did,      // from_org_did
                None,                 // from_ledger_scope_id
                None,                 // from_key_scope
                executor_did,         // to_org_did
                None,                 // to_ledger_scope_id
                None,                 // to_key_scope
                &icn_economics::ResourceType::Token("ICN".to_string()), // resource_type
                MOCK_BID_PRICE,       // amount
                &rt_ctx.resource_ledger,
                &rt_ctx.transaction_log,
            ).await;

            match transfer_result {
                Ok(_) => {
                    tracing::info!("[MeshNode] Economic settlement SUCCESSFUL for JobID: {}. Transferred {} ICN from {} to {}.",
                             job_id, MOCK_BID_PRICE, originator_did, executor_did);
                }
                Err(e) => {
                    tracing::error!("[MeshNode] Economic settlement FAILED for JobID: {}. Error during transfer from {} to {}: {:?}",
                              job_id, originator_did, executor_did, e);
                }
            }
        } else {
            tracing::warn!("[MeshNode] Economic settlement skipped for JobID: {}: No local_runtime_context available.", job_id);
        }
    }

    // Placeholder for triggering reputation update
    async fn trigger_reputation_update(&self, job_id_str: &IcnJobId, receipt: &ExecutionReceipt) {
        let executor_did = &receipt.executor;
        let timestamp_utc = Utc::now();

        // Attempt to use the receipt's CID as the job_id for the reputation event, as it's a verifiable anchor.
        // If the JobId for reputation events *must* be the original job's CID, this will need adjustment.
        let event_job_cid = match receipt.cid() {
            Ok(cid) => cid,
            Err(e) => {
                tracing::error!(
                    "[MeshNode] Failed to get receipt CID for reputation event (JobID: {}): {:?}. Skipping reputation update.",
                    job_id_str, e
                );
                return;
            }
        };

        tracing::info!(
            "[MeshNode] Attempting to trigger reputation update for JobID: {}, Executor: {}, ReceiptCID for event: {}",
            job_id_str, executor_did, event_job_cid
        );

        if self.local_runtime_context.is_none() {
            tracing::warn!(
                "[MeshNode] Reputation update skipped for JobID: {}: No local_runtime_context available.",
                job_id_str
            );
            return;
        }

        let reputation_event = match &receipt.status {
            StandardJobStatus::CompletedSuccess => {
                let execution_duration_ms = receipt.execution_end_time.saturating_sub(receipt.execution_start_time) * 1000; // Assuming s to ms
                ReputationUpdateEvent::JobCompletedSuccessfully {
                    job_id: event_job_cid, // Using receipt's CID
                    execution_duration_ms: execution_duration_ms as u32, // Ensure type cast is safe
                    bid_accuracy: 1.0, // Placeholder: TODO: Requires actual bid vs. resource usage
                    on_time: true,     // Placeholder: TODO: Requires definition of "on time"
                    anchor_cid: Some(event_job_cid),
                }
            }
            StandardJobStatus::Failed { error, .. } => ReputationUpdateEvent::JobFailed {
                job_id: event_job_cid, // Using receipt's CID
                reason: error.clone(),
                anchor_cid: Some(event_job_cid),
            },
            _ => {
                tracing::warn!(
                    "[MeshNode] No specific reputation event for JobID: {} with status: {:?}. Skipping reputation update.",
                    job_id_str, receipt.status
                );
                return;
            }
        };

        let record = ReputationRecord {
            timestamp: timestamp_utc,
            issuer: self.local_node_did.clone(), // The node verifying the receipt and issuing the record
            subject: executor_did.clone(),       // The node whose reputation is being updated
            event: reputation_event,
            anchor: Some(event_job_cid),         // Anchoring to the receipt itself
            signature: None, // TODO: Consider signing this record with self.local_keypair if needed by reputation system
        };

        if let Some(base_url) = &self.reputation_service_url {
            let client = &self.http_client;
            let url = format!("{}/reputation/records", base_url.trim_end_matches('/'));

            tracing::info!(
                "[MeshNode] Submitting ReputationRecord for JobID: {} to URL: {}",
                job_id_str, url
            );

            match client.post(&url).json(&record).send().await {
                Ok(response) => {
                    if response.status().is_success() || response.status() == reqwest::StatusCode::CREATED {
                        tracing::info!(
                            "[MeshNode] Reputation record submitted successfully for JobID: {}, Executor: {}. Status: {}",
                            job_id_str, executor_did, response.status()
                        );
                    } else {
                        let status = response.status();
                        let error_body = response.text().await.unwrap_or_else(|_| "<no body>".to_string());
                        tracing::error!(
                            "[MeshNode] Failed to submit reputation record for JobID: {}, Executor: {}. Status: {}. Body: {}",
                            job_id_str, executor_did, status, error_body
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "[MeshNode] HTTP request failed during reputation record submission for JobID: {}, Executor: {}: {:?}",
                        job_id_str, executor_did, e
                    );
                }
            }
        } else {
            tracing::warn!(
                "[MeshNode] Reputation submission skipped for JobID: {}: Reputation service URL not configured.",
                job_id_str
            );
        }
    }

    pub async fn run_event_loop(&mut self, mut internal_action_rx: mpsc::Receiver<NodeInternalAction>) -> Result<(), Box<dyn Error>> {
        // Periodic tasks setup
        let mut capabilities_interval = time::interval(Duration::from_secs(60)); // Broadcast capabilities every 60s
        let mut job_queue_interval = time::interval(Duration::from_secs(10)); // Check job queue every 10s
        let mut executor_selection_interval = time::interval(Duration::from_secs(15)); // Check for job interests every 15s

        loop {
            tokio::select! {
                // Timer for broadcasting capabilities
                _ = capabilities_interval.tick() => {
                    if let Err(e) = self.broadcast_capabilities().await {
                        eprintln!("Failed to broadcast capabilities: {:?}", e);
                    }
                }

                // Timer for checking and announcing jobs from the runtime queue
                _ = job_queue_interval.tick() => {
                    let mut jobs_to_announce = Vec::new();
                    {
                        if let Ok(mut queue) = self.runtime_job_queue.lock() {
                            while let Some(job) = queue.pop_front() {
                                jobs_to_announce.push(job);
                            }
                        } else {
                            eprintln!("Failed to lock runtime_job_queue");
                        }
                    }
                    for job in jobs_to_announce {
                        if let Err(e) = self.announce_job(job.clone()).await {
                            eprintln!("Failed to announce job {}: {:?}", job.job_id, e);
                            // Potentially re-queue the job or mark as failed announcement
                        }
                    }
                }
                
                // Timer for selecting executors for originated jobs
                _ = executor_selection_interval.tick() => {
                    let jobs_to_assign: Vec<(IcnJobId, Did)> = {
                        let originated_jobs = self.announced_originated_jobs.read().unwrap_or_else(|e| {
                            eprintln!("Failed to get read lock on originated_jobs: {}", e);
                            Default::default()
                        });
                        let interests = self.job_interests_received.read().unwrap_or_else(|e| {
                            eprintln!("Failed to get read lock on job_interests: {}", e);
                            Default::default()
                        });
                        
                        let mut assignments = Vec::new();
                        for (job_id, _job_details) in originated_jobs.iter() {
                            // Simple selection: if we originated it and it has interests, pick the first interested party.
                            // And ensure we haven't already assigned it (needs better state tracking for assigned jobs).
                            // This is a placeholder for more sophisticated selection logic.
                            if let Some(interested_dids) = interests.get(job_id) {
                                if !interested_dids.is_empty() {
                                    // TODO: Add logic to ensure a job isn't assigned multiple times.
                                    // This might involve checking job_details.status or a separate tracking map.
                                    println!("Job {} has {} interested parties. Selecting first one.", job_id, interested_dids.len());
                                    assignments.push((job_id.clone(), interested_dids[0].clone()));
                                }
                            }
                        }
                        assignments
                    };

                    for (job_id, executor_did) in jobs_to_assign {
                         // Before assigning, we should lock `announced_originated_jobs` and update its status
                         // or use another mechanism to prevent re-assignment.
                         // For this example, we proceed directly.
                        if let Err(e) = self.assign_job_to_executor(&job_id, executor_did.clone(), job_details.clone(), self.local_node_did.clone()).await {
                            eprintln!("Failed to assign job {} to {}: {:?}", job_id, executor_did, e);
                        }
                    }
                }

                // Swarm events
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(behaviour_event) => match behaviour_event {
                            MeshBehaviourEvent::Mdns(mdns_event) => {
                                match mdns_event {
                                    libp2p::mdns::Event::Discovered(list) => {
                                        for (peer_id, _multiaddr) in list {
                                            println!("mDNS discovered a new peer: {}", peer_id);
                                            // Optionally add to known peers or attempt to connect for gossipsub
                                            self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                        }
                                    }
                                    libp2p::mdns::Event::Expired(list) => {
                                        for (peer_id, _multiaddr) in list {
                                            println!("mDNS peer has expired: {}", peer_id);
                                            self.swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                        }
                                    }
                                }
                            }
                            MeshBehaviourEvent::Gossipsub(gossip_event) => {
                                if let libp2p::gossipsub::Event::Message {
                                    propagation_source: _peer_id,
                                    message_id: _id,
                                    message,
                                } = gossip_event
                                {
                                    match serde_cbor::from_slice::<MeshProtocolMessage>(&message.data) {
                                        Ok(protocol_message) => {
                                            match protocol_message {
                                                MeshProtocolMessage::CapabilityAdvertisementV1(capability) => {
                                                    println!("Received CapabilityAdvertisementV1 from DID: {}", capability.node_did);
                                                    // TODO: Store or process capability information
                                                }
                                                MeshProtocolMessage::JobAnnouncementV1(job) => {
                                                    println!("Received JobAnnouncementV1 for JobID: {} on topic {}", job.job_id, message.topic);
                                                    // Store the job if not already known
                                                    if let Ok(mut available) = self.available_jobs_on_mesh.write() {
                                                        available.entry(job.job_id.clone()).or_insert_with(|| job.clone());
                                                    }
                                                    // Evaluate and potentially express interest
                                                    let mut self_clone_for_interest = self.clone_for_async_tasks();
                                                    let job_clone_for_interest = job.clone();
                                                    tokio::spawn(async move {
                                                        if let Err(e) = self_clone_for_interest.evaluate_and_express_interest(&job_clone_for_interest).await {
                                                            eprintln!("Error evaluating/expressing interest for job {}: {:?}", job_clone_for_interest.job_id, e);
                                                        }
                                                    });
                                                }
                                                MeshProtocolMessage::JobInterestV1 { job_id, executor_did } => {
                                                    println!("Received JobInterestV1 for JobID: {} from Executor DID: {} on topic {}", job_id, executor_did, message.topic);
                                                    if let Ok(announced_jobs) = self.announced_originated_jobs.read() {
                                                        if announced_jobs.contains_key(&job_id) {
                                                            // This node originated the job
                                                            if let Ok(mut interests) = self.job_interests_received.write() {
                                                                interests.entry(job_id.clone()).or_default().push(executor_did.clone());
                                                                println!("Recorded interest for job {} from executor {}", job_id, executor_did);
                                                            }
                                                        } else {
                                                            // Not the originator, or not tracking this job as originated. Log or ignore.
                                                        }
                                                    }
                                                }
                                                MeshProtocolMessage::AssignJobV1 {
                                                    job_id,
                                                    originator_did,
                                                    target_executor_did,
                                                    job_details,
                                                } => {
                                                    println!(
                                                        "Received AssignJobV1 for JobID: {} from Originator: {} to Executor: {} on topic {}",
                                                        job_id, originator_did, target_executor_did, message.topic
                                                    );
                                                    if target_executor_did == self.local_node_did {
                                                        tracing::info!(
                                                            "This node ({}) IS the target_executor for job {}. Processing assignment...",
                                                            self.local_node_did, job_id
                                                        );
                                                        
                                                        // 1. Store the job locally for execution.
                                                        {
                                                            let mut assigned_jobs_map = self.assigned_jobs.write().unwrap_or_else(|e| {
                                                                tracing::error!("assigned_jobs RwLock poisoned: {}", e);
                                                                e.into_inner()
                                                            });
                                                            assigned_jobs_map.insert(job_id.clone(), job_details.clone());
                                                            tracing::info!("Job {} stored in assigned_jobs.", job_id);
                                                        }

                                                        // 2. Send JobStatusUpdateV1 (Assigned) back to originator.
                                                        let assigned_status = super::JobStatus::Assigned { 
                                                            node_id: self.local_node_did.to_string(), // Use local node's DID string as node_id
                                                        };
                                                        let status_update_msg = MeshProtocolMessage::JobStatusUpdateV1 {
                                                            job_id: job_id.clone(),
                                                            executor_did: self.local_node_did.clone(),
                                                            status: assigned_status,
                                                        };

                                                        if let Ok(serialized_update) = serde_cbor::to_vec(&status_update_msg) {
                                                            let topic_str = job_interest_topic_string(&job_id);
                                                            let topic = Topic::new(topic_str.clone());
                                                            if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(topic.clone(), serialized_update) {
                                                                tracing::error!("Failed to publish JobStatusUpdateV1 (Assigned) for {}: {:?}", job_id, e);
                                                            } else {
                                                                tracing::info!("Published JobStatusUpdateV1 (Assigned) for job {} to topic {}", job_id, topic_str);
                                                            }
                                                        } else {
                                                            tracing::error!("Failed to serialize JobStatusUpdateV1 (Assigned) for job {}", job_id);
                                                        }

                                                        // 3. (Optional) Trigger actual job execution process.
                                                        // This part would involve converting MeshJob to JobManifest if needed by execution logic,
                                                        // interacting with icn-runtime, etc.
                                                        // For now, we've stored it. The actual execution can be picked up by another loop
                                                        // or triggered here. The existing example for spawning a task to send "Executing"
                                                        // status can be adapted.

                                                        // Example: Placeholder for triggering execution.
                                                        // A more robust system might have a dedicated task that monitors `assigned_jobs`
                                                        // or this handler could directly initiate it.
                                                        tracing::info!("Job {} is ready for execution. Actual execution triggering is a TODO.", job_id);

                                                        // The existing tokio::spawn example for sending "Executing" can be kept or adapted.
                                                        // For now, we've sent "Assigned". "Executing" would come when it actually starts.

                                                    } else {
                                                        // Not for this node.
                                                        tracing::trace!("AssignJobV1 for job {} is not for this node ({}). Ignoring.", job_id, self.local_node_did);
                                                    }
                                                }
                                                MeshProtocolMessage::ExecutionReceiptAvailableV1 { job_id, receipt_cid, executor_did } => {
                                                    println!(
                                                        "Received ExecutionReceiptAvailableV1 for JobID: {} from Executor DID: {} with Receipt CID: {} on topic {}",
                                                        job_id, executor_did, receipt_cid, message.topic
                                                    );
                                                    let parsed_receipt_cid = match Cid::try_from(receipt_cid.as_str()) {
                                                        Ok(cid) => cid,
                                                        Err(e) => {
                                                            eprintln!("[MeshNode] Failed to parse receipt_cid string {} for job {}: {}", receipt_cid, job_id, e);
                                                            continue; // Skip processing this message
                                                        }
                                                    };

                                                    if let Ok(mut discovered_receipts) = self.discovered_receipt_announcements.write() {
                                                        discovered_receipts.insert(job_id.clone(), (parsed_receipt_cid, executor_did.clone()));
                                                        println!("[MeshNode] Stored receipt announcement for job {}.", job_id);
                                                    } else {
                                                        eprintln!("[MeshNode] Failed to get write lock for discovered_receipt_announcements for job {}.", job_id);
                                                        // Continue, as we might still want to process if we are the originator
                                                    }

                                                    // Check if this node is the originator of the job
                                                    let is_originator = self.announced_originated_jobs.read().unwrap().contains_key(&job_id);

                                                    if is_originator {
                                                        println!("[MeshNode] This node is the originator for job {}. Attempting to fetch and verify receipt {}.", job_id, parsed_receipt_cid);

                                                        // Call the updated Kademlia fetch function
                                                        let cbor_data_result = self.fetch_receipt_cbor_via_kad(&parsed_receipt_cid).await;

                                                        match cbor_data_result { 
                                                            Ok(cbor_data) => {
                                                                println!("[MeshNode] Successfully fetched CBOR data for receipt CID: {}", parsed_receipt_cid);
                                                                match serde_cbor::from_slice::<ExecutionReceipt>(&cbor_data) {
                                                                    Ok(receipt) => {
                                                                        // Calculate CID from the received and deserialized receipt data
                                                                        let actual_receipt_cid = match receipt.cid() {
                                                                            Ok(cid) => cid,
                                                                            Err(e) => {
                                                                                tracing::error!("[MeshNode] Failed to calculate CID from deserialized receipt for JobID {}: {:?}. Announced CID was {}. Skipping.", job_id, e, parsed_receipt_cid);
                                                                                continue; // Skip processing this message
                                                                            }
                                                                        };

                                                                        // Compare with announced CID
                                                                        if actual_receipt_cid != parsed_receipt_cid {
                                                                            tracing::error!("[MeshNode] CID mismatch! Announced CID {} does not match calculated CID {} for deserialized receipt (JobID {}). Skipping.", parsed_receipt_cid, actual_receipt_cid, job_id);
                                                                            continue; // Skip processing this message
                                                                        }
                                                                        tracing::info!("[MeshNode] Successfully deserialized ExecutionReceipt, CID {} matches announced. JobID: {}", actual_receipt_cid, job_id);

                                                                        // Security check: ensure the executor in the receipt matches the one in the announcement
                                                                        if receipt.executor != executor_did {
                                                                            eprintln!("[MeshNode] Receipt verification failed: Executor DID mismatch. Announced: {}, In Receipt: {}. JobID: {}", executor_did, receipt.executor, job_id);
                                                                            continue; // Skip processing this message
                                                                        }

                                                                        match verify_embedded_signature(&receipt) {
                                                                            Ok(true) => {
                                                                                tracing::info!("[MeshNode] SUCCESS: Receipt signature VERIFIED for JobID: {}, Receipt CID: {}, Executor: {}",
                                                                                                job_id, actual_receipt_cid, executor_did);

                                                                                // ANCHORING LOGIC STARTS HERE
                                                                                if let Some(rt_ctx) = &self.local_runtime_context {
                                                                                    tracing::info!("[MeshNode] Attempting to anchor verified receipt for JobID: {}, Receipt CID: {}", job_id, actual_receipt_cid);
                                                                                    match receipt.to_dag_node() {
                                                                                        Ok(dag_node) => {
                                                                                            match rt_ctx.receipt_store.write() {
                                                                                                Ok(mut store) => {
                                                                                                    if store.dag_nodes.contains_key(&actual_receipt_cid) {
                                                                                                        tracing::info!("[MeshNode] Receipt CID {} already present in local store. Skipping re-anchoring.", actual_receipt_cid);
                                                                                                    } else {
                                                                                                        store.dag_nodes.insert(actual_receipt_cid, dag_node);
                                                                                                        tracing::info!("[MeshNode] Successfully anchored receipt CID {} locally for JobID: {}.", actual_receipt_cid, job_id);
                                                                                                    }
                                                                                                }
                                                                                                Err(e) => {
                                                                                                    tracing::error!("[MeshNode] Failed to acquire lock on receipt_store for anchoring receipt CID {}: {:?}", actual_receipt_cid, e);
                                                                                                }
                                                                                            }
                                                                                        }
                                                                                        Err(e) => {
                                                                                            tracing::error!("[MeshNode] Failed to convert ExecutionReceipt to DagNode for CID {}: {:?}", actual_receipt_cid, e);
                                                                                        }
                                                                                    }
                                                                                } else {
                                                                                    tracing::warn!("[MeshNode] No local_runtime_context available. Skipping local anchoring of verified receipt CID {} for JobID: {}.", actual_receipt_cid, job_id);
                                                                                }
                                                                                // ANCHORING LOGIC ENDS HERE

                                                                                // Trigger post-verification actions
                                                                                let self_clone = self.clone_for_async_tasks(); // Assuming such a helper exists or can be made
                                                                                let job_id_clone = job_id.clone();
                                                                                let receipt_clone = receipt.clone();
                                                                                tokio::spawn(async move {
                                                                                    self_clone.trigger_economic_settlement(&job_id_clone, &receipt_clone).await;
                                                                                });

                                                                                let self_clone_rep = self.clone_for_async_tasks();
                                                                                let job_id_clone_rep = job_id.clone();
                                                                                // let receipt_clone_rep = receipt.clone(); // uncomment if needed, receipt already cloned
                                                                                tokio::spawn(async move {
                                                                                    self_clone_rep.trigger_reputation_update(&job_id_clone_rep, &receipt).await;
                                                                                });

                                                                            }
                                                                            Ok(false) => {
                                                                                eprintln!("[MeshNode] Receipt verification FAILED: Invalid signature for JobID: {}, Receipt CID: {}, Executor: {}",
                                                                                            job_id, parsed_receipt_cid, executor_did);
                                                                            }
                                                                            Err(e) => {
                                                                                eprintln!("[MeshNode] Error during receipt signature verification for JobID: {}: {:?}. Receipt CID: {}",
                                                                                            job_id, e, parsed_receipt_cid);
                                                                            }
                                                                        }
                                                                    }
                                                                    Err(e) => {
                                                                        eprintln!("[MeshNode] Failed to deserialize CBOR to ExecutionReceipt for CID: {}: {}. Data len: {}", parsed_receipt_cid, e, cbor_data.len());
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                eprintln!("[MeshNode] Failed to fetch receipt CBOR data for CID {}: {:?}", parsed_receipt_cid, e);
                                                                // TODO: Implement retry logic or add to a pending queue?
                                                            }
                                                        }
                                                    } else {
                                                        // Not the originator, just discovered the announcement.
                                                        // Might be interested for other reasons in the future (e.g. federation member verifying all receipts)
                                                    }
                                                }
                                                MeshProtocolMessage::JobStatusUpdateV1 { job_id, executor_did, status } => {
                                                    println!(
                                                        "Received JobStatusUpdateV1 for JobID: {} from Executor: {}, New Status: {:?}, on topic: {}",
                                                        job_id, executor_did, status, message.topic
                                                    );
                                                    // If this node originated the job, update its status.
                                                    if let Ok(mut originated_jobs) = self.announced_originated_jobs.write() {
                                                        if let Some(originated_job_entry) = originated_jobs.get_mut(&job_id) {
                                                            // We need to update the status within MeshJob or its wrapper.
                                                            // Assuming MeshJob itself doesn't have a mutable status directly usable here,
                                                            // this highlights the need for JobManifest or a similar mutable structure
                                                            // for the originator to track detailed status.
                                                            // For now, we just log the reception.
                                                            println!("Originator received status update for job {}: {:?}", job_id, status);
                                                            
                                                            // Example of how it *could* look if MeshJob had a status field:
                                                            // originated_job_entry.status = status.into_standard_job_status(); // Assuming a conversion method

                                                            // If the status is Completed or Failed, the originator might unsubscribe from the job's interest topic.
                                                            match status {
                                                                super::JobStatus::Completed { .. } | super::JobStatus::Failed { .. } => {
                                                                    let interest_topic_string = job_interest_topic_string(&job_id);
                                                                    let interest_topic = Topic::new(interest_topic_string.clone());
                                                                    if self.swarm.behaviour_mut().gossipsub.unsubscribe(&interest_topic).is_ok() {
                                                                        println!("Unsubscribed from interest topic {} after job terminal state.", interest_topic_string);
                                                                    } else {
                                                                        eprintln!("Failed to unsubscribe from interest topic {} after job terminal state.", interest_topic_string);
                                                                    }
                                                                    // Also potentially remove from job_interests_received for this job_id
                                                                    if let Ok(mut interests) = self.job_interests_received.write() {
                                                                        interests.remove(&job_id);
                                                                        println!("Cleared interests for job {} after terminal state.", job_id);
                                                                    }
                                                                }
                                                                _ => {}
                                                            }
                                                        } else {
                                                            // Not the originator, or job not in announced_originated_jobs. Could be an executor seeing its own status update echo.
                                                        }
                                                    } else {
                                                        eprintln!("Failed to get write lock for announced_originated_jobs while handling status update for {}.\n", job_id);
                                                    }
                                                }
                                                MeshProtocolMessage::JobBidV1 { job_id, bidder, price, comment } => {
                                                    tracing::info!(
                                                        "[MeshNode] Received JobBidV1 for JobID: {} from Bidder: {} with Price: {}. Comment: {:?}. Topic: {}",
                                                        job_id, bidder, price, comment, message.topic
                                                    );

                                                    let current_timestamp = Utc::now().timestamp();
                                                    let new_bid = crate::protocol::Bid {
                                                        job_id: job_id.clone(), // Clone if IcnJobId is String
                                                        bidder: bidder.clone(),   // Clone Did
                                                        price,
                                                        timestamp: current_timestamp,
                                                        comment: comment.clone(), // Clone Option<String>
                                                    };

                                                    match self.bids.write() {
                                                        Ok(mut bids_map) => {
                                                            bids_map.entry(job_id.clone()).or_default().push(new_bid);
                                                            tracing::info!("[MeshNode] Stored bid for JobID: {}. Total bids for job: {}", 
                                                                         job_id, bids_map.get(&job_id).map_or(0, |b_vec| b_vec.len()));
                                                        }
                                                        Err(e) => {
                                                            tracing::error!("[MeshNode] Failed to get write lock for bids map while storing bid for job {}: {:?}", job_id, e);
                                                        }
                                                    }
                                                    // TODO: If this node is the job originator, it might trigger bid evaluation/selection logic here or in a periodic task.
                                                }
                                                // Handle other message types like JobInteractiveInputV1, etc.
                                                _ => {
                                                    // println!("Received unhandled MeshProtocolMessage type: {}", protocol_message.name());
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to deserialize MeshProtocolMessage: {:?}. Data: {:?}", e, message.data);
                                        }
                                    }
                                }
                                // Handle other gossipsub events if necessary (e.g., Subscription, Unsubscription)
                                _ => {}
                            }
                            MeshBehaviourEvent::Kademlia(kademlia_event) => {
                                match kademlia_event {
                                    KademliaEvent::OutboundQueryProgressed {
                                        id, 
                                        result: QueryResult::GetRecord(Ok(GetRecordOk::FoundRecord(record))),
                                        .. 
                                    } => {
                                        tracing::debug!("[KAD] GetRecord FoundRecord for QueryId: {:?}. PeerId: {:?}", id, record.peer);
                                        if let Some(tx) = self.receipt_queries.lock().unwrap().remove(&id) {
                                            tracing::info!("[KAD] Found pending receipt query for QueryId: {:?}. Sending record value.", id);
                                            if let Err(e) = tx.send(Ok(record.record.value)) {
                                                tracing::error!("[KAD] Failed to send Kademlia record value to oneshot channel for QueryId {:?}: (value not logged for brevity)", id);
                                            }
                                        } else {
                                            tracing::warn!("[KAD] GetRecord FoundRecord for QueryId: {:?}, but no pending oneshot sender found. Was it a different type of query or timed out?", id);
                                        }
                                    }
                                    KademliaEvent::OutboundQueryProgressed {
                                        id, 
                                        result: QueryResult::GetRecord(Ok(GetRecordOk::FinishedWithNoRecords)),
                                        .. 
                                    } => {
                                        tracing::debug!("[KAD] GetRecord FinishedWithNoRecords for QueryId: {:?}", id);
                                        if let Some(tx) = self.receipt_queries.lock().unwrap().remove(&id) {
                                            tracing::info!("[KAD] Found pending receipt query for QueryId: {:?}. Sending KadRecordNotFound error.", id);
                                            // We need the original CID to construct KadRecordNotFound error.
                                            // This is a limitation of this approach; ideally, store CID with sender.
                                            // For now, sending a generic query error.
                                            // To fix this, the HashMap value could be (oneshot::Sender<...>, Cid).
                                            if let Err(e) = tx.send(Err(FetchError::KadQueryError(Cid::default(), "FinishedWithNoRecords".to_string()))) { // Placeholder CID
                                                tracing::error!("[KAD] Failed to send Kademlia KadRecordNotFound to oneshot channel for QueryId {:?}: {:?}", id, e);
                                            }
                                        } else {
                                            tracing::warn!("[KAD] GetRecord FinishedWithNoRecords for QueryId {:?}, but no pending oneshot sender. Timed out?", id);
                                        }
                                    }
                                    KademliaEvent::OutboundQueryProgressed {
                                        id, 
                                        result: QueryResult::GetRecord(Err(err)),
                                        .. 
                                    } => {
                                        tracing::warn!("[KAD] GetRecord errored for QueryId: {:?}: {:?}", id, err);
                                        if let Some(tx) = self.receipt_queries.lock().unwrap().remove(&id) {
                                            tracing::info!("[KAD] Found pending receipt query for QueryId: {:?}. Sending KadQueryError.", id);
                                            // Similar to above, we need the original CID for a better error message.
                                            if let Err(e) = tx.send(Err(FetchError::KadQueryError(Cid::default(), err.to_string()))) { // Placeholder CID
                                                tracing::error!("[KAD] Failed to send Kademlia KadQueryError to oneshot channel for QueryId {:?}: {:?}", id, e);
                                            }
                                        } else {
                                            tracing::warn!("[KAD] GetRecord errored for QueryId {:?}, but no pending oneshot sender. Timed out?", id);
                                        }
                                    }
                                    // Handle other Kademlia events like PutRecord results, routing updates etc. if needed.
                                    _ => { 
                                        // tracing::trace!("[KAD] Unhandled KademliaEvent: {:?}", kademlia_event);
                                    }
                                }
                            }
                        }
                        SwarmEvent::NewListenAddr { address, .. } => {
                            println!("MeshNode listening on {}", address);
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                            println!("Connection established with peer: {}, endpoint: {:?}", peer_id, endpoint);
                        }
                        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                            println!("Connection closed with peer: {}, cause: {:?}", peer_id, cause.map(|c| c.to_string()));
                        }
                        // Handle other swarm events as needed
                        _ => { // Exhaustive match for other SwarmEvents
                            // println!("Unhandled SwarmEvent: {:?}", event);
                        }
                    }
                }
                // Select arm for internal actions
                Some(action) = internal_action_rx.recv() => {
                    match action {
                        NodeInternalAction::AnnounceReceipt { job_id, receipt_cid, executor_did } => {
                            tracing::info!("[EventLoop] Received internal action to announce receipt for job {}, CID: {}", job_id, receipt_cid);
                            let msg = MeshProtocolMessage::ExecutionReceiptAvailableV1 {
                                job_id: job_id.clone(),
                                receipt_cid: receipt_cid.to_string(),
                                executor_did: executor_did.clone(),
                            };

                            match serde_cbor::to_vec(&msg) {
                                Ok(bytes) => {
                                    if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(
                                        self.receipt_announcement_topic.clone(),
                                        bytes,
                                    ) {
                                        tracing::error!("[EventLoop] Failed to publish ExecutionReceiptAvailableV1 for {}: {:?}", job_id, e);
                                    } else {
                                        tracing::info!("[EventLoop] Published ExecutionReceiptAvailableV1 for job {}", job_id);
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("[EventLoop] Failed to serialize ExecutionReceiptAvailableV1 for job {}: {:?}", job_id, e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Helper to clone necessary Arcs for async tasks spawned from event loop
    // This is a simplified clone; a real one might need more careful consideration of what needs to be Arc<Mutex/RwLock<T>> vs what can be cloned directly.
    // For MeshNode methods that take `&self` or `&mut self` and are called from spawned tasks, `self` needs to be Arc-wrapped.
    // However, our `
}