use crate::behaviour::{MeshBehaviour, MeshBehaviourEvent, CAPABILITY_TOPIC, JOB_ANNOUNCEMENT_TOPIC};
use crate::protocol::{MeshProtocolMessage, NodeCapability};
use futures::StreamExt;
use icn_identity::Did;
use libp2p::gossipsub::IdentTopic as Topic;
use libp2p::identity;
use libp2p::swarm::{Swarm, SwarmEvent};
use libp2p::{PeerId, Transport};
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;
use tokio::time;
use icn_economics::ResourceType; // For mock capability data
use icn_types::mesh::{MeshJob, MeshJobParams, QoSProfile}; // Added for MeshJob
use uuid::Uuid; // For generating mock JobId

pub struct MeshNode {
    swarm: Swarm<MeshBehaviour>,
    local_peer_id: PeerId,
    local_node_did: Did, // Store the DID string for capability construction
    capability_gossip_topic: Topic,
    job_announcement_topic: Topic, // Added for job announcements
    // TODO: Add a way to store received capabilities from other nodes
    // received_capabilities: Arc<Mutex<HashMap<PeerId, NodeCapability>>>,
    // TODO: Add a way to store received jobs
    // available_jobs: Arc<Mutex<HashMap<JobId, MeshJob>>>,
}

impl MeshNode {
    pub async fn new(node_did_str: String) -> Result<Self, Box<dyn Error>> {
        // Create a random PeerId (for now, this should ideally be persistent)
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        println!("Local Peer ID: {}", local_peer_id);
        println!("Local Node DID for capabilities: {}", node_did_str);

        // Set up an encrypted transport over TCP.
        let transport = libp2p::development_transport(local_key.clone()).await?;

        // Create the Mesh custom network behaviour.
        let behaviour = MeshBehaviour::new(&local_key)?;

        // Create the Swarm
        let mut swarm = Swarm::new(transport, behaviour, local_peer_id);

        // Listen on all interfaces and a random OS-assigned port.
        swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

        Ok(Self {
            swarm,
            local_peer_id,
            local_node_did: Did::parse(&node_did_str)?, // Convert string to Did type
            capability_gossip_topic: Topic::new(CAPABILITY_TOPIC),
            job_announcement_topic: Topic::new(JOB_ANNOUNCEMENT_TOPIC), // Initialize job announcement topic
            // received_capabilities: Arc::new(Mutex::new(HashMap::new())),
            // available_jobs: Arc::new(Mutex::new(HashMap::new()))
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

    fn construct_mock_job(&self) -> MeshJob {
        // Create mock MeshJobParams
        let params = MeshJobParams {
            wasm_cid: "bafyreigdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(), // Placeholder CID
            input_data_cid: Some("bafyreigcafecafebeeffeedbeeffeedbeeffeedbeeffeedbeeffeedbeeffeed".to_string()), // Placeholder CID
            required_resources_json: r#"{ "min_memory_mb": 512, "min_cpu_cores": 1 }"#.to_string(),
            qos_profile_json: serde_json::to_string(&QoSProfile::Balanced).unwrap_or_default(),
            max_acceptable_bid_tokens: Some(100),
            environment_vars: Some(vec![("KEY".to_string(), "VALUE".to_string())]),
        };

        // Create and return the mock MeshJob
        MeshJob {
            job_id: Uuid::new_v4().to_string(), // Generate a unique JobId
            originator_did: self.local_node_did.clone(),
            params,
            submitted_at: chrono::Utc::now(),
            // status will be set by runtime/orchestrator later
        }
    }

    pub async fn run_event_loop(mut self) {
        let mut broadcast_interval = time::interval(Duration::from_secs(30)); // Broadcast capabilities every 30s
        let mut job_announce_interval = time::interval(Duration::from_secs(45)); // Broadcast mock job every 45s

        loop {
            tokio::select! {
                _ = broadcast_interval.tick() => {
                    if let Err(e) = self.broadcast_capabilities().await {
                        eprintln!("Failed to broadcast capabilities: {:?}", e);
                    }
                }
                _ = job_announce_interval.tick() => {
                    let mock_job = self.construct_mock_job();
                    if let Err(e) = self.announce_job(mock_job).await {
                        eprintln!("Failed to announce mock job: {:?}", e);
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
                                message_id: id, // Renamed from _message_id to avoid unused warning
                                message,
                            } => {
                                if message.topic == self.capability_gossip_topic.hash() {
                                    match serde_cbor::from_slice::<MeshProtocolMessage>(&message.data) {
                                        Ok(protocol_message) => match protocol_message {
                                            MeshProtocolMessage::CapabilityAdvertisementV1(capability) => {
                                                println!(
                                                    "Received CapabilityAdvertisementV1 from PeerID: {}
  Node DID: {}
  Resources: {:?}
  Engines: {:?}
  Load: {}
  Region: {:?}",
                                                    peer_id,
                                                    capability.node_did,
                                                    capability.available_resources,
                                                    capability.supported_wasm_engines,
                                                    capability.current_load_factor,
                                                    capability.geographical_region
                                                );
                                                // TODO: Store received capability
                                                // let mut caps = self.received_capabilities.lock().unwrap();
                                                // caps.insert(peer_id, capability);
                                            }
                                            _ => {
                                                // Handle other message types on this topic if any, or log unexpected
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
                                                    "Received JobAnnouncementV1 from PeerID: {}
  Job ID: {}
  Originator DID: {}
  WASM CID: {}
  Submitted At: {}",
                                                    peer_id,
                                                    received_job.job_id,
                                                    received_job.originator_did,
                                                    received_job.params.wasm_cid,
                                                    received_job.submitted_at
                                                );
                                                // TODO: Store received job in available_jobs
                                                // let mut jobs = self.available_jobs.lock().unwrap();
                                                // jobs.insert(received_job.job_id.clone(), received_job);
                                            }
                                            _ => {
                                                // Handle other message types on this topic if any, or log unexpected
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