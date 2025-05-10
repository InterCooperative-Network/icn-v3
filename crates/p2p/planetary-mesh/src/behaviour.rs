use libp2p::{
    gossipsub::{self, IdentTopic as Topic, MessageId, PublishError, TopicHash},
    mdns::tokio::Behaviour as Mdns,
    swarm::NetworkBehaviour,
};
use serde::{Deserialize, Serialize};
use crate::protocol::MeshProtocolMessage;

// Define a topic for capability advertisements
// It's crucial that all nodes use the same topic string.
pub const CAPABILITY_TOPIC: &str = "/icn/mesh/capabilities/v1";

// Define a new topic for job announcements
pub const JOB_ANNOUNCEMENT_TOPIC: &str = "/icn/mesh/jobs/announcements/v1";

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "MeshBehaviourEvent")]
pub struct MeshBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: Mdns,
}

impl MeshBehaviour {
    pub fn new(keypair: &libp2p::identity::Keypair) -> Result<Self, String> {
        let message_id_fn = |message: &gossipsub::Message| {
            // Use a hash of the message data as the message ID
            // This helps prevent duplicate messages if the same content is received from multiple peers.
            let mut s = std::collections::hash_map::DefaultHasher::new();
            message.data.hash(&mut s);
            MessageId::from(std::hash::Hasher::finish(&s).to_string())
        };

        // Set up gossipsub configuration
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(std::time::Duration::from_secs(10))
            .validation_mode(gossipsub::ValidationMode::Strict) // Or Anonymous if identities are not strictly managed yet
            .message_id_fn(message_id_fn)
            .build()
            .map_err(|e| format!("Failed to build gossipsub config: {}", e))?;

        // Build gossipsub behaviour
        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(keypair.clone()), // Messages are signed by the publisher
            gossipsub_config,
        ).map_err(|e| format!("Failed to create gossipsub behaviour: {}", e))?;

        // Subscribe to the capability topic
        let capability_topic = Topic::new(CAPABILITY_TOPIC);
        gossipsub.subscribe(&capability_topic)
            .map_err(|e| format!("Failed to subscribe to capability topic: {:?}", e))?;

        // Subscribe to the job announcement topic
        let job_announcement_topic = Topic::new(JOB_ANNOUNCEMENT_TOPIC);
        gossipsub.subscribe(&job_announcement_topic)
            .map_err(|e| format!("Failed to subscribe to job announcement topic: {:?}", e))?;

        // Configure mDNS for local peer discovery
        // The mdns::Config::default() should work for basic local discovery.
        let mdns = Mdns::new(libp2p::mdns::Config::default())
            .map_err(|e| format!("Failed to create mDNS behaviour: {}", e))?;

        Ok(Self {
            gossipsub,
            mdns,
        })
    }
}

/// Events emitted by the MeshBehaviour to be handled by the Swarm event loop.
#[derive(Debug)]
pub enum MeshBehaviourEvent {
    Gossipsub(gossipsub::Event),
    Mdns(libp2p::mdns::Event),
}

impl From<gossipsub::Event> for MeshBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        MeshBehaviourEvent::Gossipsub(event)
    }
}

impl From<libp2p::mdns::Event> for MeshBehaviourEvent {
    fn from(event: libp2p::mdns::Event) -> Self {
        MeshBehaviourEvent::Mdns(event)
    }
} 