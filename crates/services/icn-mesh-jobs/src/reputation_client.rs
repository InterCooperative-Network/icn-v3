use anyhow::{anyhow, Result};
use icn_identity::Did;
use icn_types::reputation::{ReputationProfile, ReputationRecord}; // Assuming this path is correct based on icn-types structure
use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::time::Duration;
use std::sync::Arc;
use crate::models::BidEvaluatorConfig;

/// Reputation profile with detailed metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationProfile {
    /// The DID of the node
    pub node_id: String,
    
    /// Total jobs executed
    pub total_jobs: u64,
    
    /// Successfully completed jobs
    pub successful_jobs: u64,
    
    /// Failed jobs
    pub failed_jobs: u64,
    
    /// Jobs completed on time
    pub jobs_on_time: u64,
    
    /// Jobs completed late
    pub jobs_late: u64,
    
    /// Average execution time in milliseconds
    pub average_execution_ms: Option<u32>,
    
    /// Average bid accuracy (0-1)
    pub average_bid_accuracy: Option<f32>,
    
    /// Count of dishonesty events
    pub dishonesty_events: u32,
    
    /// List of DIDs that have endorsed this node
    pub endorsements: Vec<String>,
    
    /// Computed reputation score (0-100)
    pub computed_score: f64,
}

/// Constants for configuration
const DEFAULT_REPUTATION_API_TIMEOUT_SECS: u64 = 5;

/// Fetches the reputation profile for a given node DID from the reputation service
/// and returns its computed score.
pub async fn get_reputation_score(node_id: &Did, base_url: &str) -> Result<Option<f64>> {
    // Ensure base_url doesn't have a trailing slash, and construct the full URL.
    let base = base_url.trim_end_matches('/');
    let url = format!("{}/reputation/profiles/{}", base, node_id.0); // Accessing inner String of Did

    tracing::debug!("Querying reputation score for {} at URL: {}", node_id.0, url);

    let client = reqwest::Client::new();
    let resp = client.get(&url).send().await?;

    if resp.status().is_success() {
        // Attempt to deserialize the full ReputationProfile
        match resp.json::<ReputationProfile>().await {
            Ok(profile) => {
                tracing::debug!("Successfully fetched reputation profile for {}: score = {}", node_id.0, profile.computed_score);
                Ok(Some(profile.computed_score))
            }
            Err(e) => {
                tracing::error!("Failed to deserialize ReputationProfile for {}: {}. Response: {:?}", node_id.0, e, resp.text().await.unwrap_or_else(|_| "<failed to read body>".to_string()));
                Err(anyhow!("Failed to deserialize reputation profile: {}", e))
            }
        }
    } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
        tracing::debug!("Reputation profile not found for {}", node_id.0);
        Ok(None) // Node has no reputation profile yet, or service returned 404 correctly
    } else {
        let status = resp.status();
        let error_body = resp.text().await.unwrap_or_else(|_| "<no body>".to_string());
        tracing::error!("Reputation query for {} failed with status {}: {}", node_id.0, status, error_body);
        Err(anyhow!(
            "Reputation service query failed for node {} with status {}: {}",
            node_id.0, status, error_body
        ))
    }
}

/// Submits a reputation record to the reputation service.
pub async fn submit_reputation_record(record: &ReputationRecord, base_url: &str) -> Result<()> {
    let base = base_url.trim_end_matches('/');
    let url = format!("{}/reputation/records", base);

    tracing::debug!("Submitting reputation record for subject {} to URL: {}", record.subject.0, url);

    let client = reqwest::Client::new();
    let resp = client.post(&url).json(record).send().await?;

    if resp.status().is_success() || resp.status() == reqwest::StatusCode::CREATED {
        tracing::info!(
            "Successfully submitted reputation record for subject {}. Status: {}",
            record.subject.0,
            resp.status()
        );
        Ok(())
    } else {
        let status = resp.status();
        let error_body = resp.text().await.unwrap_or_else(|_| "<no body>".to_string());
        tracing::error!(
            "Failed to submit reputation record for subject {}. Status: {}. Body: {}",
            record.subject.0, status, error_body
        );
        Err(anyhow!(
            "Reputation service failed to accept record for subject {} with status {}: {}",
            record.subject.0, status, error_body
        ))
    }
}

pub async fn get_reputation_profile(did: &Did, reputation_url: &str) -> Result<Option<ReputationProfile>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(DEFAULT_REPUTATION_API_TIMEOUT_SECS))
        .build()
        .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))?;
    
    let url = format!("{}/profiles/{}/history/latest", reputation_url.trim_end_matches('/'), did.0);
    
    let response = client.get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to fetch reputation profile: {}", e))?;
        
    if response.status().is_success() {
        let profile = response.json::<ReputationProfile>().await
            .map_err(|e| anyhow!("Failed to parse reputation profile: {}", e))?;
            
        Ok(Some(profile))
    } else if response.status().as_u16() == 404 {
        // Not found is a valid response - no reputation data exists yet
        Ok(None)
    } else {
        Err(anyhow!("Failed to fetch reputation profile: HTTP status {}", response.status()))
    }
}

/// Trait for fetching and caching reputation profiles
#[async_trait::async_trait]
pub trait ReputationClient: Send + Sync {
    /// Fetch a reputation profile for a DID
    async fn fetch_profile(&self, did: &str) -> Result<ReputationProfile>;
    
    /// Calculate a bid score using reputation data
    fn calculate_bid_score(
        &self,
        config: &BidEvaluatorConfig,
        profile: &ReputationProfile,
        normalized_price: f64,
        resource_match: f64,
    ) -> f64;
}

/// Default implementation of the reputation client
pub struct DefaultReputationClient {
    client: Client,
    reputation_url: Arc<String>,
}

impl DefaultReputationClient {
    pub fn new(reputation_url: Arc<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_REPUTATION_API_TIMEOUT_SECS))
            .build()
            .expect("Failed to create HTTP client");
        
        Self { client, reputation_url }
    }
}

#[async_trait::async_trait]
impl ReputationClient for DefaultReputationClient {
    async fn fetch_profile(&self, did: &str) -> Result<ReputationProfile> {
        let url = format!("{}/profiles/{}/history/latest", self.reputation_url.trim_end_matches('/'), did);
        
        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch reputation profile: {}", e))?;
            
        if !response.status().is_success() {
            if response.status().as_u16() == 404 {
                // Create a default profile for new or unknown nodes
                return Ok(ReputationProfile {
                    node_id: did.to_string(),
                    total_jobs: 0,
                    successful_jobs: 0,
                    failed_jobs: 0,
                    jobs_on_time: 0,
                    jobs_late: 0,
                    average_execution_ms: None,
                    average_bid_accuracy: None,
                    dishonesty_events: 0,
                    endorsements: Vec::new(),
                    computed_score: 50.0, // Default neutral score
                });
            }
            
            return Err(anyhow!(
                "Failed to fetch reputation profile. Status: {}", 
                response.status()
            ));
        }
        
        let profile = response.json::<ReputationProfile>().await
            .map_err(|e| anyhow!("Failed to parse reputation profile: {}", e))?;
            
        Ok(profile)
    }
    
    fn calculate_bid_score(
        &self,
        config: &BidEvaluatorConfig,
        profile: &ReputationProfile,
        normalized_price: f64,
        resource_match: f64,
    ) -> f64 {
        // Extract parameters from the profile
        let reputation_score = profile.computed_score / 100.0;
        
        // Calculate timeliness score - avoid division by zero
        let timeliness_score = if profile.successful_jobs > 0 {
            profile.jobs_on_time as f64 / profile.successful_jobs as f64
        } else {
            0.5 // Default value if no successful jobs
        };
        
        // Calculate the weighted score
        let price_component = config.weight_price * (1.0 - normalized_price);
        let resource_component = config.weight_resources * resource_match;
        let reputation_component = config.weight_reputation * reputation_score;
        let timeliness_component = config.weight_timeliness * timeliness_score;
        
        // Sum all components for total score
        price_component + resource_component + reputation_component + timeliness_component
    }
} 