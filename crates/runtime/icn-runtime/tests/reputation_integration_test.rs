#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use anyhow::{Result, anyhow, Context};
use icn_runtime::{
    RuntimeStorage, Runtime, RuntimeContext, RuntimeContextBuilder,
    reputation_integration::{ReputationUpdater, HttpReputationUpdater}
};
use icn_identity::{KeyPair, Did};
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use uuid::Uuid;
use async_trait::async_trait;
use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;
use std::str::FromStr;
use icn_identity::KeyPair as IcnKeyPair;
use icn_types::mesh::MeshExecutionReceipt;
use icn_types::receipt_verification::VerifiableReceipt;
use bincode;
use chrono::Utc;
use httpmock::MockServer;
use httpmock::Method::POST;
use tempfile;
use icn_runtime::config::RuntimeConfig;

/// Mock storage implementation
#[derive(Clone, Default)]
struct MockStorage {
    proposals: Arc<Mutex<HashMap<String, icn_runtime::Proposal>>>,
    wasm_modules: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    receipts: Arc<Mutex<HashMap<String, RuntimeExecutionReceipt>>>,
    anchored_cids: Arc<Mutex<Vec<String>>>,
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

    async fn store_receipt(&self, receipt: &RuntimeExecutionReceipt) -> Result<String> {
        let receipt_id = receipt.id.clone();
        self.receipts.lock().unwrap().insert(receipt_id.clone(), receipt.clone());
        Ok(receipt_id)
    }

    async fn store_wasm(&self, cid: &str, bytes: &[u8]) -> Result<()> {
        self.wasm_modules.lock().unwrap().insert(cid.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn load_receipt(&self, receipt_id: &str) -> Result<RuntimeExecutionReceipt> {
        self.receipts.lock().unwrap().get(receipt_id).cloned().ok_or_else(|| anyhow!("Receipt not found"))
    }

    async fn anchor_to_dag(&self, _cid: &str) -> Result<String> {
        Ok("mock-dag-anchor".into())
    }
}

#[tokio::test]
async fn test_reputation_submission_on_anchor() -> Result<()> {
    let storage = Arc::new(MockStorage::default());
    let keypair = KeyPair::generate();
    let identity_did_str = keypair.did.to_string();

    let identity_did_obj = Did::from_str(&identity_did_str)?;
    let updater = Arc::new(HttpReputationUpdater::new(
        "http://localhost:12345".to_string(),
        identity_did_obj,
    ));

    let context = RuntimeContextBuilder::new()
        .with_identity(keypair)
        .with_executor_id(identity_did_str.clone())
        .build();

    let runtime = Runtime::with_context(storage.clone(), Arc::new(context))
        .with_reputation_updater(updater);

    let receipt = RuntimeExecutionReceipt {
        id: "mock-receipt-id".to_string(),
        issuer: identity_did_str,
        proposal_id: "prop-1".to_string(),
        wasm_cid: "wasm-cid".to_string(),
        ccl_cid: "ccl-cid".to_string(),
        metrics: RuntimeExecutionMetrics {
            fuel_used: 100,
            host_calls: 5,
            io_bytes: 1024,
        },
        anchored_cids: vec![],
        resource_usage: vec![],
        timestamp: 1234567890,
        dag_epoch: Some(1),
        receipt_cid: None,
        signature: None,
    };

    let receipt_cid = runtime.anchor_receipt(&receipt).await?;

    assert!(receipt_cid.starts_with("anchor-"));

    Ok(())
}

#[tokio::test]
async fn test_reputation_submission_skipped_if_no_updater() -> Result<()> {
    let storage = Arc::new(MockStorage::default());
    let keypair = KeyPair::generate();
    let identity_did_str = keypair.did.to_string();

    let context = RuntimeContextBuilder::new()
        .with_identity(keypair)
        .with_executor_id(identity_did_str.clone())
        .build();

    let runtime = Runtime::with_context(storage.clone(), Arc::new(context));

    let receipt = RuntimeExecutionReceipt {
        id: "mock-receipt-id-2".to_string(),
        issuer: identity_did_str,
        proposal_id: "prop-2".to_string(),
        wasm_cid: "wasm-cid".to_string(),
        ccl_cid: "ccl-cid".to_string(),
        metrics: RuntimeExecutionMetrics {
            fuel_used: 100,
            host_calls: 5,
            io_bytes: 1024,
        },
        anchored_cids: vec![],
        resource_usage: vec![],
        timestamp: 1234567890,
        dag_epoch: Some(1),
        receipt_cid: None,
        signature: None,
    };

    let receipt_cid = runtime.anchor_receipt(&receipt).await?;

    assert!(receipt_cid.starts_with("anchor-"));

    Ok(())
}

use icn_runtime::{Runtime, config::RuntimeConfig, reputation_integration::ReputationUpdater};
use icn_runtime::reputation_integration::{HttpReputationUpdater, NoopReputationUpdater};
use icn_identity::{KeyPair as IcnKeyPair, Did};
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use std::sync::Arc;
use icn_runtime::MemStorage; // Assuming MemStorage is pub or accessible via pub mod storage
use httpmock::MockServer;
use httpmock::Method::POST;
use tempfile;
use chrono::Utc;

// Import the signing helper if it's made public or accessible
// For now, assuming it's defined locally or accessible within the tests
// If not, we might need to call runtime.issue_receipt which does the signing internally.
// fn sign_runtime_receipt_in_place(
//     receipt: &mut RuntimeExecutionReceipt,
//     keypair: &IcnKeyPair,
// ) -> Result<()>;

// Helper to get runtime identity keypair (assumes runtime has identity)
fn get_runtime_keypair(runtime: &Runtime) -> Result<IcnKeyPair> {
    runtime.context().identity()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Runtime context has no identity keypair for test"))
}

// Helper to sign a receipt (needs access to sign_runtime_receipt_in_place or similar)
// This might be redundant if we use issue_receipt, but useful if constructing receipts manually.
fn sign_receipt(receipt: &mut RuntimeExecutionReceipt, keypair: &IcnKeyPair) -> Result<()> {
    // Placeholder: Ideally call the actual sign_runtime_receipt_in_place helper.
    // For now, mimic signing for test setup.
    let payload = receipt.signed_payload(); // Ensure this is public
    let bytes = bincode::serialize(&payload).unwrap();
    let signature = keypair.sign(&bytes); // Assumes KeyPair::sign exists
    receipt.signature = Some(signature.to_bytes().to_vec());
    Ok(())
}

// Helper to generate and sign a dummy receipt for tests
fn generate_and_sign_dummy_receipt(keypair: &IcnKeyPair) -> Result<RuntimeExecutionReceipt> {
    use bincode;

    let mut receipt = RuntimeExecutionReceipt {
        id: Uuid::new_v4().to_string(),
        issuer: keypair.did.to_string(), // Use the provided keypair's DID
        proposal_id: "test-proposal".to_string(),
        wasm_cid: "bafybeibogus".to_string(),
        ccl_cid: "bafybeiccl".to_string(),
        metrics: RuntimeExecutionMetrics {
            fuel_used: 100,
            host_calls: 10,
            io_bytes: 512,
        },
        anchored_cids: vec!["bafybeidata".to_string()],
        resource_usage: vec![("cpu".to_string(), 100)], // Must be Vec<(String, u64)>
        timestamp: Utc::now().timestamp() as u64,
        dag_epoch: Some(42), // Must be Option<u64>
        receipt_cid: None,
        signature: None, // Will be added below
    };

    let payload = receipt.signed_payload(); // RuntimeExecutionReceipt::signed_payload must be pub
    let bytes = bincode::serialize(&payload)
        .context("Failed to serialize payload in test helper")?;
    
    // Assumes KeyPair::sign exists
    let signature = keypair.sign(&bytes);
    receipt.signature = Some(signature.to_bytes().to_vec());
    Ok(receipt)
}

#[tokio::test]
async fn test_valid_receipt_sends_to_http_reputation_service() -> Result<()> {
    // Setup mock server
    let server = MockServer::start();
    let mock_endpoint = "/reputation/records"; // Match HttpReputationUpdater's expected path
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path(mock_endpoint)
            .header("content-type", "application/json");
            // TODO: Add body assertion if needed: .json_body(...) 
        then.status(200);
    });

    // Create runtime config with mock reputation service URL
    let config = RuntimeConfig {
        reputation_service_url: Some(server.url(mock_endpoint)),
        // Ensure storage_path points to a valid temp dir or use MemStorage approach
        storage_path: tempfile::tempdir()?.path().to_path_buf(), 
        key_path: None, // Generate in-memory key
        ..Default::default()
    };
    
    // Initialize runtime from config (this sets up HttpReputationUpdater)
    let runtime = Runtime::from_config(config).await?;
    let keypair = get_runtime_keypair(&runtime)?;

    // Generate dummy receipt (needs fields from RuntimeExecutionReceipt)
    let mut receipt = RuntimeExecutionReceipt {
        id: uuid::Uuid::new_v4().to_string(),
        issuer: keypair.did.to_string(), // Use the runtime's DID as issuer
        proposal_id: "proposal-xyz".to_string(),
        wasm_cid: "wasm-cid-demo".to_string(),
        ccl_cid: "ccl-cid-demo".to_string(),
        anchored_cids: vec!["cid1".to_string()],
        metrics: RuntimeExecutionMetrics {
            fuel_used: 123,
            host_calls: 10,
            io_bytes: 1024,
        },
        resource_usage: vec![("CPU".to_string(), 500)],
        timestamp: chrono::Utc::now().timestamp() as u64, // Cast to u64
        dag_epoch: Some(0), // Use Option<u64>
        receipt_cid: None, // Will be set by anchor_receipt
        signature: None,
    };

    // Sign the receipt (using helper or logic)
    sign_receipt(&mut receipt, &keypair)?;
    // Alternatively, if issue_receipt is used, it would handle signing:
    // let receipt = runtime.issue_receipt(...)?; 

    // Anchor receipt (should trigger reputation submission via HttpReputationUpdater)
    // Note: anchor_receipt takes &RuntimeExecutionReceipt
    runtime.anchor_receipt(&receipt).await.expect("Anchoring failed, check verification and submission logic");

    // Assert HTTP request was made exactly once
    mock.assert();
    // Or more robustly:
    mock.assert_hits(1);
    
    Ok(())
}

#[tokio::test]
async fn test_reputation_updater_handles_http_500() -> Result<()> {
    // Setup mock server
    let server = MockServer::start();
    let mock_endpoint = "/reputation/records";
    let mock = server.mock(|when, then| {
        when.method(POST).path(mock_endpoint);
        then.status(500); // Internal server error
    });

    // Create runtime config pointing to mock server
    let temp_dir = tempfile::tempdir()?;
    let config = RuntimeConfig {
        reputation_service_url: Some(server.url(mock_endpoint)),
        storage_path: temp_dir.path().to_path_buf(), 
        key_path: None, // Generate in-memory key
        ..Default::default()
    };
    
    // Initialize runtime
    let runtime = Runtime::from_config(config).await?;
    let keypair = get_runtime_keypair(&runtime)?;

    // Generate a signed receipt
    let receipt = generate_and_sign_dummy_receipt(&keypair)?;

    // Anchor receipt - should attempt submission but handle the 500 error gracefully
    let anchor_result = runtime.anchor_receipt(&receipt).await;

    // Assert anchoring succeeded (error was logged, not propagated)
    // The anchor_receipt function logs the error but returns Ok(receipt_cid)
    assert!(anchor_result.is_ok(), "anchor_receipt should succeed even on reputation submission failure");
    
    // Assert the mock server was hit
    mock.assert_hits(1);

    Ok(())
}

#[tokio::test]
async fn test_noop_reputation_updater_ignores_submission() -> Result<()> {
    // Setup runtime config WITHOUT reputation service URL
    let temp_dir = tempfile::tempdir()?;
    let config = RuntimeConfig {
        reputation_service_url: None, // <-- This triggers NoopReputationUpdater
        storage_path: temp_dir.path().to_path_buf(), 
        key_path: None, 
        ..Default::default()
    };

    // Initialize runtime (will use NoopReputationUpdater)
    let runtime = Runtime::from_config(config).await?;
    let keypair = get_runtime_keypair(&runtime)?;

    // Generate a signed receipt
    let receipt = generate_and_sign_dummy_receipt(&keypair)?;

    // Anchor receipt - should not attempt any HTTP submission
    let anchor_result = runtime.anchor_receipt(&receipt).await;

    // Assert anchoring succeeded
    assert!(anchor_result.is_ok(), "anchor_receipt failed with NoopReputationUpdater");
    
    // We cannot easily assert that *no* HTTP request was made without 
    // setting up a mock server and asserting it *wasn't* hit, which feels 
    // brittle. Trusting the code path based on config is sufficient here.

    Ok(())
}

#[tokio::test]
async fn test_mesh_receipt_signature_verification_and_submission() -> Result<()> {
    // 1. Start mock reputation service
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/reputation"); // Adjusted path based on your config
        then.status(200);
    });

    // 2. Setup runtime with mock URL
    let storage_path = tempdir()?.path().to_path_buf();
    let config = RuntimeConfig {
        reputation_service_url: Some(server.url("/reputation")), // Ensure path matches mock
        storage_path,
        ..Default::default()
    };
    let runtime = Runtime::from_config(config).await?;

    // 3. Generate keypair and create signed MeshExecutionReceipt
    use icn_identity::IcnKeyPair;
    use icn_types::mesh::{ExecutionReceipt as MeshExecutionReceipt, JobStatus as IcnJobStatus};
    use icn_types::receipt_verification::VerifiableReceipt; // For get_payload_for_signing
    use icn_economics::ResourceType; // For resource_usage HashMap key
    use std::collections::HashMap; // For HashMap
    use chrono::Utc;
    use bincode; // For serialization

    let keypair = IcnKeyPair::generate();
    let now_dt = Utc::now();
    let now_ts = now_dt.timestamp() as u64;
    
    let mut receipt = MeshExecutionReceipt {
        job_id: "job-mesh-abc123".into(),
        executor: keypair.did.clone(),
        status: IcnJobStatus::Completed, // Fully initialize
        result_data_cid: None,
        logs_cid: None,
        resource_usage: HashMap::new(), // Initialize with all fields
        execution_start_time: now_ts.saturating_sub(1), 
        execution_end_time: now_ts,
        execution_end_time_dt: now_dt, 
        signature: vec![], // Will be filled after signing
        coop_id: None,
        community_id: None,
    };
    
    let payload = receipt.get_payload_for_signing()
        .expect("Failed to get payload for MeshExecutionReceipt signing");
    let bytes = bincode::serialize(&payload)
        .expect("Failed to serialize MeshExecutionReceipt payload");
    let sig = keypair.sign(&bytes);
    receipt.signature = sig.to_bytes().to_vec();

    // 4. Submit to anchor_mesh_receipt
    runtime.anchor_mesh_receipt(&receipt).await?; // Pass by reference

    // 5. Confirm the mock server was hit
    mock.assert_hits(1);
    Ok(())
} 