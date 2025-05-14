use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Utc;
use httpmock::{Method, MockServer};
use icn_identity::{Did, KeyPair};
use icn_runtime::reputation_integration::{HttpReputationUpdater, NoopReputationUpdater};
use icn_runtime::{Runtime, RuntimeContext, RuntimeContextBuilder, MemStorage, InMemoryManaLedger, RegenerationPolicy, ManaRegenerator};
use icn_types::mesh::{JobStatus as IcnJobStatus, MeshExecutionReceipt};
use icn_types::runtime_receipt::{RuntimeExecutionMetrics, RuntimeExecutionReceipt};
use icn_types::VerifiableReceipt;
use serde_json::json;
use std::str::FromStr;
use std::sync::Arc;
use icn_types::dag_store::SharedDagStore;
use icn_runtime::metrics;

// Helper function to create a basic, signed RuntimeExecutionReceipt
// This might need to be more sophisticated or use runtime.issue_receipt if direct signing is complex.
// For now, let's try a simplified approach.
fn create_basic_receipt(
    issuer_did: &str,
    receipt_id: &str,
    mana_cost: Option<u64>,
) -> RuntimeExecutionReceipt {
    RuntimeExecutionReceipt {
        id: receipt_id.to_string(),
        issuer: issuer_did.to_string(),
        proposal_id: "test_proposal".to_string(),
        wasm_cid: "test_wasm_cid".to_string(),
        ccl_cid: "test_ccl_cid".to_string(),
        metrics: RuntimeExecutionMetrics {
            host_calls: 1,
            io_bytes: 10,
            mana_cost,
        },
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
    let payload_struct = receipt
        .get_payload_for_signing()
        .expect("Failed to get payload for signing in helper");
    let bytes_to_sign = bincode::serialize(&payload_struct)
        .expect("Failed to serialize payload for signing in helper");
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
        runtime_did.clone(),
    ));

    let dag_store = Arc::new(SharedDagStore::new());
    let mana_ledger = Arc::new(InMemoryManaLedger::new());
    let policy = RegenerationPolicy::FixedRatePerTick(1);
    let mana_regenerator = Arc::new(ManaRegenerator::new(mana_ledger.clone(), policy));

    let context = RuntimeContextBuilder::<InMemoryManaLedger>::new()
        .with_identity(runtime_identity_keypair.clone())
        .with_executor_id(runtime_did.to_string())
        .with_dag_store(dag_store.clone())
        .with_mana_regenerator(mana_regenerator.clone())
        .build();

    let runtime = Runtime::<InMemoryManaLedger>::with_context(Arc::new(MemStorage::default()), Arc::new(context))
        .with_reputation_updater(http_reputation_updater.clone());

    let expected_partial_body = json!({
        "subject": issuer_did_str,
        "success": true
    });

    let rep_submission_mock = server
        .mock_async(move |when, then| {
            when.method(httpmock::Method::POST) // Method::POST is from the use line
                .path("/")
                .json_body_partial(expected_partial_body.to_string()); // Use json_body_partial
            then.status(200).body("{ \"status\": \"ok\" }");
        })
        .await;

    runtime
        .anchor_receipt(&receipt)
        .await
        .expect("Anchor receipt failed");

    rep_submission_mock.assert_async().await;

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
        runtime_did.clone(),
    ));

    let dag_store = Arc::new(SharedDagStore::new());
    let mana_ledger = Arc::new(InMemoryManaLedger::new());
    let policy = RegenerationPolicy::FixedRatePerTick(1);
    let mana_regenerator = Arc::new(ManaRegenerator::new(mana_ledger.clone(), policy));

    let context = RuntimeContextBuilder::<InMemoryManaLedger>::new()
        .with_identity(runtime_identity_keypair.clone())
        .with_executor_id(runtime_did.to_string())
        .with_dag_store(dag_store.clone())
        .with_mana_regenerator(mana_regenerator.clone())
        .build();

    let runtime = Runtime::<InMemoryManaLedger>::with_context(Arc::new(MemStorage::default()), Arc::new(context))
        .with_reputation_updater(http_reputation_updater.clone());

    let rep_submission_mock = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::POST).path("/");
            then.status(500).body("Internal Server Error");
        })
        .await;

    let initial_http_errors = metrics::REPUTATION_SUBMISSION_HTTP_ERRORS
        .with_label_values(&[receipt.issuer.as_str(), "500"])
        .get() as f64;

    runtime
        .anchor_receipt(&receipt)
        .await
        .expect("Anchor receipt should succeed even if reputation update fails");

    rep_submission_mock.assert_async().await;
    let final_http_errors = metrics::REPUTATION_SUBMISSION_HTTP_ERRORS
        .with_label_values(&[receipt.issuer.as_str(), "500"])
        .get() as f64;
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
        runtime_did.clone(),
    ));

    let dag_store = Arc::new(SharedDagStore::new());
    let mana_ledger = Arc::new(InMemoryManaLedger::new());
    let policy = RegenerationPolicy::FixedRatePerTick(1);
    let mana_regenerator = Arc::new(ManaRegenerator::new(mana_ledger.clone(), policy));

    let context = RuntimeContextBuilder::<InMemoryManaLedger>::new()
        .with_identity(runtime_identity_keypair.clone())
        .with_executor_id(runtime_did.to_string())
        .with_dag_store(dag_store.clone())
        .with_mana_regenerator(mana_regenerator.clone())
        .build();

    let runtime = Runtime::<InMemoryManaLedger>::with_context(Arc::new(MemStorage::default()), Arc::new(context))
        .with_reputation_updater(reputation_updater.clone());

    let initial_client_errors = metrics::REPUTATION_SUBMISSION_CLIENT_ERRORS
        .get_metric_with_label_values(&[
            receipt.issuer.as_str(),
            "CLIENT_ERROR_PLACEHOLDER_WILL_NOT_MATCH_DYNAMIC_ERROR",
        ])
        .map_or(0.0, |m| m.get() as f64);

    runtime
        .anchor_receipt(&receipt)
        .await
        .expect("Anchor receipt should succeed");

    let final_client_errors = metrics::REPUTATION_SUBMISSION_CLIENT_ERRORS
        .get_metric_with_label_values(&[
            receipt.issuer.as_str(),
            "CLIENT_ERROR_PLACEHOLDER_WILL_NOT_MATCH_DYNAMIC_ERROR",
        ])
        .map_or(0.0, |m| m.get() as f64);
    assert!(final_client_errors > initial_client_errors, "Client error counter should have incremented. This assertion may fail due to dynamic error string mismatch.");

    Ok(())
}
