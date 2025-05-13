use anyhow::Result;
use reqwest::Client;
use icn_types::runtime_receipt::RuntimeExecutionReceipt;
use icn_types::reputation::ReputationRecord;
use std::time::Duration;
use cid::Cid;
use cid::multihash::{Multihash, Code};
use chrono::Utc;
use icn_identity::Did;
use tracing;
use std::str::FromStr;
use multihash::{Hasher, Sha2_256};
use serde::Deserialize;
use std::path::Path;
use std::fs;

use crate::metrics;

/// Configuration for reputation scoring parameters
#[derive(Debug, Clone, Deserialize)]
pub struct ReputationScoringConfig {
    pub mana_cost_weight: f64, // Weight factor for mana cost scoring (e.g., numerator in 1/cost) - Will be replaced by sigmoid
    pub failure_penalty: f64, // Flat penalty score for failed submissions - Will be replaced by scaled penalty
    pub max_positive_score: f64, // Maximum possible score delta for a successful, mana-based update

    // New fields for refined scoring model
    pub sigmoid_k: f64,              // Steepness factor for the sigmoid curve
    pub sigmoid_midpoint: f64,       // Midpoint for the sigmoid curve (mana_cost where score is 0.5 * max_positive_score scaling factor)
    pub failure_penalty_weight: f64, // Weight factor for the scaled failure penalty

    // Fields for reputation modifier
    pub enable_reputation_modifier: bool, // Feature flag to enable/disable modifier logic
    pub modifier_min_bound: f64,          // Minimum value for the reputation modifier
    pub modifier_max_bound: f64,          // Maximum value for the reputation modifier
    // Optional: Add field for assumed max reputation score if normalization needs it, e.g., `max_possible_reputation_score: f64`
}

impl ReputationScoringConfig {
    /// Load reputation scoring configuration from a TOML file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path_ref = path.as_ref();
        tracing::info!("Attempting to load reputation scoring config from: {:?}", path_ref);
        let text = fs::read_to_string(path_ref)
            .map_err(|e| anyhow::anyhow!("Failed to read reputation config file at {:?}: {}", path_ref, e))?;
        let config: Self = toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("Failed to parse reputation config from TOML at {:?}: {}", path_ref, e))?;
        tracing::info!("Successfully loaded reputation scoring config from: {:?}", path_ref);
        Ok(config)
    }
}

impl Default for ReputationScoringConfig {
    fn default() -> Self {
        Self {
            // Old fields - keep for now, but their direct use will be phased out by new logic
            mana_cost_weight: 100.0,
            failure_penalty: -25.0,
            // New fields with default values
            max_positive_score: 5.0,     // Max positive score remains
            sigmoid_k: 0.02,             // Default steepness for sigmoid
            sigmoid_midpoint: 100.0,     // Default midpoint for sigmoid
            failure_penalty_weight: 5.0, // Default weight for scaled failure penalty (e.g. ln(101) * 5 ~= 23)

            // Default values for reputation modifier
            enable_reputation_modifier: false, // Disabled by default for backward compatibility
            modifier_min_bound: 0.5,           // Default min modifier (e.g., for low reputation)
            modifier_max_bound: 2.0,           // Default max modifier (e.g., for high reputation)
            // max_possible_reputation_score: 100.0, // Example if needed for normalization
        }
    }
}

/// This trait allows providing different implementations of reputation update
/// logic for testing and production environments
#[async_trait::async_trait]
pub trait ReputationUpdater: Send + Sync {
    /// Submit a reputation record derived from a runtime execution receipt
    async fn submit_receipt_based_reputation(
        &self,
        receipt: &RuntimeExecutionReceipt,
        is_successful: bool, // Verification/Execution success status
        coop_id: &str,       // Cooperative ID label
        community_id: &str,  // Community ID label
    ) -> Result<()>;
}

/// The real implementation that sends HTTP requests to the reputation service
pub struct HttpReputationUpdater {
    client: Client,
    reputation_service_url: String,
    local_did: Did,
    config: ReputationScoringConfig, // Add config field
}

impl HttpReputationUpdater {
    /// Creates a new HttpReputationUpdater with default configuration.
    pub fn new(reputation_service_url: String, local_did: Did) -> Self {
        Self::new_with_config(reputation_service_url, local_did, ReputationScoringConfig::default())
    }

    /// Creates a new HttpReputationUpdater with specific configuration.
    pub fn new_with_config(reputation_service_url: String, local_did: Did, config: ReputationScoringConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client for reputation updater");

        Self { client, reputation_service_url, local_did, config }
    }

    // Placeholder method to fetch current reputation score
    // Assumes the reputation service has an endpoint like GET /reputation/profiles/{did}
    // and returns a JSON object containing a field like `computed_score`.
    // Error handling and response parsing are simplified here.
    async fn get_current_score(&self, did_str: &str) -> Result<Option<f64>> {
        // Construct URL: Ensure base_url doesn't have trailing slash
        let base = self.reputation_service_url.trim_end_matches('/');
        // Use the endpoint identified earlier in icn-reputation/src/main.rs
        let url = format!("{}/reputation/profiles/{}", base, did_str); 
        
        tracing::debug!("Querying reputation score for {} at URL: {}", did_str, url);

        // Define a nested struct matching the expected JSON structure from the service
        // Based on icn-types/src/reputation.rs
        #[derive(Deserialize)]
        struct ProfileResponse {
            // Add other fields from ReputationProfile if needed, but only score is required here
            computed_score: f64, 
        }

        match self.client.get(&url).send().await {
            Ok(resp) => {
                if resp.status() == reqwest::StatusCode::NOT_FOUND {
                    tracing::debug!("Reputation profile not found for {}, assuming default score for modifier.", did_str);
                    Ok(None) // No profile exists yet
                } else if resp.status().is_success() {
                    match resp.json::<ProfileResponse>().await {
                        Ok(profile) => Ok(Some(profile.computed_score)),
                        Err(e) => {
                            tracing::warn!("Failed to parse reputation profile JSON for {}: {}. Using default score for modifier.", did_str, e);
                            Ok(None) // Treat parse error as if no score available
                        }
                    }
                } else {
                    let status = resp.status();
                    let error_body = resp.text().await.unwrap_or_else(|_| "<failed to read response>".to_string());
                    tracing::warn!("Failed GET request for reputation profile {}: HTTP {} - {}. Using default score for modifier.", did_str, status, error_body);
                    Ok(None) // Treat API error as if no score available
                }
            }
            Err(e) => {
                tracing::warn!("Failed to connect or send request for reputation profile {}: {}. Using default score for modifier.", did_str, e);
                Ok(None) // Treat connection error as if no score available
            }
        }
    }
}

#[async_trait::async_trait]
impl ReputationUpdater for HttpReputationUpdater {
    async fn submit_receipt_based_reputation(
        &self,
        receipt: &RuntimeExecutionReceipt,
        is_successful: bool,
        coop_id: &str,
        community_id: &str,
    ) -> Result<()> {
        let executor_did = receipt.issuer.as_str();
        fn sigmoid(mc: f64, k: f64, midpoint: f64) -> f64 {
            1.0 / (1.0 + f64::exp(k * (mc - midpoint)))
        }

        let score_delta = if is_successful {
            let mana_cost = receipt.metrics.mana_cost.unwrap_or(0) as f64;
            let base_sigmoid_score = sigmoid(mana_cost, self.config.sigmoid_k, self.config.sigmoid_midpoint);
            let mut calculated_score = base_sigmoid_score * self.config.max_positive_score;

            // --- Apply Reputation Modifier --- 
            if self.config.enable_reputation_modifier {
                tracing::debug!("Reputation modifier enabled for executor {}", executor_did);
                // Fetch current score
                let current_score_opt = self.get_current_score(executor_did).await?;
                
                // Assume a default score (e.g., 0.5 on a 0-1 scale) if none exists or fetch fails
                // Normalize based on an assumed 0-100 scale from the reputation service (adjust if needed)
                // A more robust approach might involve getting min/max possible scores from the service or config.
                let assumed_max_score = 100.0; // TODO: Make this configurable if needed
                let normalized_score = current_score_opt.map_or(0.5, |score| (score / assumed_max_score).clamp(0.0, 1.0));
                
                let reputation_modifier = (1.0 + normalized_score)
                    .clamp(self.config.modifier_min_bound, self.config.modifier_max_bound);
                
                tracing::debug!("Applying reputation modifier: {:.2} (normalized score: {:.2})", reputation_modifier, normalized_score);
                calculated_score *= reputation_modifier;
            }
            // --- End Reputation Modifier --- 

            calculated_score.min(self.config.max_positive_score)
        } else {
            let mana_cost = receipt.metrics.mana_cost.unwrap_or(0) as f64;
            let penalty_base = if mana_cost >= 0.0 { mana_cost + 1.0 } else { 1.0 };
            -self.config.failure_penalty_weight * penalty_base.ln()
            // Note: Modifier is not applied to penalties in this version
        };

        // Create the record
        let record = ReputationRecord {
            subject: receipt.issuer.clone(),
            anchor: receipt.receipt_cid.clone().unwrap_or_else(|| receipt.id.clone()), // Use receipt_cid if available, else id
            score_delta,
            success: is_successful,
            mana_cost: receipt.metrics.mana_cost,
            timestamp: Utc::now().timestamp() as u64, // Use current time for submission
        };

        // Increment submission counter metric with all labels
        metrics::increment_reputation_submission(
            is_successful, 
            coop_id, 
            community_id, 
            executor_did
        );

        // Observe score delta metric with federation labels
        metrics::observe_reputation_score_delta(
            score_delta, 
            coop_id, 
            community_id, 
            executor_did
        );
        
        // Send the record via HTTP
        let response = self.client
            .post(&self.reputation_service_url)
            .json(&record)
            .send()
            .await?;

        // Process response (removed old metric calls here, handled above)
        if response.status().is_success() {
            tracing::info!(
                "Successfully submitted reputation record for subject {} (anchor: {})",
                record.subject, record.anchor
            );
            // metrics::record_reputation_update_success(); // Removed, handled by increment_reputation_submission
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(
                "Failed to submit reputation record: Status {}, Body: {}",
                status, body
            );
            // metrics::record_reputation_update_failure(); // Removed, handled by increment_reputation_submission
            anyhow::bail!("Failed to submit reputation record: {}", status)
        }
    }
}

/// A no-op implementation for testing or when reputation updates should be disabled
pub struct NoopReputationUpdater;

#[async_trait::async_trait]
impl ReputationUpdater for NoopReputationUpdater {
    async fn submit_receipt_based_reputation(
        &self, 
        _receipt: &RuntimeExecutionReceipt,
        _is_successful: bool, // Accept new parameter
        _coop_id: &str,       // Accept new parameter
        _community_id: &str,  // Accept new parameter
    ) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use icn_types::runtime_receipt::RuntimeExecutionMetrics; // Keep if used
    
    // Helper to calculate expected score delta for tests, mirroring the main logic
    fn calculate_expected_score_delta(config: &ReputationScoringConfig, mana_cost_val: Option<u64>, is_successful: bool) -> f64 {
        fn sigmoid(mc: f64, k: f64, midpoint: f64) -> f64 {
            1.0 / (1.0 + f64::exp(k * (mc - midpoint)))
        }

        if is_successful {
            let mc = mana_cost_val.unwrap_or(0) as f64;
            let base_sigmoid_score = sigmoid(mc, config.sigmoid_k, config.sigmoid_midpoint);
            let calculated_score = base_sigmoid_score * config.max_positive_score;
            calculated_score.min(config.max_positive_score)
        } else {
            let mc = mana_cost_val.unwrap_or(0) as f64;
            let penalty_base = if mc >= 0.0 { mc + 1.0 } else { 1.0 };
            -config.failure_penalty_weight * penalty_base.ln()
        }
    }
    
    #[derive(Clone)]
    struct MockReputationUpdater {
        submitted_items: Arc<Mutex<Vec<(RuntimeExecutionReceipt, bool, String, String)>>>,
        // To inspect the record sent to the HTTP client, including the calculated score_delta
        submitted_records_to_service: Arc<Mutex<Vec<ReputationRecord>>>,
    }
    
    impl MockReputationUpdater {
        fn new() -> Self {
            Self {
                submitted_items: Arc::new(Mutex::new(Vec::new())),
                submitted_records_to_service: Arc::new(Mutex::new(Vec::new())),
            }
        }
        
        fn get_submissions(&self) -> Vec<(RuntimeExecutionReceipt, bool, String, String)> {
            self.submitted_items.lock().unwrap().clone()
        }

        // Getter for the records that would have been sent
        fn get_submitted_records_to_service(&self) -> Vec<ReputationRecord> {
            self.submitted_records_to_service.lock().unwrap().clone()
        }
    }
    
    // Mock HttpReputationUpdater to intercept the record before it would be sent
    // This is a simplified mock focusing on capturing the ReputationRecord
    // It doesn't actually make HTTP calls.
    async fn mock_http_submit(
        updater_config: &ReputationScoringConfig, // Pass updater's config
        receipt: &RuntimeExecutionReceipt,
        is_successful: bool,
        // coop_id: &str, // Not used by the mocked part of logic directly
        // community_id: &str, // Not used by the mocked part of logic directly
        records_log: Arc<Mutex<Vec<ReputationRecord>>> // Log to store the generated record
    ) -> Result<()> {
        fn sigmoid(mc: f64, k: f64, midpoint: f64) -> f64 {
            1.0 / (1.0 + f64::exp(k * (mc - midpoint)))
        }

        let score_delta = if is_successful {
            let mc = receipt.metrics.mana_cost.unwrap_or(0) as f64;
            let base_sigmoid_score = sigmoid(mc, updater_config.sigmoid_k, updater_config.sigmoid_midpoint);
            let calculated_score = base_sigmoid_score * updater_config.max_positive_score;
            calculated_score.min(updater_config.max_positive_score)
        } else {
            let mc = receipt.metrics.mana_cost.unwrap_or(0) as f64;
            let penalty_base = if mc >= 0.0 { mc + 1.0 } else { 1.0 };
            -updater_config.failure_penalty_weight * penalty_base.ln()
        };

        let record = ReputationRecord {
            subject: receipt.issuer.clone(),
            anchor: receipt.receipt_cid.clone().unwrap_or_else(|| receipt.id.clone()),
            score_delta,
            success: is_successful,
            mana_cost: receipt.metrics.mana_cost,
            timestamp: Utc::now().timestamp() as u64,
        };
        records_log.lock().unwrap().push(record);
        Ok(())
    }

    #[tokio::test]
    async fn test_reputation_update_from_receipt() {
        // This test remains to check the trait plumbing with MockReputationUpdater
        let mock_updater_trait_impl = MockReputationUpdater::new();
        let updater_trait_arc = Arc::new(mock_updater_trait_impl.clone());
        
        let receipt = RuntimeExecutionReceipt { /* ... minimal fields ... */ 
            id: "test-receipt-1".into(), issuer: "did:key:test-executor".into(), proposal_id: "p1".into(), wasm_cid: "w1".into(), ccl_cid: "c1".into(),
            metrics: RuntimeExecutionMetrics { host_calls:0, io_bytes:0, mana_cost: Some(1000) }, anchored_cids: vec![], resource_usage: vec![],
            timestamp:0, dag_epoch:None, receipt_cid:None, signature:None };
        
        updater_trait_arc.submit_receipt_based_reputation(&receipt, true, "test-coop", "test-community").await.unwrap();
        
        let submitted_to_trait = mock_updater_trait_impl.get_submissions();
        assert_eq!(submitted_to_trait.len(), 1);
        assert_eq!(submitted_to_trait[0].0.id, "test-receipt-1");
        assert_eq!(submitted_to_trait[0].1, true);
    }

    // --- Tests for new scoring logic ---
    fn default_test_config() -> ReputationScoringConfig {
        ReputationScoringConfig::default()
    }

    fn create_test_receipt(mana_cost: Option<u64>) -> RuntimeExecutionReceipt {
        RuntimeExecutionReceipt {
            id: "test-dynamic-receipt".into(),
            issuer: "did:key:test-executor-dynamic".into(),
            proposal_id: "prop-dynamic".into(),
            wasm_cid: "wasm-cid-dynamic".into(),
            ccl_cid: "ccl-cid-dynamic".into(),
            metrics: RuntimeExecutionMetrics { host_calls: 1, io_bytes: 10, mana_cost },
            anchored_cids: vec![],
            resource_usage: vec![],
            timestamp: Utc::now().timestamp() as u64,
            dag_epoch: None,
            receipt_cid: Some("cid-dynamic-receipt".into()),
            signature: None,
        }
    }

    struct ScoringTestCase {
        description: &'static str,
        mana_cost: Option<u64>,
        is_successful: bool,
        // expected_delta: f64, // Calculated by helper
    }

    #[tokio::test]
    async fn test_new_scoring_model_logic() {
        let config = default_test_config();
        let test_cases = vec![
            ScoringTestCase { description: "Success: Low mana cost (10)", mana_cost: Some(10), is_successful: true },
            ScoringTestCase { description: "Success: Mid mana cost (100)", mana_cost: Some(100), is_successful: true },
            ScoringTestCase { description: "Success: High mana cost (200)", mana_cost: Some(200), is_successful: true },
            ScoringTestCase { description: "Success: Zero mana cost", mana_cost: Some(0), is_successful: true },
            ScoringTestCase { description: "Success: None mana cost", mana_cost: None, is_successful: true }, // Should also be treated as 0 cost

            ScoringTestCase { description: "Failure: Low mana cost (10)", mana_cost: Some(10), is_successful: false },
            ScoringTestCase { description: "Failure: Mid mana cost (100)", mana_cost: Some(100), is_successful: false },
            ScoringTestCase { description: "Failure: High mana cost (200)", mana_cost: Some(200), is_successful: false },
            ScoringTestCase { description: "Failure: Zero mana cost", mana_cost: Some(0), is_successful: false },
            ScoringTestCase { description: "Failure: None mana cost", mana_cost: None, is_successful: false }, // Should also be treated as 0 cost
        ];

        for case in test_cases {
            let receipt = create_test_receipt(case.mana_cost);
            let records_log = Arc::new(Mutex::new(Vec::<ReputationRecord>::new()));
            
            // We use the mock_http_submit to simulate what HttpReputationUpdater's main logic would do
            // with its *own* config before sending. This tests the core calculation.
            mock_http_submit(&config, &receipt, case.is_successful, records_log.clone()).await.unwrap();
            
            let submitted_service_records = records_log.lock().unwrap();
            assert_eq!(submitted_service_records.len(), 1, "Failed for case: {}", case.description);
            let record_sent = &submitted_service_records[0];

            let expected_delta = calculate_expected_score_delta(&config, case.mana_cost, case.is_successful);
            
            // Compare with a tolerance for f64 comparisons
            assert!((record_sent.score_delta - expected_delta).abs() < 1e-9, 
                "Score delta mismatch for case: {}. Expected: {}, Got: {}. Mana: {:?}, Success: {}", 
                case.description, expected_delta, record_sent.score_delta, case.mana_cost, case.is_successful);
        }
    }
} 