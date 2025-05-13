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

use crate::metrics;

/// Configuration for reputation scoring parameters
#[derive(Debug, Clone)]
pub struct ReputationScoringConfig {
    pub mana_cost_weight: f64, // Weight factor for mana cost scoring (e.g., numerator in 1/cost)
    pub failure_penalty: f64, // Flat penalty score for failed submissions
    pub max_positive_score: f64, // Maximum possible score delta for a successful, mana-based update
}

impl Default for ReputationScoringConfig {
    fn default() -> Self {
        Self { 
            mana_cost_weight: 100.0, 
            failure_penalty: -25.0, 
            max_positive_score: 5.0, // Default cap for positive scores
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
        // Extract executor DID from receipt issuer
        let executor_did = receipt.issuer.as_str();

        // Calculate score delta based on success and mana cost using config
        let score_delta = if is_successful {
            let mana_cost = receipt.metrics.mana_cost.unwrap_or(1); // Avoid division by zero, treat 0 cost as 1
            if mana_cost == 0 {
                // Assign max score if mana cost is 0 (or handle as appropriate)
                self.config.max_positive_score
            } else {
                // Apply weight and cap the score
                (self.config.mana_cost_weight / mana_cost as f64)
                    .min(self.config.max_positive_score) 
            }
        } else {
            self.config.failure_penalty
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
    
    #[derive(Clone)]
    struct MockReputationUpdater {
        // Store a tuple if you want to assert the success status
        submitted_items: Arc<Mutex<Vec<(RuntimeExecutionReceipt, bool, String, String)>>>,
    }
    
    impl MockReputationUpdater {
        fn new() -> Self {
            Self {
                submitted_items: Arc::new(Mutex::new(Vec::new())),
            }
        }
        
        // Getter might change if you want to inspect the success bool
        fn get_submissions(&self) -> Vec<(RuntimeExecutionReceipt, bool, String, String)> {
            self.submitted_items.lock().unwrap().clone()
        }
    }
    
    #[async_trait::async_trait]
    impl ReputationUpdater for MockReputationUpdater {
        async fn submit_receipt_based_reputation(
            &self,
            receipt: &RuntimeExecutionReceipt,
            is_successful: bool,
            coop_id: &str,
            community_id: &str,
        ) -> Result<()> {
            self.submitted_items.lock().unwrap().push((
                receipt.clone(), 
                is_successful, 
                coop_id.to_string(), 
                community_id.to_string()
            ));
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_reputation_update_from_receipt() {
        let mock_updater = MockReputationUpdater::new();
        let updater = Arc::new(mock_updater.clone());
        
        let receipt = RuntimeExecutionReceipt {
            id: "test-receipt-1".into(),
            issuer: "did:key:test-executor".into(),
            proposal_id: "test-proposal".into(),
            wasm_cid: "test-wasm-cid".into(),
            ccl_cid: "test-ccl-cid".into(),
            metrics: RuntimeExecutionMetrics {
                host_calls: 50,
                io_bytes: 2048,
                mana_cost: Some(1000),
            },
            anchored_cids: vec!["test-anchored-cid".into()],
            resource_usage: vec![("cpu".into(), 100), ("memory".into(), 1024)],
            timestamp: 1234567890,
            dag_epoch: Some(42),
            receipt_cid: Some("test-receipt-cid".into()),
            signature: None,
        };
        
        // Pass the new is_successful parameter
        updater.submit_receipt_based_reputation(&receipt, true, "test-coop", "test-community").await.unwrap();
        
        let submitted = mock_updater.get_submissions();
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].0.id, "test-receipt-1");
        assert_eq!(submitted[0].1, true);
        assert_eq!(submitted[0].2, "test-coop");
        assert_eq!(submitted[0].3, "test-community");
    }
} 