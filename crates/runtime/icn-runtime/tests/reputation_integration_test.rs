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
use crate::metrics;
use crate::reputation_integration::{HttpReputationUpdater, ReputationScoringConfig};
use icn_types::reputation::ReputationRecord;
use httpmock::prelude::*;

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
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/reputation/records");
        then.status(200);
    });
    let did = Did::from_str("did:key:z6MkpTHR8VNsESGeQGSwQy1VBCLeP2g2rM86Zbf3pt12345")?;
    let receipt = RuntimeExecutionReceipt {
        id: "test-receipt".into(),
        issuer: did.to_string(),
        proposal_id: "test-proposal".into(),
        wasm_cid: "test-wasm".into(),
        ccl_cid: "test-ccl".into(),
        metrics: RuntimeExecutionMetrics {
            host_calls: 10,
            io_bytes: 1024,
            mana_cost: Some(500), // Corrected: Added mana_cost
        },
        anchored_cids: vec!["cid1".into()],
        resource_usage: vec![("cpu".into(), 100)],
        timestamp: Utc::now().timestamp() as u64,
        dag_epoch: Some(1), // Corrected: Added dag_epoch
        receipt_cid: Some("receipt-cid-123".into()),
        signature: Some(vec![1, 2, 3]),
    };
    let updater = HttpReputationUpdater::new(server.url(""), did.clone());
    updater.submit_receipt_based_reputation(&receipt, true).await?;
    mock.assert_hits(1);
    Ok(())
}

#[tokio::test]
async fn test_reputation_updater_handles_http_500() -> Result<()> {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/reputation/records");
        then.status(500).body("Internal Server Error");
    });
    let did = Did::from_str("did:key:z6MkpTHR8VNsESGeQGSwQy1VBCLeP2g2rM86Zbf3pt12345")?;
    let receipt = RuntimeExecutionReceipt {
        id: "test-receipt-500".into(),
        issuer: did.to_string(),
        proposal_id: "test-proposal-500".into(),
        wasm_cid: "test-wasm-500".into(),
        ccl_cid: "test-ccl-500".into(),
        metrics: RuntimeExecutionMetrics {
            host_calls: 5,
            io_bytes: 512,
            mana_cost: Some(250), // Corrected: Added mana_cost
        },
        anchored_cids: vec![],
        resource_usage: vec![],
        timestamp: Utc::now().timestamp() as u64,
        dag_epoch: Some(2), // Corrected: Added dag_epoch
        receipt_cid: Some("receipt-cid-500".into()),
        signature: None,
    };
    let updater = HttpReputationUpdater::new(server.url(""), did.clone());
    let result = updater.submit_receipt_based_reputation(&receipt, true).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Failed to submit reputation record: 500"));
    mock.assert_hits(1);
    Ok(())
}

#[tokio::test]
async fn test_noop_reputation_updater_ignores_submission() -> Result<()> {
    let updater = icn_runtime::reputation_integration::NoopReputationUpdater;
    let did = Did::from_str("did:key:z6MkpTHR8VNsESGeQGSwQy1VBCLeP2g2rM86Zbf3pt12345")?;
    let receipt = RuntimeExecutionReceipt {
       id: "test-receipt-noop".into(),
       issuer: did.to_string(),
       proposal_id: "test-proposal-noop".into(),
       wasm_cid: "test-wasm-noop".into(),
       ccl_cid: "test-ccl-noop".into(),
       metrics: RuntimeExecutionMetrics {
           host_calls: 1,
           io_bytes: 1,
           mana_cost: Some(10), // Corrected: Added mana_cost
       },
       anchored_cids: vec![],
       resource_usage: vec![],
       timestamp: Utc::now().timestamp() as u64,
       dag_epoch: Some(3), // Corrected: Added dag_epoch
       receipt_cid: Some("receipt-cid-noop".into()),
       signature: None,
   };
   let result = updater.submit_receipt_based_reputation(&receipt, true).await;
   assert!(result.is_ok());
   // No mock server to assert hits against
   Ok(())
}

#[tokio::test]
async fn test_http_reputation_updater_submits_correct_payload() -> Result<()> {
    let server = MockServer::start();
    let expected_subject = "did:key:z6MkpTHR8VNsESGeQGSwQy1VBCLeP2g2rM86Zbf3pt12345".to_string();
    let expected_anchor = "cid-abc123".to_string();
    let expected_mana_cost = Some(1000); // Cost that likely won't hit the cap
    let expected_timestamp = 1_700_000_000;
    
    let config = ReputationScoringConfig {
        mana_cost_weight: 100.0,
        failure_penalty: -25.0,
        max_positive_score: 5.0, // Set a cap for the test
    };

    let raw_score = config.mana_cost_weight / expected_mana_cost.unwrap() as f64;
    let expected_score_delta = raw_score.min(config.max_positive_score);

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/reputation/records")
            .json_body_partial(serde_json::json!({
                "subject": expected_subject,
                "anchor": expected_anchor,
                "mana_cost": expected_mana_cost,
                "score_delta": expected_score_delta, // Will be 0.1, not capped
                "timestamp": expected_timestamp,
                "success": true
            }));
        then.status(200);
    });

    let receipt = RuntimeExecutionReceipt {
        issuer: expected_subject.clone(),
        proposal_id: "prop-1".to_string(),
        wasm_cid: "wasm-1".to_string(),
        ccl_cid: "ccl-1".to_string(),
        anchored_cids: vec![],
        metrics: RuntimeExecutionMetrics {
            host_calls: 5,
            io_bytes: 2048,
            mana_cost: expected_mana_cost,
        },
        resource_usage: vec![],
        timestamp: expected_timestamp,
        receipt_cid: Some(expected_anchor.clone()),
        signature: Some(vec![0u8; 64]),
        id: "receipt-id-123".to_string(),
        dag_epoch: Some(4),
    };

    let updater = HttpReputationUpdater::new_with_config(
        server.url(""),
        Did::from_str(&expected_subject)?,
        config.clone(), // Clone config for this updater
    );
    updater.submit_receipt_based_reputation(&receipt, true).await?;
    mock.assert_hits(1);
    Ok(())
}

#[tokio::test]
async fn test_http_reputation_updater_score_capping() -> Result<()> {
    let server = MockServer::start();
    let subject = "did:key:z6MkpTHR8VNsESGeQGSwQy1VBCLeP2g2rM86Zbf3pt12345".to_string();
    let anchor = "cid-cap-test".to_string();
    let timestamp = 1_700_000_002;
    
    // Config with a specific cap
    let config_for_capping = ReputationScoringConfig {
        mana_cost_weight: 100.0,      // Same weight
        failure_penalty: -25.0,
        max_positive_score: 2.0,      // Lower cap to ensure it's hit
    };

    // Mana cost low enough that raw_score (100.0 / 10.0 = 10.0) would exceed max_positive_score (2.0)
    let mana_cost_for_capping = Some(10);
    let expected_capped_score_delta = config_for_capping.max_positive_score; // Should be 2.0

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/reputation/records")
            .json_body_partial(serde_json::json!({
                "subject": subject,
                "anchor": anchor,
                "mana_cost": mana_cost_for_capping,
                "score_delta": expected_capped_score_delta, // Expect the capped score
                "timestamp": timestamp,
                "success": true
            }));
        then.status(200);
    });

    let receipt = RuntimeExecutionReceipt {
        issuer: subject.clone(),
        proposal_id: "prop-cap".to_string(),
        wasm_cid: "wasm-cap".to_string(),
        ccl_cid: "ccl-cap".to_string(),
        anchored_cids: vec![],
        metrics: RuntimeExecutionMetrics {
            host_calls: 1,
            io_bytes: 128,
            mana_cost: mana_cost_for_capping,
        },
        resource_usage: vec![],
        timestamp,
        receipt_cid: Some(anchor.clone()),
        signature: Some(vec![0u8; 64]),
        id: "receipt-cap-id".to_string(),
        dag_epoch: Some(6),
    };

    let updater = HttpReputationUpdater::new_with_config(
        server.url(""),
        Did::from_str(&subject)?,
        config_for_capping, // Use the specific config for this test
    );
    updater.submit_receipt_based_reputation(&receipt, true).await?;
    mock.assert_hits(1);
    Ok(())
}

#[tokio::test]
async fn test_http_reputation_updater_submits_failure_penalty() -> Result<()> {
    let server = MockServer::start();
    let subject = "did:key:z6MkpTHR8VNsESGeQGSwQy1VBCLeP2g2rM86Zbf3pt12345".to_string();
    let anchor = "cid-fail-xyz".to_string();
    let timestamp = 1_700_000_001;
    let config = ReputationScoringConfig {
        mana_cost_weight: 100.0,
        failure_penalty: -25.0,
    };
    let expected_score_delta_on_fail = config.failure_penalty;
    let expected_success_status = false;

    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/reputation/records")
            .json_body_partial(serde_json::json!({
                "subject": subject,
                "anchor": anchor,
                "mana_cost": Some(1000),
                "score_delta": expected_score_delta_on_fail,
                "timestamp": timestamp,
                "success": expected_success_status
            }));
        then.status(200);
    });
    let receipt = RuntimeExecutionReceipt {
        issuer: subject.clone(),
        proposal_id: "prop-fail".to_string(),
        wasm_cid: "wasm-fail".to_string(),
        ccl_cid: "ccl-fail".to_string(),
        anchored_cids: vec![],
        metrics: RuntimeExecutionMetrics {
            host_calls: 2,
            io_bytes: 512,
            mana_cost: Some(1000), // Corrected: Added mana_cost
        },
        resource_usage: vec![],
        timestamp,
        receipt_cid: Some(anchor.clone()),
        signature: Some(vec![0u8; 64]),
        id: "receipt-fail-id".to_string(),
        dag_epoch: Some(5), // Corrected: Added dag_epoch
    };
    let updater = HttpReputationUpdater::new_with_config(
        server.url(""),
        Did::from_str(&subject)?,
        config,
    );
    
    // Pass is_successful = false
    let _result = updater.submit_receipt_based_reputation(&receipt, false).await;

    // Now the mock assertion should pass because the implementation uses the parameter
    mock.assert_hits(1); 

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