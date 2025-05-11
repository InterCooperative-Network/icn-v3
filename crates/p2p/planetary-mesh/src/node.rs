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
use icn_types::mesh::{MeshJob, MeshJobParams, QoSProfile, JobId as IcnJobId, JobStatus as StandardJobStatus};
use icn_mesh_receipts::{ExecutionReceipt, sign_receipt_in_place, ReceiptError, SignError as ReceiptSignError}; // Added for receipt generation
use cid::Cid; // For storing receipt CIDs

// Access to RuntimeContext for anchoring receipts locally
use icn_runtime::context::RuntimeContext; 
use icn_runtime::host_environment::ConcreteHostEnvironment; // For calling anchor_receipt

use libp2p::gossipsub::TopicHash;

// Helper to create job-specific interest topic strings
fn job_interest_topic_string(job_id: &IcnJobId) -> String {
    format!("/icn/mesh/jobs/{}/interest/v1", job_id)
}

#[derive(Clone)]
pub struct MeshNode {
    swarm: Swarm<MeshBehaviour>,
    local_peer_id: PeerId,
    local_node_did: Did,
    local_keypair: IcnKeyPair, // Store keypair for signing receipts
    capability_gossip_topic: Topic,
    job_announcement_topic: Topic,
    pub available_jobs_on_mesh: Arc<RwLock<HashMap<IcnJobId, MeshJob>>>,
    pub runtime_job_queue_for_announcement: Arc<Mutex<VecDeque<MeshJob>>>,
    pub job_interests_received: Arc<RwLock<HashMap<IcnJobId, Vec<Did>>>>,
    pub announced_originated_jobs: Arc<RwLock<HashMap<IcnJobId, MeshJob>>>,

    // State for executor simulation
    pub executing_jobs: Arc<RwLock<HashMap<IcnJobId, MeshJob>>>,
    pub completed_job_receipt_cids: Arc<RwLock<HashMap<IcnJobId, Cid>>>,
    
    // Access to local runtime context for anchoring receipts
    runtime_context: Option<Arc<RuntimeContext>>, 
    pub discovered_receipt_announcements: Arc<RwLock<HashMap<IcnJobId, (Cid, Did)>>>,
}

impl MeshNode {
    pub async fn new(
        identity_keypair: IcnKeyPair, // Primary identity keypair
        listen_addr_opt: Option<String>,
        runtime_job_queue: Arc<Mutex<VecDeque<MeshJob>>>,
        local_runtime_context: Option<Arc<RuntimeContext>>,
    ) -> Result<Self, Box<dyn Error>> {
        
        // Derive libp2p keypair from the icn_identity::KeyPair secret key bytes
        // This assumes icn_identity::KeyPair's secret key part can be exposed or converted to a compatible format.
        // ed25519_dalek::SigningKey has to_bytes() -> [u8; 32], which is a seed.
        // libp2p ed25519 SecretKey can be from_bytes (seed).
        let libp2p_secret_key = Libp2pSecretKey::from_bytes(identity_keypair.to_bytes())?;
        let p2p_keypair = Libp2pKeypair::Ed25519(libp2p_secret_key.into());
        
        let local_peer_id = PeerId::from(p2p_keypair.public());
        let local_node_did_for_ops = identity_keypair.did.clone();

        println!("Local Peer ID: {}", local_peer_id);
        println!("Local Node DID for operations: {}", local_node_did_for_ops);

        let transport = libp2p::development_transport(p2p_keypair.clone()).await?;
        let behaviour = MeshBehaviour::new(&p2p_keypair)?;
        let mut swarm = Swarm::new(transport, behaviour, local_peer_id);

        let listen_addr = listen_addr_opt
            .unwrap_or_else(|| "/ip4/0.0.0.0/tcp/0".to_string())
            .parse()?;
        swarm.listen_on(listen_addr)?;

        let announced_originated_jobs = Arc::new(RwLock::new(HashMap::new()));
        let completed_job_receipt_cids = Arc::new(RwLock::new(HashMap::new()));
        let discovered_receipt_announcements = Arc::new(RwLock::new(HashMap::new()));

        Ok(Self {
            swarm,
            local_peer_id,
            local_node_did: local_node_did_for_ops,
            local_keypair: identity_keypair,
            capability_gossip_topic: Topic::new(CAPABILITY_TOPIC),
            job_announcement_topic: Topic::new(JOB_ANNOUNCEMENT_TOPIC),
            available_jobs_on_mesh: Arc::new(RwLock::new(HashMap::new())),
            runtime_job_queue_for_announcement: runtime_job_queue,
            job_interests_received: Arc::new(RwLock::new(HashMap::new())),
            announced_originated_jobs,
            executing_jobs: Arc::new(RwLock::new(HashMap::new())),
            completed_job_receipt_cids,
            runtime_context: local_runtime_context,
            discovered_receipt_announcements,
        })
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
        let message = MeshProtocolMessage::JobAnnouncementV1(job.clone());
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

                // Originator subscribes to the interest topic for this job
                let interest_topic_string = job_interest_topic_string(&job.job_id);
                let interest_topic = Topic::new(interest_topic_string.clone());
                match self.swarm.behaviour_mut().gossipsub.subscribe(&interest_topic) {
                    Ok(_) => println!("Subscribed to interest topic: {}", interest_topic_string),
                    Err(e) => eprintln!("Failed to subscribe to interest topic {}: {:?}", interest_topic_string, e),
                }

                // Add to our announced_originated_jobs map
                if let Ok(mut announced_jobs) = self.announced_originated_jobs.write() {
                    announced_jobs.insert(job.job_id.clone(), job.clone());
                    println!("Added job {} to announced_originated_jobs.", job.job_id);
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
        if let Some(rt_ctx) = &self.runtime_context {
            let host_env = ConcreteHostEnvironment::new(rt_ctx.clone(), self.local_node_did.clone());
            // anchor_receipt expects the receipt by value
            match host_env.anchor_receipt(receipt.clone()).await {
                Ok(_) => {
                    let anchored_receipt_cid = receipt.cid().map_err(|e| format!("Failed to get CID of anchored receipt: {}", e))?;
                    println!("Receipt successfully anchored for JobId: {}, Receipt CID: {}", job_id, anchored_receipt_cid);
                    self.completed_job_receipt_cids.write().unwrap().insert(job_id.clone(), anchored_receipt_cid);
                }
                Err(e) => {
                    eprintln!("Failed to anchor receipt for JobId {}: {:?}", job_id, e);
                    // TODO: Consider error handling, e.g., retrying or marking job as failed to anchor
                }
            }
        } else {
            eprintln!("No runtime_context available to anchor receipt for JobID: {}. Skipping anchoring.", job_id);
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

        Ok(())
    }

    pub async fn run_event_loop(&mut self) -> Result<(), Box<dyn Error>> {
        let mut capability_broadcast_interval = time::interval(Duration::from_secs(30));
        let mut runtime_job_check_interval = time::interval(Duration::from_secs(5));
        let mut express_interest_interval = time::interval(Duration::from_secs(15));
        let mut job_execution_check_interval = time::interval(Duration::from_secs(20)); // New interval for executor simulation

        // Known topic hashes for quick matching
        let capability_topic_hash = Topic::new(CAPABILITY_TOPIC).hash();
        let job_announcement_topic_hash = Topic::new(JOB_ANNOUNCEMENT_TOPIC).hash();

        loop {
            tokio::select! {
                _ = capability_broadcast_interval.tick() => {
                    if let Err(e) = self.broadcast_capabilities().await {
                        eprintln!("Failed to broadcast capabilities: {:?}", e);
                    }
                }
                _ = runtime_job_check_interval.tick() => {
                    let mut job_to_announce = None;
                    // Try to lock the runtime queue and get a job
                    match self.runtime_job_queue_for_announcement.lock() {
                        Ok(mut queue) => {
                            if let Some(job) = queue.pop_front() {
                                job_to_announce = Some(job);
                            }
                        }
                        Err(e) => {
                            eprintln!("Error locking runtime_job_queue_for_announcement: {:?}", e);
                        }
                    }

                    // If a job was retrieved, announce it
                    if let Some(job) = job_to_announce {
                        println!("Dequeued job {} from runtime for announcement.", job.job_id);
                        if let Err(e) = self.announce_job(job).await {
                            eprintln!("Failed to announce job from runtime queue: {:?}", e);
                            // Potentially re-queue the job or handle error
                        }
                    }
                }
                _ = express_interest_interval.tick() => {
                    // Iterate over available_jobs_on_mesh and express interest if suitable
                    if let Ok(jobs_map) = self.available_jobs_on_mesh.read() {
                        for (_job_id, job) in jobs_map.iter() {
                            // Avoid expressing interest in our own jobs
                            if job.originator_did != self.local_node_did {
                                if let Err(e) = self.evaluate_and_express_interest(job).await {
                                    eprintln!("Error during interest expression for job {}: {:?}", job.job_id, e);
                                }
                            }
                        }
                    } else {
                        eprintln!("Failed to get read lock on available_jobs_on_mesh for expressing interest.");
                    }
                }
                _ = job_execution_check_interval.tick() => {
                    let mut job_to_execute: Option<MeshJob> = None;
                    if let Ok(available_map) = self.available_jobs_on_mesh.read() {
                        for (_id, job) in available_map.iter() {
                            // Simple selection: not originated by self, and not already completed/executing
                            if job.originator_did != self.local_node_did && 
                               !self.executing_jobs.read().unwrap().contains_key(&job.job_id) &&
                               !self.completed_job_receipt_cids.read().unwrap().contains_key(&job.job_id) {
                                
                                // TODO: Add more sophisticated suitability check here, e.g., based on expressed interest or resource matching
                                println!("Considering job {} for execution.", job.job_id);
                                job_to_execute = Some(job.clone());
                                break; // Take the first suitable one for now
                            }
                        }
                    }

                    if let Some(job) = job_to_execute {
                        // Spawn as a new task to avoid blocking the event loop
                        let self_clone = Arc::new(self.clone_for_async_tasks()); 
                        tokio::spawn(async move {
                            if let Err(e) = self_clone.simulate_execution_and_anchor_receipt(job).await {
                                eprintln!("Error during simulated execution and anchoring: {:?}", e);
                            }
                        });
                    }
                }
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::Behaviour(MeshBehaviourEvent::Mdns(mdns_event)) => match mdns_event {
                            libp2p::mdns::Event::Discovered(list) => {
                                for (peer_id, _multiaddr) in list {
                                    println!("mDNS discovered a new peer: {}", peer_id);
                                    self.swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                }
                            }
                            libp2p::mdns::Event::Expired(list) => {
                                for (peer_id, _multiaddr) in list {
                                    println!("mDNS peer expired: {}", peer_id);
                                    self.swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                }
                            }
                        }
                        SwarmEvent::Behaviour(MeshBehaviourEvent::Gossipsub(gossip_event)) => match gossip_event {
                            libp2p::gossipsub::Event::Message {
                                propagation_source: peer_id,
                                message_id: id,
                                message,
                            } => {
                                trace!(
                                    "Received gossipsub message with id: {} from peer: {:?}, topic: {:?}",
                                    id, peer_id, message.topic
                                );
                                if message.topic == CAPABILITY_TOPIC {
                                    match serde_cbor::from_slice::<MeshProtocolMessage>(&message.data) {
                                        Ok(protocol_message) => match protocol_message {
                                            MeshProtocolMessage::CapabilityAdvertisementV1(capability) => {
                                                println!(
                                                    "Rxd CAPABILITY from {}: DID: {}, Res: {:?}, Eng: {:?}, Load: {}, Region: {:?}",
                                                    peer_id, capability.node_did, capability.available_resources,
                                                    capability.supported_wasm_engines, capability.current_load_factor, capability.geographical_region
                                                );
                                            }
                                            _ => {
                                                eprintln!("Rxd unexpected msg type on CAPABILITY topic from {}", peer_id);
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to deserialize CAPABILITY msg from {}: {:?}", peer_id, e);
                                        }
                                    }
                                } else if message.topic == JOB_ANNOUNCEMENT_TOPIC {
                                    match serde_cbor::from_slice::<MeshProtocolMessage>(&message.data) {
                                        Ok(protocol_message) => match protocol_message {
                                            MeshProtocolMessage::JobAnnouncementV1(received_job) => {
                                                println!(
                                                    "Rxd JOB_ANNOUNCEMENT from {}: JobID: {}, Originator: {}, WASM: {}, Submitted: {}",
                                                    peer_id, received_job.job_id, received_job.originator_did,
                                                    received_job.params.wasm_cid, received_job.submitted_at
                                                );
                                                match self.available_jobs_on_mesh.write() {
                                                    Ok(mut jobs_map) => {
                                                        let job_id_clone = received_job.job_id.clone();
                                                        jobs_map.insert(received_job.job_id.clone(), received_job);
                                                        // Safe to unwrap as we just inserted it.
                                                        println!("Stored job {} in available_jobs_on_mesh.", jobs_map.get(&job_id_clone).unwrap().job_id);
                                                    }
                                                    Err(e) => {
                                                        eprintln!("Error locking available_jobs_on_mesh for write: {:?}", e);
                                                    }
                                                }
                                            }
                                            _ => {
                                                eprintln!("Rxd unexpected msg type on JOB_ANNOUNCEMENT topic from {}", peer_id);
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to deserialize JOB_ANNOUNCEMENT msg from {}: {:?}", peer_id, e);
                                        }
                                    }
                                } else if message.topic == RECEIPT_AVAILABILITY_TOPIC_HASH {
                                    match serde_json::from_slice::<MeshProtocolMessage>(&message.data) {
                                        Ok(MeshProtocolMessage::ExecutionReceiptAvailableV1 {
                                            job_id,
                                            receipt_cid,
                                            executor_did,
                                        }) => {
                                            info!(
                                                "Received ExecutionReceiptAvailableV1 from {:?} for JobId: {}, Receipt CID: {}, Executor: {}",
                                                peer_id, job_id, receipt_cid, executor_did
                                            );

                                            match Cid::try_from(receipt_cid.as_str()) {
                                                Ok(parsed_cid) => {
                                                    self.discovered_receipt_announcements
                                                        .write()
                                                        .await
                                                        .insert(job_id.clone(), (parsed_cid, executor_did.clone()));
                                                    info!("Stored discovered receipt announcement for JobId: {}", job_id);

                                                    if self.announced_originated_jobs.read().await.contains_key(&job_id) {
                                                        info!(
                                                            "This node originated JobId: {}. Receipt is now available from Executor: {} with CID: {}.",
                                                            job_id, executor_did, receipt_cid
                                                        );
                                                    }
                                                }
                                                Err(e) => {
                                                    error!(
                                                        "Failed to parse receipt_cid '{}' from ExecutionReceiptAvailableV1 (from {:?}) for JobId {}: {:?}", 
                                                        receipt_cid, peer_id, job_id, e
                                                    );
                                                }
                                            }
                                } else {
                                    trace!("Received message on unhandled topic: {:?} from {:?}", message.topic, peer_id);
                                }
                            }
                            libp2p::gossipsub::Event::Subscribed { peer_id, topic } => {
                                println!(
                                    "Peer {} subscribed to topic: {:?}",
                                    peer_id,
                                    topic
                                );
                            }
                            _ => { /* Other gossipsub events */ }
                        }
                        SwarmEvent::NewListenAddr { address, .. } => {
                            println!("Local node listening on: {}", address);
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            println!("Connection established with: {}", peer_id);
                        }
                        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                            println!("Connection closed with: {}. Cause: {:?}", peer_id, cause);
                        }
                        _ => { /* Other swarm events */ }
                    }
                }
            }
        }
    }
    
    // Helper to clone necessary Arcs for async tasks spawned from event loop
    // This is a simplified clone; a real one might need more careful consideration of what needs to be Arc<Mutex/RwLock<T>> vs what can be cloned directly.
    // For MeshNode methods that take `&self` or `&mut self` and are called from spawned tasks, `self` needs to be Arc-wrapped.
    // However, our `simulate_execution_and_anchor_receipt` takes `&self` but acts on Arc fields, so it's okay if the task owns `self_clone`.
    // This method is primarily for if we needed to pass `MeshNode` itself into a context that requires `'static` lifetime or if the method was `&mut self`.
    // The current approach of `let self_clone = Arc::new(self.clone_for_async_tasks());` and then calling `self_clone.method()` on it
    // requires MeshNode to be Clone. Let's make it Clone. The Arcs within it make this a shallow clone, which is what we want.
    fn clone_for_async_tasks(&self) -> Self {
        // This relies on MeshNode deriving Clone. The Arcs will be cloned (incrementing ref count).
        // The KeyPair also needs to be Clone.
        self.clone()
    }
}

// Added helper for MeshProtocolMessage to get a name string for logging
impl MeshProtocolMessage {
    fn name(&self) -> &'static str {
        match self {
            MeshProtocolMessage::JobAnnouncementV1(_) => "JobAnnouncementV1",
            MeshProtocolMessage::CapabilityAdvertisementV1(_) => "CapabilityAdvertisementV1",
            MeshProtocolMessage::JobInterestV1 { .. } => "JobInterestV1",
            // Add other variants if they exist
        }
    }
} 