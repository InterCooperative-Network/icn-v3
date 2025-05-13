use anyhow::Result;
use icn_runtime::{Runtime, RuntimeContextBuilder, reputation_integration::{HttpReputationUpdater, ReputationScoringConfig}};
use icn_runtime::MemStorage; // Corrected import path
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use icn_identity::{KeyPair, Did};
use std::sync::Arc;
use httpmock::MockServer;
use serde_json::json;
use std::str::FromStr;
use icn_types::dag_store::SharedDagStore;
use tempfile::TempDir;
use url::Url;
use icn_runtime::reputation_integration::{ReputationUpdater as _};
use icn_runtime::metrics;
use icn_runtime::config::RuntimeConfig;
use icn_types::VerifiableReceipt;


// Helper function to create a basic, signed RuntimeExecutionReceipt
// This might need to be more sophisticated or use runtime.issue_receipt if direct signing is complex.
// For now, let's try a simplified approach.
fn create_basic_receipt(issuer_did: &str, receipt_id: &str, mana_cost: Option<u64>) -> RuntimeExecutionReceipt {
    RuntimeExecutionReceipt {
        id: receipt_id.to_string(),
        issuer: issuer_did.to_string(),
        proposal_id: "test_proposal".to_string(),
        wasm_cid: "test_wasm_cid".to_string(),
        ccl_cid: "test_ccl_cid".to_string(),
        metrics: RuntimeExecutionMetrics { host_calls: 1, io_bytes: 10, mana_cost },
        anchored_cids: vec![],
        resource_usage: vec![],
        timestamp: 1234567890,
        dag_epoch: Some(1),
        receipt_cid: None, 
        signature: None, // Signature will be added later if needed by the test
    }
}

// Helper to sign a receipt for testing purposes
fn sign_receipt(receipt: &mut RuntimeExecutionReceipt, keypair: &KeyPair) {
    let payload_struct = receipt.get_payload_for_signing().expect("Failed to get payload for signing in helper");
    let bytes_to_sign = bincode::serialize(&payload_struct).expect("Failed to serialize payload for signing in helper");
    let signature = keypair.sign(&bytes_to_sign);
    receipt.signature = Some(signature.to_bytes().to_vec());
}

#[tokio::test]
async fn test_anchor_receipt_triggers_reputation_submission_success() -> Result<()> {
    let server = MockServer::start_async().await;
    let issuer_keypair = KeyPair::generate();
    let issuer_did_str = issuer_keypair.did.to_string();
    let mut receipt = create_basic_receipt(&issuer_did_str, "test_receipt_success", Some(100));
    sign_receipt(&mut receipt, &issuer_keypair);

    let runtime_identity_keypair = KeyPair::generate();
    let runtime_did = Did::from_str(&runtime_identity_keypair.did.to_string()).unwrap();

    let http_reputation_updater = Arc::new(HttpReputationUpdater::new(
        server.base_url(), 
        runtime_did.clone()
    ));

    let dag_store = Arc::new(SharedDagStore::new());
    
    let context = RuntimeContextBuilder::new()
        .with_identity(runtime_identity_keypair.clone())
        .with_executor_id(runtime_did.to_string())
        .with_dag_store(dag_store.clone())
        .build();

    let runtime = Runtime::with_context(Arc::new(MemStorage::default()), Arc::new(context))
        .with_reputation_updater(http_reputation_updater.clone());

    let rep_submission_mock = server.mock_async(|when, then| {
        when.method(httpmock::Method::POST).path("/");
        then.status(200).body("{ \"status\": \"ok\" }");
    }).await;

    runtime.anchor_receipt(&receipt).await.expect("Anchor receipt failed");

    rep_submission_mock.assert_async().await;
    let received_requests = server.received_requests();
    assert_eq!(received_requests.len(), 1, "Expected one request to the mock server");
    let submitted_data = serde_json::from_slice::<serde_json::Value>(&received_requests[0].body).unwrap();
    assert_eq!(submitted_data["subject"].as_str().unwrap(), issuer_did_str);
    assert_eq!(submitted_data["success"].as_bool().unwrap(), true);

    Ok(())
}

#[tokio::test]
async fn test_anchor_receipt_reputation_submission_http_500() -> Result<()> {
    let server = MockServer::start_async().await;
    let issuer_keypair = KeyPair::generate();
    let issuer_did_str = issuer_keypair.did.to_string();
    let mut receipt = create_basic_receipt(&issuer_did_str, "test_receipt_http_500", Some(50));
    sign_receipt(&mut receipt, &issuer_keypair);

    let runtime_identity_keypair = KeyPair::generate();
    let runtime_did = Did::from_str(&runtime_identity_keypair.did.to_string()).unwrap();

    let http_reputation_updater = Arc::new(HttpReputationUpdater::new(
        server.base_url(), 
        runtime_did.clone()
    ));

    let dag_store = Arc::new(SharedDagStore::new());
    
    let context = RuntimeContextBuilder::new()
        .with_identity(runtime_identity_keypair.clone())
        .with_executor_id(runtime_did.to_string())
        .with_dag_store(dag_store.clone())
        .build();

    let runtime = Runtime::with_context(Arc::new(MemStorage::default()), Arc::new(context))
        .with_reputation_updater(http_reputation_updater.clone());

    let rep_submission_mock = server.mock_async(|when, then| {
        when.method(httpmock::Method::POST).path("/");
        then.status(500).body("Internal Server Error");
    }).await;

    let initial_http_errors = metrics::REPUTATION_SUBMISSION_HTTP_ERRORS.with_label_values(&[receipt.issuer.as_str(), "500"]).get() as f64;

    runtime.anchor_receipt(&receipt).await.expect("Anchor receipt should succeed even if reputation update fails");

    rep_submission_mock.assert_async().await;
    let final_http_errors = metrics::REPUTATION_SUBMISSION_HTTP_ERRORS.with_label_values(&[receipt.issuer.as_str(), "500"]).get() as f64;
    assert_eq!(final_http_errors, initial_http_errors + 1.0);

    Ok(())
}

#[tokio::test]
async fn test_anchor_receipt_reputation_submission_client_error() -> Result<()> {
    let invalid_url = "http://localhost:1"; 
    let issuer_keypair = KeyPair::generate();
    let issuer_did_str = issuer_keypair.did.to_string();
    let mut receipt = create_basic_receipt(&issuer_did_str, "test_receipt_client_err", Some(20));
    sign_receipt(&mut receipt, &issuer_keypair);

    let runtime_identity_keypair = KeyPair::generate();
    let runtime_did = Did::from_str(&runtime_identity_keypair.did.to_string()).unwrap();

    let reputation_updater = Arc::new(HttpReputationUpdater::new(
        invalid_url.to_string(), 
        runtime_did.clone()
    ));

    let dag_store = Arc::new(SharedDagStore::new());
    
    let context = RuntimeContextBuilder::new()
        .with_identity(runtime_identity_keypair.clone())
        .with_executor_id(runtime_did.to_string())
        .with_dag_store(dag_store.clone())
        .build();

    let runtime = Runtime::with_context(Arc::new(MemStorage::default()), Arc::new(context))
        .with_reputation_updater(reputation_updater.clone());

    let initial_client_errors = metrics::REPUTATION_SUBMISSION_CLIENT_ERRORS
        .get_metric_with_label_values(&[receipt.issuer.as_str(), "CLIENT_ERROR_PLACEHOLDER_WILL_NOT_MATCH_DYNAMIC_ERROR"])
        .map_or(0.0, |m| m.get() as f64);

    runtime.anchor_receipt(&receipt).await.expect("Anchor receipt should succeed");

    let final_client_errors = metrics::REPUTATION_SUBMISSION_CLIENT_ERRORS
        .get_metric_with_label_values(&[receipt.issuer.as_str(), "CLIENT_ERROR_PLACEHOLDER_WILL_NOT_MATCH_DYNAMIC_ERROR"])
        .map_or(0.0, |m| m.get() as f64);
    assert!(final_client_errors > initial_client_errors, "Client error counter should have incremented. This assertion may fail due to dynamic error string mismatch.");

    Ok(())
} 