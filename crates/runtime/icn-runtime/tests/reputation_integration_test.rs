use anyhow::Result;
use async_trait::async_trait;
use icn_runtime::{
    RuntimeStorage, Runtime, RuntimeContext, RuntimeContextBuilder,
    anchor_receipt, metrics,
    reputation_integration::{ReputationUpdater, HttpReputationUpdater}
};
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use icn_identity::KeyPair;
use mockito::{mock, server_url};
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use std::collections::HashMap;

/// Mock storage implementation for testing
struct MockStorage {
    anchored_data: Arc<Mutex<HashMap<String, String>>>,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            anchored_data: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    fn get_anchored_data(&self, cid: &str) -> Option<String> {
        self.anchored_data.lock().unwrap().get(cid).cloned()
    }
}

#[async_trait]
impl RuntimeStorage for MockStorage {
    async fn load_proposal(&self, _id: &str) -> Result<icn_runtime::Proposal> {
        unimplemented!("Not needed for this test")
    }

    async fn update_proposal(&self, _proposal: &icn_runtime::Proposal) -> Result<()> {
        unimplemented!("Not needed for this test")
    }

    async fn load_wasm(&self, _cid: &str) -> Result<Vec<u8>> {
        unimplemented!("Not needed for this test")
    }

    async fn store_receipt(&self, _receipt: &icn_runtime::ExecutionReceipt) -> Result<String> {
        Ok(Uuid::new_v4().to_string())
    }

    async fn anchor_to_dag(&self, data: &str) -> Result<String> {
        let cid = format!("test-cid-{}", Uuid::new_v4());
        self.anchored_data.lock().unwrap().insert(cid.clone(), data.to_string());
        Ok(cid)
    }
}

#[tokio::test]
async fn test_runtime_reputation_integration() -> Result<()> {
    // Setup a mock server for the reputation service
    let mock_server = mock("POST", "/reputation/records")
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status":"created"}"#)
        .create();
    
    // Generate a test keypair
    let keypair = KeyPair::generate();
    let identity_did = keypair.did().to_string();
    
    // Create a test context with reputation service
    let context = RuntimeContextBuilder::new()
        .with_identity(keypair)
        .with_reputation_service(format!("{}/reputation", server_url()))
        .build();
    
    // Create storage and runtime
    let storage = Arc::new(MockStorage::new());
    let runtime = Runtime::with_context(storage.clone(), context);
    
    // Create a test receipt
    let receipt = RuntimeExecutionReceipt {
        id: "test-receipt-1".into(),
        issuer: identity_did,
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
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        dag_epoch: Some(42),
        receipt_cid: None,
        signature: None,
    };
    
    // Anchor the receipt (which should automatically trigger reputation update)
    let receipt_cid = runtime.anchor_receipt(&receipt).await?;
    
    // Verify the receipt was anchored
    let storage_ref = storage.as_ref() as &MockStorage;
    let anchored_data = storage_ref.get_anchored_data(&receipt_cid)
        .expect("Receipt should be anchored");
    
    // Parse the anchored receipt
    let anchored_receipt: RuntimeExecutionReceipt = serde_json::from_str(&anchored_data)?;
    assert_eq!(anchored_receipt.id, "test-receipt-1");
    
    // Verify the mock reputation service was called
    mock_server.assert();
    
    Ok(())
}

#[tokio::test]
async fn test_runtime_reputation_failure_handling() -> Result<()> {
    // Setup a mock server for the reputation service that will fail
    let mock_server = mock("POST", "/reputation/records")
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error":"internal server error"}"#)
        .create();
    
    // Generate a test keypair
    let keypair = KeyPair::generate();
    let identity_did = keypair.did().to_string();
    
    // Create a test context with reputation service
    let context = RuntimeContextBuilder::new()
        .with_identity(keypair)
        .with_reputation_service(format!("{}/reputation", server_url()))
        .build();
    
    // Create storage and runtime
    let storage = Arc::new(MockStorage::new());
    let runtime = Runtime::with_context(storage.clone(), context);
    
    // Create a test receipt
    let receipt = RuntimeExecutionReceipt {
        id: "test-receipt-2".into(),
        issuer: identity_did,
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
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        dag_epoch: Some(42),
        receipt_cid: None,
        signature: None,
    };
    
    // Anchor the receipt (which should try to trigger reputation update but fail)
    // The method should still succeed because reputation failure is non-fatal
    let receipt_cid = runtime.anchor_receipt(&receipt).await?;
    
    // Verify the receipt was still anchored despite reputation failure
    let storage_ref = storage.as_ref() as &MockStorage;
    let anchored_data = storage_ref.get_anchored_data(&receipt_cid)
        .expect("Receipt should be anchored even when reputation update fails");
    
    // Parse the anchored receipt
    let anchored_receipt: RuntimeExecutionReceipt = serde_json::from_str(&anchored_data)?;
    assert_eq!(anchored_receipt.id, "test-receipt-2");
    
    // Verify the mock reputation service was called
    mock_server.assert();
    
    Ok(())
} 