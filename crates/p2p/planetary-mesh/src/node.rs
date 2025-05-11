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
use icn_mesh_receipts::{ExecutionReceipt, sign_receipt_in_place, ReceiptError, SignError as ReceiptSignError}; // Added for receipt generation
use cid::Cid; // For storing receipt CIDs
use chrono::{TimeZone, Utc}; // For timestamp conversion

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
    pub announced_originated_jobs: Arc<RwLock<HashMap<IcnJobId, super::JobManifest>>>,

    // State for executor simulation
    pub executing_jobs: Arc<RwLock<HashMap<IcnJobId, super::JobManifest>>>,
    pub completed_job_receipt_cids: Arc<RwLock<HashMap<IcnJobId, Cid>>>,
    pub assigned_jobs: Arc<RwLock<HashMap<IcnJobId, MeshJob>>>,
    
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
        let assigned_jobs = Arc::new(RwLock::new(HashMap::new()));
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
            assigned_jobs,
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

    async fn assign_job_to_executor(
        &mut self,
        job_id: &IcnJobId,
        target_executor_did: Did,
    ) -> Result<(), Box<dyn Error>> {
        let job_details: MeshJob;
        // Retrieve the job details from announced_originated_jobs
        {
            let announced_jobs = self.announced_originated_jobs.read().map_err(|_| "Failed to get read lock for announced_originated_jobs")?;
            if let Some(job) = announced_jobs.get(job_id) {
                job_details = job.clone();
            } else {
                eprintln!("Cannot assign job {}: Not found in originated jobs.", job_id);
                return Err(format!("Job {} not found in originated jobs", job_id).into());
            }
        }

        println!(
            "Assigning JobID: {} from Originator DID: {} to Executor DID: {}",
            job_id, self.local_node_did, target_executor_did
        );

        let assignment_message = MeshProtocolMessage::AssignJobV1 {
            job_id: job_id.clone(),
            originator_did: self.local_node_did.clone(),
            target_executor_did: target_executor_did.clone(),
            job_details: job_details.clone(),
        };

        match serde_cbor::to_vec(&assignment_message) {
            Ok(serialized_message) => {
                let interest_topic_string = job_interest_topic_string(job_id);
                let assignment_topic = Topic::new(interest_topic_string.clone()); // Publish on the job's interest topic

                self.swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(assignment_topic, serialized_message)?;
                
                println!(
                    "Published AssignJobV1 for JobID: {} to Executor: {} on topic: {}",
                    job_id, target_executor_did, interest_topic_string
                );

                if let Ok(mut announced_jobs) = self.announced_originated_jobs.write() {
                    if let Some(job_entry) = announced_jobs.get_mut(job_id) {
                        // Placeholder for updating job status. MeshJob might need a status field or a wrapper like JobManifest.
                        // For now, we assume the local JobManifest (if we were using one) would be updated.
                        // If MeshJob is directly stored and doesn't have a mutable status, this part needs refinement.
                        // job_entry.status = StandardJobStatus::Assigned { node_id: target_executor_did.clone() }; 
                        println!("Originated Job {} status conceptually updated to Assigned to {}", job_id, target_executor_did);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error serializing AssignJobV1 message for job {}: {:?}", job_id, e);
                return Err(Box::new(e));
            }
        }
        Ok(())
    }

    pub async fn run_event_loop(&mut self) -> Result<(), Box<dyn Error>> {
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
                        if let Ok(mut queue) = self.runtime_job_queue_for_announcement.lock() {
                            while let Some(job) = queue.pop_front() {
                                jobs_to_announce.push(job);
                            }
                        } else {
                            eprintln!("Failed to lock runtime_job_queue_for_announcement");
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
                        if let Err(e) = self.assign_job_to_executor(&job_id, executor_did.clone()).await {
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
                                            // println!("Received Gossipsub message: {:?} from topic: {}", protocol_message.name(), message.topic);
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
                                                MeshProtocolMessage::ExecutionReceiptAvailableV1{ job_id, receipt_cid, executor_did } => {
                                                    println!("Received ExecutionReceiptAvailableV1 for JobID: {} with CID: {} from {}", job_id, receipt_cid, executor_did);
                                                    if let Ok(mut announcements) = self.discovered_receipt_announcements.write() {
                                                        if let Ok(cid_obj) = Cid::try_from(receipt_cid.clone()) { // Convert String to Cid
                                                            announcements.insert(job_id, (cid_obj, executor_did));
                                                        } else {
                                                            eprintln!("Failed to parse receipt_cid {} into Cid object.", receipt_cid);
                                                        }
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
                                                        eprintln!("Failed to get write lock for announced_originated_jobs while handling status update for {}.
", job_id);
                                                    }
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