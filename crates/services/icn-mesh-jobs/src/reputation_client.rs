use anyhow::{anyhow, Result};
use icn_identity::Did;
use icn_types::reputation::{ReputationProfile, ReputationRecord}; // Assuming this path is correct based on icn-types structure
use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::time::Duration;
use std::sync::Arc;
use crate::models::BidEvaluatorConfig;
use tracing;
use tokio::sync::RwLock;
use std::collections::HashMap;

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
    async fn fetch_profile(&self, did: &Did) -> Result<Option<ReputationProfile>>;
    
    /// Calculate a bid score using reputation data
    fn calculate_bid_score(
        &self,
        config: &BidEvaluatorConfig,
        profile: &ReputationProfile,
        normalized_price: f64,
        resource_match: f64,
    ) -> f64;

    /// Submit a reputation record
    async fn submit_record(&self, record: ReputationRecord) -> Result<()>;
}

/// Default implementation of the reputation client
pub struct DefaultReputationClient {
    client: Client,
    base_url: String,
}

impl DefaultReputationClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_REPUTATION_API_TIMEOUT_SECS))
            .build()
            .expect("Failed to create HTTP client");
        
        Self { client, base_url }
    }
}

#[async_trait::async_trait]
impl ReputationClient for DefaultReputationClient {
    async fn fetch_profile(&self, did: &Did) -> Result<Option<ReputationProfile>> {
        let base = self.base_url.trim_end_matches('/');
        let url = format!("{}/reputation/profiles/{}", base, did.to_string());

        tracing::debug!("Querying reputation score for {} at URL: {}", did.to_string(), url);

        let resp = self.client.get(&url).send().await?;

        if resp.status().is_success() {
            let profile: ReputationProfile = resp.json().await?;
            tracing::debug!(
                "Successfully retrieved reputation profile for {}: score = {}",
                did.to_string(),
                profile.computed_score
            );
            Ok(Some(profile))
        } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
            tracing::debug!(
                "Reputation profile not found for {}: {}. Response: {:?}",
                did.to_string(),
                resp.status(),
                resp.text().await.unwrap_or_else(|_| "<failed to read response>".to_string())
            );
            Ok(None)
        } else {
            let status = resp.status();
            let error_body = resp.text().await.unwrap_or_else(|_| "<failed to read response>".to_string());
            tracing::error!(
                "Failed to fetch reputation profile for {} failed with status {}: {}",
                did.to_string(),
                status,
                error_body
            );
            Err(anyhow::anyhow!(
                "Failed to fetch reputation profile for {}: HTTP {} - {}",
                did.to_string(),
                status,
                error_body
            ))
        }
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

    async fn submit_record(&self, record: ReputationRecord) -> Result<()> {
        let base = self.base_url.trim_end_matches('/');
        let url = format!("{}/reputation/records", base);

        tracing::debug!(
            "Submitting reputation record for subject {} to URL: {}",
            record.subject.to_string(),
            url
        );

        let resp = self.client.post(&url).json(&record).send().await?;

        if resp.status().is_success() {
            tracing::debug!(
                "Successfully submitted reputation record for subject {}",
                record.subject.to_string()
            );
            Ok(())
        } else {
            let status = resp.status();
            let error_body = resp.text().await.unwrap_or_else(|_| "<failed to read response>".to_string());
            tracing::error!(
                "Failed to submit reputation record for subject {}: HTTP {} - {}",
                record.subject.to_string(),
                status,
                error_body
            );
            Err(anyhow::anyhow!(
                "Failed to submit reputation record for subject {}: HTTP {} - {}",
                record.subject.to_string(),
                status,
                error_body
            ))
        }
    }
}

pub struct CachingReputationClient {
    client: Arc<dyn ReputationClient>,
    cache: Arc<RwLock<HashMap<String, (ReputationProfile, std::time::Instant)>>>,
    cache_ttl: Duration,
}

impl CachingReputationClient {
    pub fn new(client: Arc<dyn ReputationClient>, cache_ttl: Duration) -> Self {
        Self {
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl,
        }
    }

    async fn get_cached_profile(&self, did: &Did) -> Option<ReputationProfile> {
        let cache = self.cache.read().await;
        if let Some((profile, timestamp)) = cache.get(&did.to_string()) {
            if timestamp.elapsed() < self.cache_ttl {
                return Some(profile.clone());
            }
        }
        None
    }

    async fn cache_profile(&self, did: &Did, profile: ReputationProfile) {
        let mut cache = self.cache.write().await;
        cache.insert(did.to_string(), (profile, std::time::Instant::now()));
    }
}

#[async_trait::async_trait]
impl ReputationClient for CachingReputationClient {
    async fn fetch_profile(&self, did: &Did) -> Result<Option<ReputationProfile>> {
        // Try to get from cache first
        if let Some(cached) = self.get_cached_profile(did).await {
            return Ok(Some(cached));
        }

        // If not in cache, fetch from client
        if let Some(profile) = self.client.fetch_profile(did).await? {
            self.cache_profile(did, profile.clone()).await;
            Ok(Some(profile))
        } else {
            Ok(None)
        }
    }

    fn calculate_bid_score(
        &self,
        config: &BidEvaluatorConfig,
        profile: &ReputationProfile,
        normalized_price: f64,
        resource_match: f64,
    ) -> f64 {
        self.client.calculate_bid_score(config, profile, normalized_price, resource_match)
    }

    async fn submit_record(&self, record: ReputationRecord) -> Result<()> {
        self.client.submit_record(record).await
    }
} 