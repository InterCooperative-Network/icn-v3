use anyhow::Result;
use async_trait::async_trait;
use icn_identity::Did;
use icn_types::reputation::{
    compute_score,
    ReputationProfile, 
    ReputationRecord,
    ReputationUpdateEvent // Used indirectly via ReputationRecord but good to have for context
};
use std::collections::HashMap;
use tokio::sync::RwLock;
use chrono::Utc; // For setting initial last_updated timestamp

#[async_trait]
pub trait ReputationStore: Send + Sync + 'static {
    /// Retrieves the latest reputation profile for a given node DID.
    async fn get_profile(&self, node_id: &Did) -> Result<Option<ReputationProfile>>;

    /// Submits a new reputation record. 
    /// This will update the corresponding node's profile based on the event in the record.
    async fn submit_record(&self, record: ReputationRecord) -> Result<()>;

    /// Lists all reputation records submitted for a given node DID.
    async fn list_records(&self, node_id: &Did) -> Result<Vec<ReputationRecord>>;
}

pub struct InMemoryReputationStore {
    // Stores the current, aggregated reputation profile for each node.
    profiles: RwLock<HashMap<Did, ReputationProfile>>,
    // Stores a log of all reputation records for each node.
    records: RwLock<HashMap<Did, Vec<ReputationRecord>>>,
}

impl InMemoryReputationStore {
    pub fn new() -> Self {
        Self {
            profiles: RwLock::new(HashMap::new()),
            records: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl ReputationStore for InMemoryReputationStore {
    async fn get_profile(&self, node_id: &Did) -> Result<Option<ReputationProfile>> {
        let profiles_guard = self.profiles.read().await;
        Ok(profiles_guard.get(node_id).cloned())
    }

    async fn submit_record(&self, record: ReputationRecord) -> Result<()> {
        // 1. Store the raw record
        let mut records_guard = self.records.write().await;
        records_guard
            .entry(record.subject.clone()) // Group records by the subject node
            .or_default()
            .push(record.clone()); // Clone record as it's used for profile update too
        drop(records_guard); // Release lock

        // 2. Fetch or create a mutable ReputationProfile
        let mut profiles_guard = self.profiles.write().await;
        let profile = profiles_guard
            .entry(record.subject.clone())
            .or_insert_with(|| {
                // Create a new default profile if one doesn't exist for the subject DID
                // The compute_score on a default profile will give its base score.
                let mut new_profile = ReputationProfile {
                    node_id: record.subject.clone(),
                    last_updated: record.timestamp, // Initialize with the record's timestamp
                    total_jobs: 0,
                    successful_jobs: 0,
                    failed_jobs: 0,
                    jobs_on_time: 0,
                    jobs_late: 0,
                    average_execution_ms: None,
                    average_bid_accuracy: None,
                    dishonesty_events: 0,
                    endorsements: Vec::new(),
                    current_stake: None,
                    computed_score: 0.0, // Will be computed properly after creation
                    latest_anchor_cid: None,
                };
                new_profile.computed_score = compute_score(&new_profile); // Set initial score
                new_profile
            });

        // 3. Apply the event from the record to the profile
        //    apply_event updates raw metrics and profile.last_updated to Utc::now()
        profile.apply_event(&record.event);

        // 4. Recompute the overall score based on updated metrics
        profile.computed_score = compute_score(profile);
        
        // 5. Update latest_anchor_cid from the record if present
        if record.anchor.is_some() {
            profile.latest_anchor_cid = record.anchor;
        }
        
        // Profile is updated in place within the RwLock guard.
        Ok(())
    }

    async fn list_records(&self, node_id: &Did) -> Result<Vec<ReputationRecord>> {
        let records_guard = self.records.read().await;
        Ok(records_guard.get(node_id).cloned().unwrap_or_default())
    }
} 