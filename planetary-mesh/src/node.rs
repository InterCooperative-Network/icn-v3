use tokio::sync::mpsc::{self, Receiver, Sender};
use icn_types::mesh::MeshJob;
use crate::protocol::{Bid, MeshProtocolMessage};
use libp::gossipsub::IdentTopic as Topic;
use icn_types::mesh::{JobStatus as StandardJobStatus, ExecutionReceipt};
use icn_types::reputation::{ReputationRecord, ReputationUpdateEvent};
use cid::Cid;
use icn_identity::Did;
use icn_types::jobs::policy::ExecutionPolicy;
use std::cmp::Ordering;
use cid::{Cid, multihash::{Code, MultihashDigest}};
use libipld_cbor::DagCborCodec; // For Codec::Raw
use libipld_core::ipld::IpldCodec;
use std::collections::HashMap;
use once_cell::sync::Lazy;
use icn_identity::IcnPublicKey;

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
    pub verified_reputation_records: Arc<RwLock<HashMap<Cid, ReputationRecord>>>,
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
    pub anchor_cid: Option<Cid>,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct ScoredBid {
    pub bid: Bid,
    pub score: f64,
}

// Static map for known DIDs and their public keys for testing
static TEST_PUBLIC_KEYS: Lazy<HashMap<Did, IcnPublicKey>> = Lazy::new(|| {
    let mut m = HashMap::new();
    // --- REPLACE WITH YOUR ACTUAL TEST DIDs AND PUBLIC KEYS ---
    // Example 1: Replace with a real DID string and corresponding public key bytes
    // let did1_str = "did:example:issuer1".to_string();
    // let pk1_bytes: [u8; 32] = [/* ... 32 bytes of Ed25519 public key ... */]; 
    // if let Ok(icn_pk1) = IcnPublicKey::from_bytes(&pk1_bytes) { // Assuming from_bytes or similar constructor
    //     m.insert(Did::new(&did1_str), icn_pk1);
    // }

    // Example 2: Replace with another real DID string and public key bytes
    // let did2_str = "did:example:issuer2".to_string();
    // let pk2_bytes: [u8; 32] = [/* ... 32 bytes of another Ed25519 public key ... */];
    // if let Ok(icn_pk2) = IcnPublicKey::from_bytes(&pk2_bytes) {
    //     m.insert(Did::new(&did2_str), icn_pk2);
    // }
    // --- END OF REPLACE SECTION ---
    
    if m.is_empty() {
        tracing::warn!("Test public key map is empty. DID resolution will likely fail for tests.");
    }
    m
});

// Placeholder for DID resolution - replace with actual implementation
fn resolve_did_to_public_key(did: &Did) -> Option<IcnPublicKey> {
    tracing::debug!("Attempting to resolve DID: {} to a public key (using static test map)", did);
    TEST_PUBLIC_KEYS.get(did).cloned() // Cloned because IcnPublicKey might not be Copy
}

fn verify_reputation_record_signature(record: &ReputationRecord) -> Result<(), String> {
    use icn_types::reputation::get_reputation_record_signing_payload;
    // Assuming IcnPublicKey is available from icn_identity and has a verify method
    // Assuming Signature is the type used in ReputationRecord and by IcnPublicKey::verify
    use icn_identity::IcnPublicKey; 

    // Step 1: Recreate the signing payload
    // The get_reputation_record_signing_payload function expects a record without a signature for payload generation.
    // So, we clone the record and clear its signature field before generating the payload.
    let mut record_for_payload_generation = record.clone();
    record_for_payload_generation.signature = None;

    let payload = get_reputation_record_signing_payload(&record_for_payload_generation)
        .map_err(|e| format!("Failed to get signing payload for reputation record: {:?}", e))?;

    // Step 2: Resolve the DID to public key
    let public_key = resolve_did_to_public_key(&record.issuer)
        .ok_or_else(|| format!("Could not resolve public key for issuer DID: {}", record.issuer))?;

    // Step 3: Verify signature
    let signature_to_verify = record.signature.as_ref()
        .ok_or_else(|| "ReputationRecord is missing a signature to verify".to_string())?;
    
    // Assuming IcnPublicKey has a method like `verify(&self, msg: &[u8], signature: &YourSignatureType) -> Result<(), Error>`
    // Adjust if your `verify` method or `Signature` type is different.
    public_key.verify(&payload, signature_to_verify)
        .map_err(|e| format!("Signature verification failed for issuer {}: {:?}", record.issuer, e))
}

fn evaluate_bid_against_policy(
    bid: &Bid,
    policy: &ExecutionPolicy,
    executor_reputation_score: f64,
) -> Option<ScoredBid> {
    if let Some(max_price_val) = policy.max_price {
        if bid.price > max_price_val {
            tracing::debug!(
                "Bid from {} for job {} rejected: price {} > max_price {}",
                bid.executor_did, bid.job_id, bid.price, max_price_val
            );
            return None;
        }
    }

    if let Some(min_rep_val) = policy.min_reputation {
        if executor_reputation_score < min_rep_val {
            tracing::debug!(
                "Bid from {} for job {} rejected: reputation {} < min_reputation_score {}",
                bid.executor_did, bid.job_id, executor_reputation_score, min_rep_val
            );
            return None;
        }
    }

    let max_price_for_scoring_f64 = policy.max_price.map(|mp| mp as f64).unwrap_or(bid.price as f64 * 2.0);
    
    let price_component = if max_price_for_scoring_f64 > 0.0 {
        (1.0 - (bid.price as f64 / max_price_for_scoring_f64)).max(0.0).min(1.0)
    } else {
        1.0
    }

    let reputation_component = (executor_reputation_score / 100.0).max(0.0).min(1.0);

    let total_score =
        policy.price_weight * price_component +
        policy.rep_weight * reputation_component;
    
    tracing::debug!(
        "Bid from {} for job {}: price_comp={}, rep_comp={}, total_score={}",
        bid.executor_did, bid.job_id, price_component, reputation_component, total_score
    );

    if total_score > 0.0 {
        Some(ScoredBid {
            bid: bid.clone(),
            score: total_score,
        })
    } else {
        tracing::debug!(
            "Bid from {} for job {} scored {} (<=0.0), not considered selectable by policy evaluation.",
            bid.executor_did, bid.job_id, total_score
        );
        None
    }
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

        let mut swarm: Swarm<MeshBehaviour> = vervangen_door_uw_daadwerkelijke_swarm_creatie_logica_hier();

        // Subscribe to the reputation records topic
        let reputation_topic = Topic::new("reputation-records-v1".to_string());
        match swarm.behaviour_mut().gossipsub.subscribe(&reputation_topic) {
            Ok(subscribed) => {
                if subscribed {
                    tracing::info!("Successfully subscribed to '{}' gossipsub topic.", reputation_topic.hash());
                } else {
                    // This case should ideally not happen if subscribe returns Ok(true) for new subscription
                    tracing::warn!("Subscription to '{}' topic reported Ok(false), might already be subscribed or other issue.", reputation_topic.hash());
                }
            }
            Err(e) => {
                tracing::error!("Failed to subscribe to '{}' gossipsub topic: {:?}", reputation_topic.hash(), e);
                // Depending on policy, you might want to return an error here:
                // return Err(anyhow::anyhow!("Failed to subscribe to reputation topic: {}", e));
            }
        }

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
                verified_reputation_records: Arc::new(RwLock::new(HashMap::new())),
            },
            internal_action_rx,
        ))
    }

    // Helper function to calculate reputation score from verified records
    fn calculate_reputation_score_for_did(
        verified_records_map: &HashMap<Cid, ReputationRecord>,
        target_did: &Did
    ) -> f64 {
        let mut successful_jobs = 0;
        let mut failed_jobs = 0;

        for record in verified_records_map.values() {
            if record.subject == *target_did {
                match &record.event {
                    ReputationUpdateEvent::JobCompletedSuccessfully { .. } => {
                        successful_jobs += 1;
                    }
                    ReputationUpdateEvent::JobFailed { .. } => {
                        failed_jobs += 1;
                    }
                    // Add other event types if they influence reputation
                }
            }
        }

        let base_score = 50.0;
        let success_bonus = 10.0; 
        let failure_penalty = 15.0; 
        
        let mut score = base_score + (successful_jobs as f64 * success_bonus) - (failed_jobs as f64 * failure_penalty);
        score = score.max(0.0).min(100.0);

        if successful_jobs == 0 && failed_jobs == 0 {
            return 50.0; 
        }
        score
    }

    async fn publish_bid_message(&mut self, bid: Bid) -> Result<(), anyhow::Error> {
        tracing::info!("Publishing bid for job_id: {} from executor: {}", bid.job_id, bid.executor_did);
        let topic = Topic::new(format!("job-bids/{}", bid.job_id));
        let message = MeshProtocolMessage::JobBidV1 {
            job_id: bid.job_id.clone(),
            executor_did: bid.executor_did.clone(),
            price: bid.price,
            comment: bid.comment.clone(),
            region: bid.region.clone(),
        };
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

        // --- Start of new anchoring logic ---
        // 1. Serialize the signed ReputationRecord to CBOR
        let reputation_record_cbor = match serde_cbor::to_vec(&final_reputation_record) {
            Ok(cbor) => cbor,
            Err(e) => {
                tracing::error!("Failed to serialize final reputation record to CBOR for job {}: {:?}", job_id, e);
                return Err(e.into()); // Propagate error or handle differently
            }
        };

        // 2. Compute its CID
        // Using SHA2-256 (Code::Sha2_256) and Raw CBOR codec (IpldCodec::DagCbor.into() gives 0x71)
        let hash = Code::Sha2_256.digest(&reputation_record_cbor);
        let record_cid = Cid::new_v1(IpldCodec::DagCbor.into(), hash);
        tracing::info!("Calculated CID for reputation record of job {}: {}", job_id, record_cid);

        let mut observed_anchor_cid_for_test: Option<Cid> = None;

        // 3. Store it in the local DAG (dag_store)
        if let Some(runtime_ctx) = &self.local_runtime_context {
            // Assuming runtime_ctx.dag_store() returns Arc<RwLock<DagStore>>
            // And DagStore has a method like add_dag_node or put
            match runtime_ctx.dag_store().write().unwrap().add_dag_node(record_cid, reputation_record_cbor.clone()) {
                Ok(_) => {
                    tracing::info!(
                        "Successfully anchored reputation record with CID {} for job {} in local DAG store.",
                        record_cid, job_id
                    );
                    observed_anchor_cid_for_test = Some(record_cid);
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to anchor reputation record CID {} for job {} in DAG store: {:?}",
                        record_cid, job_id, e
                    );
                }
            }
        } else {
            tracing::warn!("No local runtime context available. Skipping DAG anchoring for reputation record of job {}.", job_id);
        }
        // --- End of new anchoring logic ---

        // HTTP submission (existing logic)
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
            anchor_cid: observed_anchor_cid_for_test,
            timestamp: Utc::now().timestamp(),
        };

        if let Err(e) = self.internal_action_tx.send(InternalNodeAction::ReputationSubmittedForTest(test_submission)).await {
            tracing::warn!("Failed to send ReputationSubmittedForTest internal action for job_id: {}: {:?}", job_id, e);
        }

        // --- Announce ReputationRecordAvailableV1 to the mesh ---
        if let Some(anchored_record_cid) = observed_anchor_cid_for_test {
            let announcement = ReputationRecordAvailableV1 {
                record_cid: anchored_record_cid.to_string(), // Convert Cid to String for the protocol message
                subject_did: receipt.executor.clone(),
                issuer_did: self.local_keypair.did.clone(), 
                job_id: job_id.clone(), // Assuming IcnJobId can be cloned and is compatible with protocol.JobId (String)
                execution_receipt_cid: receipt.cid.to_string(), // Convert Cid to String
            };

            let message = MeshProtocolMessage::ReputationRecordAvailableV1(announcement);
            let topic_name = "reputation-records-v1";
            let topic = Topic::new(topic_name.to_string());

            match serde_cbor::to_vec(&message) {
                Ok(cbor_payload) => {
                    if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(topic, cbor_payload) {
                        tracing::error!(
                            job_id = %job_id,
                            cid = %anchored_record_cid,
                            "Failed to publish ReputationRecordAvailableV1 to mesh: {:?}", e
                        );
                    } else {
                        tracing::info!(
                            job_id = %job_id,
                            cid = %anchored_record_cid,
                            subject = %receipt.executor,
                            "Announced ReputationRecordAvailableV1 to the mesh on topic {}.", topic_name
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        job_id = %job_id,
                        cid = %anchored_record_cid,
                        "Failed to serialize ReputationRecordAvailableV1 for publishing: {:?}", e
                    );
                }
            }
        }
        // --- End of announcement ---
        
        Ok(())
    }

    async fn fetch_reputation_record_cbor_via_kad(&mut self, record_cid: Cid) -> Result<Vec<u8>, String> {
        tracing::debug!("Attempting to fetch reputation record CBOR for CID: {} via Kademlia GET_RECORD", record_cid);
        let record_key = libp2p::kad::RecordKey::new(&record_cid.to_bytes());

        let (tx, rx) = oneshot::channel();
        let query_id = self.swarm.behaviour_mut().kademlia.get_record(record_key);
        self.pending_kad_fetches.write().unwrap().insert(query_id, tx);

        // Timeout for Kademlia GET operation
        // TODO: Make timeout duration configurable
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(result)) => {
                tracing::info!("Successfully received Kademlia GET_RECORD result for reputation record CID: {}", record_cid);
                result
            }
            Ok(Err(e)) => {
                tracing::warn!("Kademlia GET_RECORD oneshot channel error for reputation record CID: {}: {:?}", record_cid, e);
                Err(format!("Oneshot channel error for {}: {:?}", record_cid, e))
            }
            Err(_) => {
                tracing::warn!("Kademlia GET_RECORD timed out for reputation record CID: {}", record_cid);
                // Clean up the pending fetch on timeout to prevent leaks if KAD doesn't respond
                self.pending_kad_fetches.write().unwrap().remove(&query_id);
                Err(format!("Kademlia GET_RECORD timed out for {}", record_cid))
            }
        }
    }

    async fn handle_internal_action(&mut self, action: InternalNodeAction) -> Result<(), anyhow::Error> {
        match action {
            InternalNodeAction::ReputationSubmittedForTest(submission_data) => {
                tracing::debug!("Test: Recording observed reputation submission: {:?}", submission_data);
                self.test_observed_reputation_submissions.write().unwrap().push(submission_data);
            }
            InternalNodeAction::FetchReputationRecord { record_cid, subject_did, issuer_did } => {
                tracing::info!(
                    cid = %record_cid,
                    subject = %subject_did,
                    issuer = %issuer_did,
                    "Fetching ReputationRecord via Kademlia"
                );

                match self.fetch_reputation_record_cbor_via_kad(record_cid).await {
                    Ok(record_cbor) => {
                        // Step 1: Deserialize
                        match serde_cbor::from_slice::<ReputationRecord>(&record_cbor) {
                            Ok(reputation_record) => {
                                // Step 2: Recompute CID to verify against the requested CID
                                let recomputed_hash = Code::Sha2_256.digest(&record_cbor);
                                let recomputed_cid = Cid::new_v1(IpldCodec::DagCbor.into(), recomputed_hash);

                                if recomputed_cid != record_cid {
                                    tracing::warn!(
                                        expected = %record_cid,
                                        actual = %recomputed_cid,
                                        subject = %reputation_record.subject,
                                        "CID mismatch: fetched ReputationRecord data does not match expected CID"
                                    );
                                    return Ok(()); // Early exit if CID doesn't match
                                }

                                // Step 3: Verify signature
                                match verify_reputation_record_signature(&reputation_record) {
                                    Ok(_) => {
                                        tracing::info!(
                                            cid = %record_cid,
                                            issuer = %reputation_record.issuer,
                                            subject = %reputation_record.subject,
                                            "Signature on ReputationRecord is valid."
                                        );

                                        // Step 4: Store the verified record
                                        // The key is the CID of the reputation record itself.
                                        self.verified_reputation_records
                                            .write()
                                            .unwrap()
                                            .insert(record_cid, reputation_record.clone()); // Use record_cid as key

                                        tracing::info!(
                                            cid = %record_cid,
                                            subject = %reputation_record.subject,
                                            "Stored verified ReputationRecord."
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            cid = %record_cid,
                                            issuer = %reputation_record.issuer,
                                            subject = %reputation_record.subject,
                                            "ReputationRecord signature verification failed: {}", e
                                        );
                                        // Do not store if signature is invalid
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    cid = %record_cid,
                                    "Failed to deserialize fetched ReputationRecord CBOR: {:?}", e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            cid = %record_cid,
                            subject = %subject_did,
                            issuer = %issuer_did,
                            "Kademlia fetch for ReputationRecord failed: {:?}", e
                        );
                    }
                }
            }
            _ => {
                tracing::trace!("Unhandled or placeholder internal action: {:?}", action);
            }
        }
        Ok(())
    }

    async fn select_executor_for_originated_jobs(&mut self) -> Result<(), anyhow::Error> {
        let mut jobs_to_assign: Vec<(IcnJobId, JobManifest, MeshJob, Bid)> = Vec::new();
        let mut assigned_this_round = HashSet::new();

        let originated_jobs_map = self.announced_originated_jobs.read().unwrap().clone();
        let current_bids_map = self.bids.read().unwrap().clone();
        let verified_reputation_records_map = self.verified_reputation_records.read().unwrap(); // Get a read lock once

        for (job_id, (_job_manifest, original_mesh_job)) in originated_jobs_map.iter() {
            if self.assigned_by_originator.read().unwrap().contains(job_id) {
                continue;
            }

            if let Some(bids_for_job) = current_bids_map.get(job_id) {
                if bids_for_job.is_empty() {
                    continue;
                }

                let winning_bid_opt: Option<Bid> = 
                    if let Some(policy) = &original_mesh_job.params.execution_policy {
                        tracing::info!("Job {} has an execution policy. Evaluating bids against policy using verified reputation.", job_id);
                        
                        let scored_bids: Vec<ScoredBid> = bids_for_job.iter().filter_map(|bid| {
                            // Apply region_filter from policy if it exists
                            if let Some(ref required_region) = policy.region_filter {
                                // Access bid.region directly from crate::protocol::Bid
                                if bid.region.as_ref().map(|r| r.as_str()) != Some(required_region.as_str()) {
                                    tracing::debug!(
                                        bidder = %bid.executor_did, 
                                        job_id = %job_id, // job_id should be in scope here
                                        "Bidder filtered out by region policy in planetary-mesh. Required: {:?}, Bidder has: {:?}", 
                                        required_region, 
                                        bid.region
                                    );
                                    return None; // Skip this bid due to region mismatch
                                }
                            }

                            // Calculate reputation score using verified records
                            let actual_reputation_score = Self::calculate_reputation_score_for_did(
                                &verified_reputation_records_map, 
                                &bid.executor_did
                            );
                            tracing::debug!(
                                "Calculated reputation for DID {}: {} (based on {} successes, {} failures from verified records)", 
                                bid.executor_did, 
                                actual_reputation_score,
                                verified_reputation_records_map.values().filter(|r| r.subject == bid.executor_did && matches!(r.event, ReputationUpdateEvent::JobCompletedSuccessfully {..})).count(),
                                verified_reputation_records_map.values().filter(|r| r.subject == bid.executor_did && matches!(r.event, ReputationUpdateEvent::JobFailed {..})).count()
                            );
                            evaluate_bid_against_policy(bid, policy, actual_reputation_score)
                        }).collect();

                        if scored_bids.is_empty() {
                            tracing::warn!("No bids for job {} met policy criteria or scored positively with verified reputation.", job_id);
                            None
                        } else {
                            scored_bids.iter()
                                .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(Ordering::Equal))
                                .map(|scored_bid| {
                                    tracing::info!(
                                        "Policy-based selection for job {}: Winning bid from {} with score {} (Rep: {})",
                                        job_id, scored_bid.bid.executor_did, scored_bid.score,
                                        Self::calculate_reputation_score_for_did(&verified_reputation_records_map, &scored_bid.bid.executor_did)
                                    );
                                    scored_bid.bid.clone()
                                })
                        }
                    } else {
                        tracing::info!("Job {} has no execution policy. Selecting by lowest price.", job_id);
                        bids_for_job.iter().min_by_key(|b| b.price).cloned()
                    };

                if let Some(winning_bid) = winning_bid_opt {
                    let job_manifest_for_assignment = _job_manifest.clone();

                    jobs_to_assign.push((
                        job_id.clone(),
                        job_manifest_for_assignment,
                        original_mesh_job.clone(),
                        winning_bid,
                    ));
                    assigned_this_round.insert(job_id.clone());
                }
            }
        }

        for (job_id, job_manifest, _original_mesh_job, winning_bid) in jobs_to_assign {
            match self.assign_job_to_executor(&job_manifest, winning_bid.clone()).await {
                Ok(_) => {
                    tracing::info!("Successfully assigned job {} to executor {}", job_id, winning_bid.executor_did);
                    self.assigned_by_originator.write().unwrap().insert(job_id.clone());
                }
                Err(e) => {
                    tracing::error!("Failed to assign job {} to executor {}: {:?}", job_id, winning_bid.executor_did, e);
                }
            }
        }
        Ok(())
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<MeshBehaviour>) -> Result<(), anyhow::Error> {
        match event {
            SwarmEvent::Behaviour(behaviour_event) => {
                match behaviour_event {
                    BehaviourEvent::Gossipsub(gossipsub_event) => {
                        match gossipsub_event {
                            GossipsubEvent::Message(message) => {
                                let deserialized_message = serde_cbor::from_slice::<MeshProtocolMessage>(&message.data);
                                match deserialized_message {
                                    Ok(protocol_message) => {
                                        match protocol_message {
                                            MeshProtocolMessage::JobBidV1 { job_id, executor_did, price, comment, region } => {
                                                tracing::info!(
                                                    "[MeshNode] Received JobBidV1 for JobID: {} from Executor DID: {} with Price: {}. Region: {:?}. Comment: {:?}. Topic: {}",
                                                    job_id, executor_did, price, region, comment, message.topic
                                                );

                                                let current_timestamp = Utc::now().timestamp();
                                                let new_bid = crate::protocol::Bid {
                                                    job_id: job_id.clone(), 
                                                    executor_did: executor_did.clone(), 
                                                    price,
                                                    timestamp: current_timestamp,
                                                    comment: comment.clone(),
                                                    region: region.clone(), // Store the received region
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
                                            }
                                            _ => {
                                                // Handle other message types
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to deserialize MeshProtocolMessage: {:?}. Data: {:?}", e, message.data);
                                    }
                                }
                            }
                            _ => {
                                // Handle other gossipsub events
                            }
                        }
                    }
                    _ => {
                        // Handle other behaviour events
                    }
                }
            }
            _ => {
                // Handle other swarm events
            }
        }
        Ok(())
    }
} 