use crate::behaviour::{MeshBehaviour, MeshBehaviourEvent, CAPABILITY_TOPIC, JOB_ANNOUNCEMENT_TOPIC};
use crate::protocol::{MeshProtocolMessage, NodeCapability};
use futures::StreamExt;
use icn_identity::Did;
use libp2p::gossipsub::IdentTopic as Topic;
use libp2p::identity;
use libp2p::swarm::{Swarm, SwarmEvent};
use libp2p::{PeerId, Transport};
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::time;
use icn_economics::ResourceType; // For mock capability data
use icn_types::mesh::{MeshJob, MeshJobParams, QoSProfile, JobId as IcnJobId}; // Added for MeshJob, renamed JobId to avoid conflict
use uuid::Uuid; // For generating mock JobId
use libp2p::gossipsub::TopicHash;

// Helper to create job-specific interest topic strings
fn job_interest_topic_string(job_id: &IcnJobId) -> String {
    format!("/icn/mesh/jobs/{}/interest/v1", job_id)
}

pub struct MeshNode {
    swarm: Swarm<MeshBehaviour>,
    local_peer_id: PeerId,
    local_node_did: Did, // Store the DID string for capability construction
    capability_gossip_topic: Topic,
    job_announcement_topic: Topic,
    // Stores jobs received from the P2P network
    pub available_jobs_on_mesh: Arc<RwLock<HashMap<IcnJobId, MeshJob>>>,
    // Queue for jobs from the ICN Runtime to be announced on the P2P network
    pub runtime_job_queue_for_announcement: Arc<Mutex<VecDeque<MeshJob>>>,
    // Stores DIDs of nodes interested in jobs this node originated
    pub job_interests_received: Arc<RwLock<HashMap<IcnJobId, Vec<Did>>>>,
    // Stores jobs originated by this node and successfully announced
    pub announced_originated_jobs: Arc<RwLock<HashMap<IcnJobId, MeshJob>>>,
}

impl MeshNode {
    pub async fn new(
        node_did_str: String, // Node's DID
        keypair_opt: Option<identity::Keypair>, // Optional pre-generated keypair
        listen_addr_opt: Option<String>, // Optional listen address
        runtime_job_queue: Arc<Mutex<VecDeque<MeshJob>>>, // Queue from Runtime
    ) -> Result<Self, Box<dyn Error>> {
        let local_key = keypair_opt.unwrap_or_else(identity::Keypair::generate_ed25519);
        let local_peer_id = PeerId::from(local_key.public());
        println!("Local Peer ID: {}", local_peer_id);
        println!("Local Node DID for capabilities: {}", node_did_str);

        let transport = libp2p::development_transport(local_key.clone()).await?;
        let behaviour = MeshBehaviour::new(&local_key)?;
        let mut swarm = Swarm::new(transport, behaviour, local_peer_id);

        let listen_addr = listen_addr_opt
            .unwrap_or_else(|| "/ip4/0.0.0.0/tcp/0".to_string())
            .parse()?;
        swarm.listen_on(listen_addr)?;

        Ok(Self {
            swarm,
            local_peer_id,
            local_node_did: Did::parse(&node_did_str)?,
            capability_gossip_topic: Topic::new(CAPABILITY_TOPIC),
            job_announcement_topic: Topic::new(JOB_ANNOUNCEMENT_TOPIC),
            available_jobs_on_mesh: Arc::new(RwLock::new(HashMap::new())),
            runtime_job_queue_for_announcement: runtime_job_queue,
            job_interests_received: Arc::new(RwLock::new(HashMap::new())),
            announced_originated_jobs: Arc::new(RwLock::new(HashMap::new())),
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

    pub async fn run_event_loop(mut self) {
        let mut capability_broadcast_interval = time::interval(Duration::from_secs(30));
        // Interval to check runtime queue and announce jobs
        let mut runtime_job_check_interval = time::interval(Duration::from_secs(5)); 
        let mut express_interest_interval = time::interval(Duration::from_secs(15)); // New interval

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
                                message_id: _id, // Marked as unused
                                message,
                            } => {
                                let msg_topic_hash = &message.topic;
                                if msg_topic_hash == &capability_topic_hash {
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
                                } else if msg_topic_hash == &job_announcement_topic_hash {
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
                                } else {
                                    // Potentially an interest message on a dynamic topic
                                    match serde_cbor::from_slice::<MeshProtocolMessage>(&message.data) {
                                        Ok(protocol_message) => match protocol_message {
                                            MeshProtocolMessage::JobInterestV1 { job_id, executor_did } => {
                                                println!(
                                                    "Rxd JOB_INTEREST from {}: JobID: {}, Interested Executor DID: {}",
                                                    peer_id, job_id, executor_did
                                                );
                                                // Check if this node is the originator of the job_id
                                                // (implicitly, if we are subscribed, we might be an originator, 
                                                // or if we sent an interest and somehow got subscribed to our own interest by mistake - less likely)
                                                // For now, we will store if the job_id key exists, created upon announcement by self.
                                                match self.job_interests_received.write() {
                                                    Ok(mut interests_map) => {
                                                        // Ensure the entry exists if this node originated the job
                                                        // It should have been created in announce_job
                                                        interests_map.entry(job_id.clone()).or_default().push(executor_did.clone());
                                                        println!("Stored interest for job {} from DID {}. Total interests: {}", 
                                                                 job_id, executor_did, interests_map.get(&job_id).unwrap().len());
                                                    }
                                                    Err(e) => {
                                                        eprintln!("Error locking job_interests_received for write: {:?}", e);
                                                    }
                                                }
                                            }
                                            // Handle other message types if any were to arrive on dynamic topics
                                            _ => {
                                                // This case might be hit if a message on a dynamic topic isn't JobInterestV1
                                                // Or if it's a message on a topic we didn't expect.
                                                // Consider logging the topic hash for debugging: message.topic.to_string()
                                                eprintln!("Rxd msg on DYNAMIC TOPIC ({}) from {}, but not JobInterestV1. Type: {:?}", 
                                                         message.topic, peer_id, protocol_message.name());
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to deserialize msg on DYNAMIC TOPIC ({}) from {}: {:?}", message.topic, peer_id, e);
                                        }
                                    }
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