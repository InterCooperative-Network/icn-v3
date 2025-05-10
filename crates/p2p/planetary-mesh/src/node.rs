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
        let message = MeshProtocolMessage::JobAnnouncementV1(job.clone()); // Clone job if it's used after this
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
            }
            Err(e) => {
                eprintln!("Error serializing job announcement message: {:?}", e);
                // Consider returning an error or specific result
                return Err(Box::new(e));
            }
        }
        Ok(())
    }

    pub async fn run_event_loop(mut self) {
        let mut capability_broadcast_interval = time::interval(Duration::from_secs(30));
        // Interval to check runtime queue and announce jobs
        let mut runtime_job_check_interval = time::interval(Duration::from_secs(5)); 

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
                                if message.topic == self.capability_gossip_topic.hash() {
                                    match serde_cbor::from_slice::<MeshProtocolMessage>(&message.data) {
                                        Ok(protocol_message) => match protocol_message {
                                            MeshProtocolMessage::CapabilityAdvertisementV1(capability) => {
                                                println!(
                                                    "Received CapabilityAdvertisementV1 from PeerID: {}\n  Node DID: {}\n  Resources: {:?}\n  Engines: {:?}\n  Load: {}\n  Region: {:?}",
                                                    peer_id,
                                                    capability.node_did,
                                                    capability.available_resources,
                                                    capability.supported_wasm_engines,
                                                    capability.current_load_factor,
                                                    capability.geographical_region
                                                );
                                            }
                                            _ => {
                                                eprintln!("Received unexpected message type on capability topic from PeerID {}", peer_id);
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to deserialize CAPABILITY gossip message data from PeerID {}: {:?}", peer_id, e);
                                        }
                                    }
                                } else if message.topic == self.job_announcement_topic.hash() {
                                    match serde_cbor::from_slice::<MeshProtocolMessage>(&message.data) {
                                        Ok(protocol_message) => match protocol_message {
                                            MeshProtocolMessage::JobAnnouncementV1(received_job) => {
                                                println!(
                                                    "Received JobAnnouncementV1 from PeerID: {}\n  Job ID: {}\n  Originator DID: {}\n  WASM CID: {}\n  Submitted At: {}",
                                                    peer_id,
                                                    received_job.job_id,
                                                    received_job.originator_did,
                                                    received_job.params.wasm_cid,
                                                    received_job.submitted_at
                                                );
                                                // Store received job
                                                match self.available_jobs_on_mesh.write() {
                                                    Ok(mut jobs_map) => {
                                                        jobs_map.insert(received_job.job_id.clone(), received_job);
                                                        println!("Stored job {} in available_jobs_on_mesh.", jobs_map.get(&received_job.job_id.clone()).unwrap().job_id);
                                                    }
                                                    Err(e) => {
                                                        eprintln!("Error locking available_jobs_on_mesh for write: {:?}", e);
                                                    }
                                                }
                                            }
                                            _ => {
                                                eprintln!("Received unexpected message type on job announcement topic from PeerID {}", peer_id);
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to deserialize JOB ANNOUNCEMENT gossip message data from PeerID {}: {:?}", peer_id, e);
                                        }
                                    }
                                } else {
                                   // println!("Received gossip message on unexpected topic: {:?}", message.topic);
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