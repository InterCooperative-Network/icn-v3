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
}

impl Default for ReputationScoringConfig {
    fn default() -> Self {
        Self { mana_cost_weight: 100.0, failure_penalty: -25.0 } // Default weight and penalty
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
        is_successful: bool, // Added success status parameter
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
        is_successful: bool, // Use the parameter
    ) -> Result<()> {
        metrics::record_reputation_update_attempt();

        let anchor_str = receipt.receipt_cid.clone().unwrap_or_else(|| {
             tracing::warn!("RuntimeExecutionReceipt {} is missing receipt_cid for reputation update.", receipt.id);
            "missing_cid".to_string()
        });

        // Use the passed is_successful status
        let success = is_successful;

        let score_delta = if success {
            match receipt.metrics.mana_cost {
                Some(cost) if cost > 0 => self.config.mana_cost_weight / (cost as f64),
                _ => 0.0,
            }
        } else {
            self.config.failure_penalty
        };

        let record = ReputationRecord {
            subject: receipt.issuer.clone(),
            anchor: anchor_str,
            score_delta,
            success, // Reflect the success status in the record
            mana_cost: receipt.metrics.mana_cost,
            timestamp: receipt.timestamp,
        };

        let url = format!("{}/reputation/records", self.reputation_service_url.trim_end_matches('/'));
        tracing::info!(
            "Submitting reputation record for subject {} (anchor: {}, mana_cost: {:?}, success: {}, delta: {:.2})",
            record.subject, record.anchor, record.mana_cost, record.success, record.score_delta
        );
        let response = match self.client.post(&url).json(&record).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("HTTP request failed when submitting reputation record: {}", e);
                metrics::record_reputation_update_failure();
                return Err(e.into());
            }
        };
        if response.status().is_success() {
            tracing::info!(
                "Successfully submitted reputation record for subject {} (anchor: {})",
                record.subject, record.anchor
            );
            metrics::record_reputation_update_success();
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!(
                "Failed to submit reputation record: Status {}, Body: {}",
                status, body
            );
            metrics::record_reputation_update_failure();
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
        submitted_items: Arc<Mutex<Vec<(RuntimeExecutionReceipt, bool)>>>,
    }
    
    impl MockReputationUpdater {
        fn new() -> Self {
            Self {
                submitted_items: Arc::new(Mutex::new(Vec::new())),
            }
        }
        
        // Getter might change if you want to inspect the success bool
        fn get_submitted_receipts(&self) -> Vec<RuntimeExecutionReceipt> {
            self.submitted_items.lock().unwrap().iter().map(|(r, _s)| r.clone()).collect()
        }
    }
    
    #[async_trait::async_trait]
    impl ReputationUpdater for MockReputationUpdater {
        async fn submit_receipt_based_reputation(
            &self, 
            receipt: &RuntimeExecutionReceipt,
            is_successful: bool, // Accept new parameter
        ) -> Result<()> {
            self.submitted_items.lock().unwrap().push((receipt.clone(), is_successful));
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
        updater.submit_receipt_based_reputation(&receipt, true).await.unwrap();
        
        let submitted = mock_updater.get_submitted_receipts();
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].id, "test-receipt-1");
    }
} 