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
use httpmock::MockServer;
use icn_identity::KeyPair;
use serde_json::json;

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
                    // Considered a "soft" failure or expected case, not an error metric necessarily unless desired.
                    // For now, not incrementing REPUTATION_SCORE_FETCH_FAILURES for 404 as it might be common for new DIDs.
                    Ok(None) // No profile exists yet
                } else if resp.status().is_success() {
                    match resp.json::<ProfileResponse>().await {
                        Ok(profile) => Ok(Some(profile.computed_score)),
                        Err(e) => {
                            tracing::warn!("Failed to parse reputation profile JSON for {}: {}. Using default score for modifier.", did_str, e);
                            metrics::REPUTATION_SCORE_FETCH_FAILURES
                                .with_label_values(&[did_str, "json_parse_error"])
                                .inc();
                            Ok(None) // Treat parse error as if no score available
                        }
                    }
                } else {
                    let status_code = resp.status();
                    let status_str = status_code.as_str().to_string(); // reqwest::StatusCode -> &str -> String
                    let error_body = resp.text().await.unwrap_or_else(|_| "<failed to read response>".to_string());
                    tracing::warn!("Failed GET request for reputation profile {}: HTTP {} - {}. Using default score for modifier.", did_str, status_code, error_body);
                    metrics::REPUTATION_SCORE_FETCH_FAILURES
                        .with_label_values(&[did_str, &status_str])
                        .inc();
                    Ok(None) // Treat API error as if no score available
                }
            }
            Err(e) => {
                tracing::warn!("Failed to connect or send request for reputation profile {}: {}. Using default score for modifier.", did_str, e);
                // Using a generic reason for client errors during score fetching
                metrics::REPUTATION_SCORE_FETCH_FAILURES
                    .with_label_values(&[did_str, "client_request_error"])
                    .inc();
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
            .await
            .map_err(|err| {
                // Increment counter for client-side (e.g., reqwest) errors
                metrics::REPUTATION_SUBMISSION_CLIENT_ERRORS
                    .with_label_values(&[record.subject.as_str(), &err.to_string()]) // Using record.subject as executor_did
                    .inc();
                // Wrap the original error to return it
                anyhow::anyhow!("HTTP client error during reputation submission: {}", err)
            })?;

        // Process response (removed old metric calls here, handled above)
        if response.status().is_success() {
            tracing::info!(
                "Successfully submitted reputation record for subject {} (anchor: {})",
                record.subject, record.anchor
            );
            // metrics::record_reputation_update_success(); // Removed, handled by increment_reputation_submission
            Ok(())
        } else {
            let status_code = response.status();
            let status_str = status_code.as_str().to_string();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(
                "Failed to submit reputation record: Status {}, Body: {}",
                status_code, body
            );
            // Increment counter for non-2xx HTTP responses from the service
            metrics::REPUTATION_SUBMISSION_HTTP_ERRORS
                .with_label_values(&[record.subject.as_str(), &status_str]) // Using record.subject as executor_did
                .inc();
            // metrics::record_reputation_update_failure(); // Removed, handled by increment_reputation_submission
            anyhow::bail!("Failed to submit reputation record: HTTP Status {}", status_code)
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
    async fn test_http_submit_receipt_success_modifier_disabled() {
        // 1. Setup MockServer
        let server = MockServer::start();

        // 2. Create HttpReputationUpdater
        let keypair = KeyPair::generate(); // Generate a DID for the updater
        let local_did = keypair.did;
        
        let mut config = ReputationScoringConfig::default();
        config.enable_reputation_modifier = false;
        // Use specific values for reproducible score_delta
        config.sigmoid_k = 0.01; 
        config.sigmoid_midpoint = 50.0;
        config.max_positive_score = 10.0;


        let updater = HttpReputationUpdater::new_with_config(
            server.base_url(), // Use mock server's URL
            local_did,
            config.clone() 
        );

        // 3. Create a sample RuntimeExecutionReceipt
        let executor_keypair = KeyPair::generate();
        let receipt_mana_cost = Some(100u64);
        let test_receipt = RuntimeExecutionReceipt {
            id: "test-receipt-id".to_string(),
            issuer: executor_keypair.did.to_string(),
            proposal_id: "prop-1".to_string(),
            wasm_cid: "wasm-cid".to_string(),
            ccl_cid: "ccl-cid".to_string(),
            metrics: RuntimeExecutionMetrics {
                host_calls: 1, // Populated to avoid default() issues if any
                io_bytes: 10,  // Populated
                mana_cost: receipt_mana_cost,
            },
            anchored_cids: vec![],
            resource_usage: vec![],
            timestamp: 1234567890,
            dag_epoch: Some(1),
            receipt_cid: Some("bafy...mockcid".to_string()),
            signature: None, // Signature not relevant for this part of the test
        };

        // 4. Mock the HTTP POST request
        let mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/") // Assuming service URL is just the base_url
                .header("content-type", "application/json");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({ "status": "ok" }));
        });

        // 5. Call updater.submit_receipt_based_reputation
        let result = updater.submit_receipt_based_reputation(
            &test_receipt,
            true, // is_successful
            "test-coop",
            "test-community"
        ).await;

        // 6. Assert mock server received the request
        mock.assert();
        assert!(result.is_ok(), "Expected successful submission, got {:?}", result.err());

        // 7. Assert the content of the ReputationRecord
        let submitted_json = mock.requests()[0].body_json::<serde_json::Value>().unwrap();
        
        let expected_score_delta = calculate_expected_score_delta(&config, receipt_mana_cost, true);

        assert_eq!(submitted_json["subject"], test_receipt.issuer);
        assert_eq!(submitted_json["anchor"], test_receipt.receipt_cid.as_ref().unwrap());
        assert_eq!(submitted_json["success"], true);
        assert_eq!(submitted_json["mana_cost"], receipt_mana_cost.unwrap());
        // For score_delta, compare f64 with a small epsilon or check if close enough
        let submitted_score_delta = submitted_json["score_delta"].as_f64().unwrap();
        assert!((submitted_score_delta - expected_score_delta).abs() < 1e-9, "Score delta mismatch");
        // Timestamp is dynamic, so we don't assert its exact value here
        // but check it exists
        assert!(submitted_json["timestamp"].is_u64());
    }

    #[tokio::test]
    async fn test_http_submit_receipt_success_modifier_enabled_score_fetched() {
        // Scenario: Modifier Enabled – Success Path with Current Score = 80
        let server = MockServer::start();
        let keypair = KeyPair::generate();
        let local_did = keypair.did.clone();
        let executor_keypair = KeyPair::generate();
        let executor_did_str = executor_keypair.did.to_string();

        let mut config = ReputationScoringConfig::default();
        config.enable_reputation_modifier = true;
        // Consistent params for base score calculation
        config.sigmoid_k = 0.01;
        config.sigmoid_midpoint = 50.0;
        config.max_positive_score = 10.0;
        // Modifier bounds
        config.modifier_min_bound = 0.5;
        config.modifier_max_bound = 2.0;

        let updater = HttpReputationUpdater::new_with_config(
            server.base_url(),
            local_did,
            config.clone()
        );

        let receipt_mana_cost = Some(100u64);
        let test_receipt = RuntimeExecutionReceipt {
            id: "test-receipt-mod-enabled".to_string(),
            issuer: executor_did_str.clone(),
            proposal_id: "prop-mod".to_string(),
            wasm_cid: "wasm-cid-mod".to_string(),
            ccl_cid: "ccl-cid-mod".to_string(),
            metrics: RuntimeExecutionMetrics { host_calls: 1, io_bytes: 10, mana_cost: receipt_mana_cost },
            anchored_cids: vec![],
            resource_usage: vec![],
            timestamp: 1234567891,
            dag_epoch: Some(1),
            receipt_cid: Some("bafy...mockcidMOD".to_string()),
            signature: None,
        };

        // Mock for GET /reputation/profiles/{did}
        let get_score_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path(format!("/reputation/profiles/{}", executor_did_str));
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({ "computed_score": 80.0 }));
        });

        // Mock for POST / (main submission)
        let post_submission_mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST).path("/");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({ "status": "ok" }));
        });

        let result = updater.submit_receipt_based_reputation(
            &test_receipt,
            true, // is_successful
            "test-coop-mod",
            "test-community-mod"
        ).await;

        get_score_mock.assert();
        post_submission_mock.assert();
        assert!(result.is_ok(), "Expected successful submission, got {:?}", result.err());

        let submitted_json = post_submission_mock.requests()[0].body_json::<serde_json::Value>().unwrap();

        let base_sigmoid_score = calculate_expected_score_delta(&config, receipt_mana_cost, true);
        let current_score_from_service = 80.0;
        let assumed_max_score = 100.0; // As hardcoded in HttpReputationUpdater
        let normalized_current_score = (current_score_from_service / assumed_max_score).clamp(0.0, 1.0);
        let mut expected_modifier = (1.0 + normalized_current_score)
            .clamp(config.modifier_min_bound, config.modifier_max_bound);
        
        // Ensure modifier is not optimized away if it's 1.0 but modifier is disabled
        // This test has modifier enabled, so this check is more for sanity.
        if !config.enable_reputation_modifier {
             expected_modifier = 1.0;
        }

        let expected_score_delta_with_modifier = (base_sigmoid_score * expected_modifier).min(config.max_positive_score);

        assert_eq!(submitted_json["subject"], test_receipt.issuer);
        assert_eq!(submitted_json["anchor"], test_receipt.receipt_cid.as_ref().unwrap());
        let submitted_score_delta = submitted_json["score_delta"].as_f64().unwrap();
        assert!((submitted_score_delta - expected_score_delta_with_modifier).abs() < 1e-9,
            "Score delta mismatch. Expected (with mod): {}, Got: {}, Base (no mod): {}, Modifier: {}",
            expected_score_delta_with_modifier, submitted_score_delta, base_sigmoid_score, expected_modifier);
        assert!(submitted_json["timestamp"].is_u64());
    }

    #[tokio::test]
    async fn test_http_submit_receipt_success_modifier_enabled_score_fails() {
        // Scenario: Modifier Enabled – Success Path with get_current_score Failure (e.g., 503 Service Unavailable)
        let server = MockServer::start();
        let keypair = KeyPair::generate();
        let local_did = keypair.did.clone();
        let executor_keypair = KeyPair::generate();
        let executor_did_str = executor_keypair.did.to_string();

        let mut config = ReputationScoringConfig::default();
        config.enable_reputation_modifier = true;
        config.sigmoid_k = 0.01;
        config.sigmoid_midpoint = 50.0;
        config.max_positive_score = 10.0;
        config.modifier_min_bound = 0.5;
        config.modifier_max_bound = 2.0;

        let updater = HttpReputationUpdater::new_with_config(
            server.base_url(),
            local_did,
            config.clone()
        );

        let receipt_mana_cost = Some(100u64);
        let test_receipt = RuntimeExecutionReceipt {
            id: "test-receipt-mod-fail".to_string(),
            issuer: executor_did_str.clone(),
            proposal_id: "prop-mod-fail".to_string(),
            wasm_cid: "wasm-cid-mod-fail".to_string(),
            ccl_cid: "ccl-cid-mod-fail".to_string(),
            metrics: RuntimeExecutionMetrics { host_calls: 1, io_bytes: 10, mana_cost: receipt_mana_cost },
            anchored_cids: vec![],
            resource_usage: vec![],
            timestamp: 1234567892,
            dag_epoch: Some(1),
            receipt_cid: Some("bafy...mockcidMODFAIL".to_string()),
            signature: None,
        };

        // Mock for GET /reputation/profiles/{did} - simulate failure (503 Service Unavailable)
        let get_score_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path(format!("/reputation/profiles/{}", executor_did_str));
            then.status(503);
        });

        // Mock for POST / (main submission)
        let post_submission_mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST).path("/");
            then.status(200).json_body(json!({ "status": "ok" }));
        });

        let metric_labels = [executor_did_str.as_str(), "503"];
        let initial_metric_value = metrics::REPUTATION_SCORE_FETCH_FAILURES
            .get_metric_with_label_values(&metric_labels)
            .map_or(0.0, |m| m.get());

        let result = updater.submit_receipt_based_reputation(
            &test_receipt,
            true, // is_successful
            "test-coop-mod-fail",
            "test-community-mod-fail"
        ).await;

        let final_metric_value = metrics::REPUTATION_SCORE_FETCH_FAILURES
            .get_metric_with_label_values(&metric_labels)
            .map_or(0.0, |m| m.get());

        assert_eq!(final_metric_value - initial_metric_value, 1.0, "REPUTATION_SCORE_FETCH_FAILURES should increment by 1");

        get_score_mock.assert();
        post_submission_mock.assert();
        assert!(result.is_ok(), "Expected successful submission logic (despite score fetch failure), got {:?}", result.err());

        let submitted_json = post_submission_mock.requests()[0].body_json::<serde_json::Value>().unwrap();

        let base_sigmoid_score = calculate_expected_score_delta(&config, receipt_mana_cost, true);
        // Current HttpReputationUpdater::get_current_score maps failure/None to a default normalized score of 0.5
        let normalized_current_score_on_failure = 0.5;
        let mut expected_modifier = (1.0 + normalized_current_score_on_failure)
            .clamp(config.modifier_min_bound, config.modifier_max_bound);

        if !config.enable_reputation_modifier { // Should be true for this test
             expected_modifier = 1.0;
        }

        let expected_score_delta_with_modifier = (base_sigmoid_score * expected_modifier).min(config.max_positive_score);

        assert_eq!(submitted_json["subject"], test_receipt.issuer);
        let submitted_score_delta = submitted_json["score_delta"].as_f64().unwrap();
        assert!((submitted_score_delta - expected_score_delta_with_modifier).abs() < 1e-9,
            "Score delta mismatch. Expected (with mod on fail): {}, Got: {}, Base (no mod): {}, Modifier: {}",
            expected_score_delta_with_modifier, submitted_score_delta, base_sigmoid_score, expected_modifier);
    }

    #[tokio::test]
    async fn test_http_submit_receipt_failure_path() {
        // Scenario: Failure Path – is_successful = false
        let server = MockServer::start();
        let keypair = KeyPair::generate();
        let local_did = keypair.did.clone();
        let executor_keypair = KeyPair::generate();

        let mut config = ReputationScoringConfig::default();
        // Explicitly set modifier to true to ensure it's NOT applied on failure path
        config.enable_reputation_modifier = true; 
        config.failure_penalty_weight = 5.0; // For predictable negative score
        // Sigmoid params don't matter here but set for consistency if base_score was ever used
        config.sigmoid_k = 0.01;
        config.sigmoid_midpoint = 50.0;
        config.max_positive_score = 10.0;

        let updater = HttpReputationUpdater::new_with_config(
            server.base_url(),
            local_did,
            config.clone()
        );

        let receipt_mana_cost = Some(120u64);
        let test_receipt = RuntimeExecutionReceipt {
            id: "test-receipt-fail-path".to_string(),
            issuer: executor_keypair.did.to_string(),
            proposal_id: "prop-fail-path".to_string(),
            wasm_cid: "wasm-cid-fail-path".to_string(),
            ccl_cid: "ccl-cid-fail-path".to_string(),
            metrics: RuntimeExecutionMetrics { host_calls: 1, io_bytes: 10, mana_cost: receipt_mana_cost },
            anchored_cids: vec![],
            resource_usage: vec![],
            timestamp: 1234567893,
            dag_epoch: Some(1),
            receipt_cid: Some("bafy...mockcidFAILPATH".to_string()),
            signature: None,
        };

        // Mock for POST / (main submission) - expect success: false and negative score_delta
        let post_submission_mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST).path("/");
            then.status(200).json_body(json!({ "status": "ok" }));
        });

        // No GET mock needed as modifier path should not be taken

        let result = updater.submit_receipt_based_reputation(
            &test_receipt,
            false, // is_successful = false
            "test-coop-fail",
            "test-community-fail"
        ).await;

        post_submission_mock.assert();
        assert!(result.is_ok(), "Expected successful HTTP submission for failure path, got {:?}", result.err());

        let submitted_json = post_submission_mock.requests()[0].body_json::<serde_json::Value>().unwrap();
        
        // calculate_expected_score_delta already handles the 'is_successful = false' case
        let expected_score_delta = calculate_expected_score_delta(&config, receipt_mana_cost, false);

        assert_eq!(submitted_json["subject"], test_receipt.issuer);
        assert_eq!(submitted_json["success"], false);
        let submitted_score_delta = submitted_json["score_delta"].as_f64().unwrap();
        assert!((submitted_score_delta - expected_score_delta).abs() < 1e-9,
            "Negative score delta mismatch. Expected: {}, Got: {}", expected_score_delta, submitted_score_delta);
        assert!(submitted_score_delta < 0.0, "Score delta should be negative for failure path");
    }

    #[tokio::test]
    async fn test_http_submit_receipt_http_post_error_500() {
        // Scenario: HTTP Failure – 500 Internal Server Error on POST
        let server = MockServer::start();
        let keypair = KeyPair::generate();
        let local_did = keypair.did.clone();
        let executor_keypair = KeyPair::generate();
        let executor_did_str = executor_keypair.did.to_string();

        // Config doesn't significantly impact this test, but use default
        let config = ReputationScoringConfig::default();

        let updater = HttpReputationUpdater::new_with_config(
            server.base_url(),
            local_did,
            config.clone()
        );

        let test_receipt = RuntimeExecutionReceipt {
            id: "test-receipt-http-500".to_string(),
            issuer: executor_did_str.clone(), // Use the string form for consistency
            proposal_id: "prop-http-500".to_string(),
            wasm_cid: "wasm-cid-http-500".to_string(),
            ccl_cid: "ccl-cid-http-500".to_string(),
            metrics: RuntimeExecutionMetrics { host_calls: 1, io_bytes: 10, mana_cost: Some(10u64) },
            anchored_cids: vec![],
            resource_usage: vec![],
            timestamp: 1234567894,
            dag_epoch: Some(1),
            receipt_cid: Some("bafy...mockcidHTTP500".to_string()),
            signature: None,
        };

        // Mock for POST / - respond with 500 Internal Server Error
        let post_submission_mock = server.mock(|when, then| {
            when.method(httpmock::Method::POST).path("/");
            then.status(500).body("Internal Server Error simulation");
        });

        let metric_labels = [executor_did_str.as_str(), "500"]; // executor_did, status
        let initial_metric_value = metrics::REPUTATION_SUBMISSION_HTTP_ERRORS
            .get_metric_with_label_values(&metric_labels)
            .map_or(0.0, |m| m.get());

        let result = updater.submit_receipt_based_reputation(
            &test_receipt,
            true, // is_successful (so it attempts submission)
            "test-coop-http-err",
            "test-community-http-err"
        ).await;

        let final_metric_value = metrics::REPUTATION_SUBMISSION_HTTP_ERRORS
            .get_metric_with_label_values(&metric_labels)
            .map_or(0.0, |m| m.get());

        assert_eq!(final_metric_value - initial_metric_value, 1.0, "REPUTATION_SUBMISSION_HTTP_ERRORS should increment by 1");

        post_submission_mock.assert(); // Ensure the call was attempted
        assert!(result.is_err(), "Expected an error result due to HTTP 500");
        
        // Optionally, check the error message if it's specific enough
        // e.g., format!("Failed to submit reputation record: {}", reqwest::StatusCode::INTERNAL_SERVER_ERROR)
        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("Failed to submit reputation record"), "Error message mismatch: {}", err_msg);
        assert!(err_msg.contains("500"), "Error message should contain status 500: {}", err_msg);
    }

    #[tokio::test]
    async fn test_http_submit_receipt_malformed_url() {
        // Scenario: Malformed ReputationService URL
        // No MockServer needed here as the error should occur before HTTP communication starts or during client build.

        let keypair = KeyPair::generate();
        let local_did = keypair.did.clone();
        let executor_keypair = KeyPair::generate();
        let executor_did_str = executor_keypair.did.to_string();

        let config = ReputationScoringConfig::default(); // Modifier disabled by default

        let malformed_url = "this is not a valid url";
        let updater = HttpReputationUpdater::new_with_config(
            malformed_url.to_string(),
            local_did,
            config.clone()
        );

        let test_receipt = RuntimeExecutionReceipt {
            id: "test-receipt-bad-url".to_string(),
            issuer: executor_did_str.clone(),
            proposal_id: "prop-bad-url".to_string(),
            wasm_cid: "wasm-cid-bad-url".to_string(),
            ccl_cid: "ccl-cid-bad-url".to_string(),
            metrics: RuntimeExecutionMetrics { host_calls: 1, io_bytes: 10, mana_cost: Some(10u64) },
            anchored_cids: vec![],
            resource_usage: vec![],
            timestamp: 1234567895,
            dag_epoch: Some(1),
            receipt_cid: Some("bafy...mockcidBADURL".to_string()),
            signature: None,
        };

        // We need to capture the expected error string for the metric label
        // This is a bit tricky as it's internal to reqwest. Let's try to predict or run once to see.
        // A common error for such a URL is "relative URL without a base"
        // The metric is incremented in map_err, so the error is whatever reqwest::Error::to_string() gives.
        // For now, we'll fetch the metric count and if it increments, we know the label was matched by err.to_string().
        // A more robust test might involve a more specific type of client error if this proves flaky.

        // Initialize a temporary variable for the expected reason label.
        // This will be populated if/when the actual error occurs.
        let mut expected_reason_label = String::new();

        // Get initial metric value. Since the reason label is dynamic (the error string itself),
        // we can't easily get it before the error. Instead, we check if *any* client error for this DID incremented,
        // or refine this if we can determine the exact error string beforehand.
        // For now, let's just check if *a* client error for this DID was recorded.
        // A simpler approach: just check the count *after* and assume if it's 1, it was this error.
        // This is okay if tests are isolated or we clear metrics, which we are not doing here.
        // Let's try to get the specific error string from the result.

        let result = updater.submit_receipt_based_reputation(
            &test_receipt,
            true, // is_successful
            "test-coop-bad-url",
            "test-community-bad-url"
        ).await;

        assert!(result.is_err(), "Expected an error result due to malformed URL");
        let actual_err = result.err().unwrap();
        expected_reason_label = actual_err.to_string(); // This is the actual error message used as reason
        
        let metric_labels = [executor_did_str.as_str(), expected_reason_label.as_str()];
        let final_metric_value = metrics::REPUTATION_SUBMISSION_CLIENT_ERRORS
            .get_metric_with_label_values(&metric_labels)
            .map_or(0.0, |m| m.get());

        // We can't easily get initial_metric_value for this specific dynamic label beforehand.
        // So we assert that the final count is at least 1.
        // This assumes that this specific error string (reason) hasn't occurred before for this DID in this test run.
        // For truly isolated test of this counter, one would need to ensure the metric is 0 before this specific error.
        assert_eq!(final_metric_value, 1.0, 
            "REPUTATION_SUBMISSION_CLIENT_ERRORS should be 1 for this specific error. Label: {}", expected_reason_label);

        // Previous error string check remains useful for general error type validation
        let err_string_lowercase = actual_err.to_string().to_lowercase();
        assert!(
            err_string_lowercase.contains("url") && (err_string_lowercase.contains("invalid") || err_string_lowercase.contains("builder error") || err_string_lowercase.contains("relative")) || err_string_lowercase.contains("failed to send request"),
            "Error message should indicate a URL parsing or request sending issue: {}", err_string_lowercase
        );
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

    // --- Tests for ReputationScoringConfig ---
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test] // Not async
    fn test_config_from_file_valid() {
        // Scenario: ReputationScoringConfig::from_file() – Valid File
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"
            mana_cost_weight = 150.0
            failure_penalty = -30.0
            max_positive_score = 7.5
            sigmoid_k = 0.025
            sigmoid_midpoint = 75.0
            failure_penalty_weight = 6.0
            enable_reputation_modifier = true
            modifier_min_bound = 0.6
            modifier_max_bound = 2.2
        "#;
        write!(temp_file, "{}", config_content).unwrap();

        let loaded_config = ReputationScoringConfig::from_file(temp_file.path()).unwrap();

        assert_eq!(loaded_config.mana_cost_weight, 150.0);
        assert_eq!(loaded_config.failure_penalty, -30.0);
        assert_eq!(loaded_config.max_positive_score, 7.5);
        assert_eq!(loaded_config.sigmoid_k, 0.025);
        assert_eq!(loaded_config.sigmoid_midpoint, 75.0);
        assert_eq!(loaded_config.failure_penalty_weight, 6.0);
        assert_eq!(loaded_config.enable_reputation_modifier, true);
        assert_eq!(loaded_config.modifier_min_bound, 0.6);
        assert_eq!(loaded_config.modifier_max_bound, 2.2);
    }

    #[test] // Not async
    fn test_config_from_file_missing() {
        // Scenario: ReputationScoringConfig::from_file() – Missing File
        let missing_path = Path::new("this/path/should/definitely/not/exist.toml");
        let result = ReputationScoringConfig::from_file(missing_path);

        assert!(result.is_err(), "Expected an error when loading a missing config file");
        let err_msg = result.err().unwrap().to_string();
        // Check for part of the error message from fs::read_to_string failure
        assert!(
            err_msg.contains("Failed to read reputation config file") && err_msg.contains("No such file or directory"),
            "Error message mismatch: {}", err_msg
        );
    }
} 