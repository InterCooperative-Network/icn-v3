use anyhow::Result;
use reqwest::Client;
use icn_types::runtime_receipt::RuntimeExecutionReceipt;
use icn_types::reputation::{ReputationRecord, ReputationUpdateEvent};
use std::time::Duration;
use cid::Cid;
use cid::multihash::{Multihash, Code};
use chrono::Utc;
use icn_identity::Did;
use tracing;
use std::str::FromStr;
use multihash::{Hasher, Sha2_256};

use crate::metrics;

/// This trait allows providing different implementations of reputation update
/// logic for testing and production environments
#[async_trait::async_trait]
pub trait ReputationUpdater: Send + Sync {
    /// Submit a reputation record derived from a runtime execution receipt
    async fn submit_receipt_based_reputation(&self, receipt: &RuntimeExecutionReceipt) -> Result<()>;
}

/// The real implementation that sends HTTP requests to the reputation service
pub struct HttpReputationUpdater {
    client: Client,
    reputation_service_url: String,
    local_did: Did,
}

impl HttpReputationUpdater {
    pub fn new(reputation_service_url: String, local_did: Did) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client for reputation updater");
            
        Self { client, reputation_service_url, local_did }
    }
    
    fn create_reputation_record(&self, receipt: &RuntimeExecutionReceipt) -> Result<ReputationRecord> {
        // Parse the receipt CID if available
        let anchor_cid = if let Some(cid_str) = &receipt.receipt_cid {
            Some(Cid::from_str(cid_str.as_str())?)
        } else {
            None
        };
        
        // Determine the appropriate reputation event based on receipt data
        let event = if receipt.metrics.fuel_used > 0 {
            // Successfully executed code that consumed resources
            ReputationUpdateEvent::JobCompletedSuccessfully {
                job_id: anchor_cid.unwrap_or_else(|| {
                    // Generate a placeholder CID if not available
                    let mut hasher = Sha2_256::default();
                    hasher.update(format!("{}:{}", receipt.id, receipt.timestamp).as_bytes());
                    let hash_bytes = hasher.finalize();
                    // Create Multihash from bytes (v0.10.1 compatible)
                    let mh = Multihash::from_bytes(&hash_bytes).expect("Failed to create multihash");
                    Cid::new_v1(0x70, mh) // 0x70 is the 'raw' codec for Cid v0.10.1
                }),
                execution_duration_ms: (receipt.metrics.fuel_used / 100) as u32, // Using fuel as proxy for duration
                bid_accuracy: 1.0, // Placeholder
                on_time: true,     // Placeholder
                anchor_cid,
            }
        } else {
            // Either failed execution or zero resource consumption
            ReputationUpdateEvent::JobFailed {
                job_id: anchor_cid.unwrap_or_else(|| {
                    // Generate a placeholder CID if not available
                    let mut hasher = Sha2_256::default();
                    hasher.update(format!("{}:{}", receipt.id, receipt.timestamp).as_bytes());
                    let hash_bytes = hasher.finalize();
                    // Create Multihash from bytes (v0.10.1 compatible)
                    let mh = Multihash::from_bytes(&hash_bytes).expect("Failed to create multihash");
                    Cid::new_v1(0x70, mh) // 0x70 is the 'raw' codec for Cid v0.10.1
                }),
                reason: "Execution failed or zero resource consumption".into(),
                anchor_cid,
            }
        };
        
        // Create the reputation record
        let record = ReputationRecord {
            timestamp: Utc::now(),
            issuer: self.local_did.clone(),
            subject: Did::from_str(&receipt.issuer)?, // The node that executed the job
            event,
            anchor: anchor_cid,
            signature: None, // Signature would be added by the service if needed
        };
        
        Ok(record)
    }
}

#[async_trait::async_trait]
impl ReputationUpdater for HttpReputationUpdater {
    async fn submit_receipt_based_reputation(&self, receipt: &RuntimeExecutionReceipt) -> Result<()> {
        // Record the attempt in metrics
        metrics::record_reputation_update_attempt();
        
        // Create a reputation record from the receipt
        let record = match self.create_reputation_record(receipt) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to create reputation record: {}", e);
                metrics::record_reputation_update_failure();
                return Err(e);
            }
        };
        
        // Submit the record to the reputation service
        let url = format!("{}/reputation/records", self.reputation_service_url.trim_end_matches('/'));
        
        tracing::info!(
            "Submitting reputation record for {} based on runtime receipt {}",
            record.subject, receipt.id
        );
        
        let response = match self.client.post(&url)
            .json(&record)
            .send()
            .await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("HTTP request failed when submitting reputation record: {}", e);
                    metrics::record_reputation_update_failure();
                    return Err(e.into());
                }
            };
            
        if response.status().is_success() {
            tracing::info!(
                "Successfully submitted reputation record for {} based on runtime receipt {}",
                record.subject, receipt.id
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
    async fn submit_receipt_based_reputation(&self, _receipt: &RuntimeExecutionReceipt) -> Result<()> {
        // Do nothing
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
    
    // A mock implementation for testing
    #[derive(Clone)]
    struct MockReputationUpdater {
        submitted_records: Arc<Mutex<Vec<RuntimeExecutionReceipt>>>,
    }
    
    impl MockReputationUpdater {
        fn new() -> Self {
            Self {
                submitted_records: Arc::new(Mutex::new(Vec::new())),
            }
        }
        
        fn get_submitted_records(&self) -> Vec<RuntimeExecutionReceipt> {
            self.submitted_records.lock().unwrap().clone()
        }
    }
    
    #[async_trait::async_trait]
    impl ReputationUpdater for MockReputationUpdater {
        async fn submit_receipt_based_reputation(&self, receipt: &RuntimeExecutionReceipt) -> Result<()> {
            self.submitted_records.lock().unwrap().push(receipt.clone());
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_reputation_update_from_receipt() {
        // Setup
        let mock_updater = MockReputationUpdater::new();
        let updater = Arc::new(mock_updater.clone());
        
        // Create a test receipt
        let receipt = RuntimeExecutionReceipt {
            id: "test-receipt-1".into(),
            issuer: "did:key:test-executor".into(),
            proposal_id: "test-proposal".into(),
            wasm_cid: "test-wasm-cid".into(),
            ccl_cid: "test-ccl-cid".into(),
            metrics: RuntimeExecutionMetrics {
                fuel_used: 1000,
                host_calls: 50,
                io_bytes: 2048,
            },
            anchored_cids: vec!["test-anchored-cid".into()],
            resource_usage: vec![("cpu".into(), 100), ("memory".into(), 1024)],
            timestamp: 1234567890,
            dag_epoch: Some(42),
            receipt_cid: Some("test-receipt-cid".into()),
            signature: None,
        };
        
        // Submit the receipt
        updater.submit_receipt_based_reputation(&receipt).await.unwrap();
        
        // Verify the receipt was submitted
        let submitted = mock_updater.get_submitted_records();
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].id, "test-receipt-1");
    }
} 