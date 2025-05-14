use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// Import types needed for reputation integration
use icn_types::reputation::{compute_score, ReputationProfile};

// Constants for configuration
const DEFAULT_REPUTATION_API_TIMEOUT_SECS: u64 = 5;
const DEFAULT_REPUTATION_SCORE_TOLERANCE: f64 = 0.05; // 5% tolerance for score verification

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidEvaluatorConfig {
    pub weight_price: f64,
    pub weight_resources: f64,
    pub weight_reputation: f64,
    pub weight_timeliness: f64,
    pub reputation_api_endpoint: String,
    pub reputation_api_timeout_secs: u64,
    pub score_verification_tolerance: f64,
}

impl Default for BidEvaluatorConfig {
    fn default() -> Self {
        Self {
            weight_price: 0.4,
            weight_resources: 0.2,
            weight_reputation: 0.3,
            weight_timeliness: 0.1,
            reputation_api_endpoint: "http://localhost:8080/reputation/profiles".to_string(),
            reputation_api_timeout_secs: DEFAULT_REPUTATION_API_TIMEOUT_SECS,
            score_verification_tolerance: DEFAULT_REPUTATION_SCORE_TOLERANCE,
        }
    }
}

#[async_trait]
pub trait ReputationClient {
    async fn fetch_profile(&self, did: &str) -> Result<ReputationProfile>;
    fn verify_reported_score(&self, profile: &ReputationProfile, reported: u32) -> bool;
    fn calculate_bid_score(
        &self,
        config: &BidEvaluatorConfig,
        profile: &ReputationProfile,
        normalized_price: f64,
        resource_match: f64,
    ) -> f64;
}

pub struct DefaultReputationClient {
    client: Client,
    config: BidEvaluatorConfig,
}

impl DefaultReputationClient {
    pub fn new(config: BidEvaluatorConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.reputation_api_timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    pub fn with_default_config() -> Self {
        Self::new(BidEvaluatorConfig::default())
    }
}

#[async_trait]
impl ReputationClient for DefaultReputationClient {
    async fn fetch_profile(&self, did: &str) -> Result<ReputationProfile> {
        let url = format!(
            "{}/{}/history/latest",
            self.config.reputation_api_endpoint, did
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch reputation profile: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to fetch reputation profile. Status: {}",
                response.status()
            ));
        }

        let profile = response
            .json::<ReputationProfile>()
            .await
            .map_err(|e| anyhow!("Failed to parse reputation profile: {}", e))?;

        Ok(profile)
    }

    fn verify_reported_score(&self, profile: &ReputationProfile, reported: u32) -> bool {
        let computed = profile.computed_score;
        let reported_f64 = reported as f64;

        // Verify the reported score is within tolerance
        let difference = (computed - reported_f64).abs();
        let tolerance = self.config.score_verification_tolerance * computed;

        difference <= tolerance
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

// Helper function to load bid evaluator config from CCL policy
pub async fn load_bid_evaluator_config_from_policy(policy_id: &str) -> Result<BidEvaluatorConfig> {
    // This would fetch the CCL policy and parse it into our config
    // For now, return the default config
    // TODO: Implement actual policy fetching and parsing
    Ok(BidEvaluatorConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_verify_reported_score() {
        let config = BidEvaluatorConfig {
            score_verification_tolerance: 0.05, // 5% tolerance
            ..BidEvaluatorConfig::default()
        };

        let client = DefaultReputationClient::new(config);

        let mut profile = ReputationProfile {
            node_id: "did:key:test".to_string(),
            last_updated: Utc::now(),
            total_jobs: 100,
            successful_jobs: 90,
            failed_jobs: 10,
            jobs_on_time: 85,
            jobs_late: 5,
            average_execution_ms: Some(500),
            average_bid_accuracy: Some(0.95),
            dishonesty_events: 0,
            endorsements: vec![],
            current_stake: None,
            computed_score: 80.0,
            latest_anchor_cid: None,
        };

        // Within tolerance
        assert!(
            client.verify_reported_score(&profile, 79),
            "Should accept score within tolerance (lower)"
        );
        assert!(
            client.verify_reported_score(&profile, 81),
            "Should accept score within tolerance (higher)"
        );
        assert!(
            client.verify_reported_score(&profile, 80),
            "Should accept exact score"
        );

        // Outside tolerance
        assert!(
            !client.verify_reported_score(&profile, 75),
            "Should reject score outside tolerance (lower)"
        );
        assert!(
            !client.verify_reported_score(&profile, 85),
            "Should reject score outside tolerance (higher)"
        );

        // Edge case: zero score
        profile.computed_score = 0.0;
        assert!(
            client.verify_reported_score(&profile, 0),
            "Should accept zero score exactly"
        );
        assert!(
            !client.verify_reported_score(&profile, 1),
            "Should reject non-zero when computed is zero"
        );
    }

    #[test]
    fn test_calculate_bid_score() {
        let config = BidEvaluatorConfig {
            weight_price: 0.4,
            weight_resources: 0.2,
            weight_reputation: 0.3,
            weight_timeliness: 0.1,
            ..BidEvaluatorConfig::default()
        };

        let client = DefaultReputationClient::new(config.clone());

        let profile = ReputationProfile {
            node_id: "did:key:test".to_string(),
            last_updated: Utc::now(),
            total_jobs: 100,
            successful_jobs: 80,
            failed_jobs: 20,
            jobs_on_time: 75,
            jobs_late: 5,
            average_execution_ms: Some(500),
            average_bid_accuracy: Some(0.95),
            dishonesty_events: 0,
            endorsements: vec![],
            current_stake: None,
            computed_score: 80.0,
            latest_anchor_cid: None,
        };

        // Test case: moderate values
        let score = client.calculate_bid_score(&config, &profile, 0.5, 0.8);

        // Expected:
        // price: 0.4 * (1 - 0.5) = 0.2
        // resources: 0.2 * 0.8 = 0.16
        // reputation: 0.3 * (80/100) = 0.24
        // timeliness: 0.1 * (75/80) = 0.09375
        // Total: 0.2 + 0.16 + 0.24 + 0.09375 = 0.69375

        assert!(
            (score - 0.69375).abs() < 0.0001,
            "Score calculation should match expected value"
        );

        // Test extreme values
        let high_reputation_profile = ReputationProfile {
            computed_score: 95.0,
            jobs_on_time: 95,
            successful_jobs: 100,
            ..profile.clone()
        };

        let low_reputation_profile = ReputationProfile {
            computed_score: 30.0,
            jobs_on_time: 20,
            successful_jobs: 50,
            ..profile.clone()
        };

        let high_rep_score =
            client.calculate_bid_score(&config, &high_reputation_profile, 0.7, 0.9);
        let low_rep_score = client.calculate_bid_score(&config, &low_reputation_profile, 0.3, 0.6);

        // Verify high reputation can overcome price disadvantage
        assert!(
            high_rep_score > low_rep_score,
            "High reputation should score better despite price disadvantage"
        );
    }
}
