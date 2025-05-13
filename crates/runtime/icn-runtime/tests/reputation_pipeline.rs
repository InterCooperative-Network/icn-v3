use anyhow::Result;
use icn_runtime::{Runtime, RuntimeContextBuilder, reputation_integration::{HttpReputationUpdater, ReputationScoringConfig}};
use icn_runtime::storage::MemStorage; // Using MemStorage for simplicity in tests
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use icn_identity::{KeyPair, Did};
use std::sync::Arc;
use httpmock::MockServer;
use serde_json::json;
use std::str::FromStr;
use icn_types::dag_store::SharedDagStore;


// Helper function to create a basic, signed RuntimeExecutionReceipt
// This might need to be more sophisticated or use runtime.issue_receipt if direct signing is complex.
// For now, let's try a simplified approach.
fn create_signed_dummy_receipt(issuer_keypair: &KeyPair, issuer_did_str: &str) -> RuntimeExecutionReceipt {
    let mut receipt = RuntimeExecutionReceipt {
        id: uuid::Uuid::new_v4().to_string(),
        issuer: issuer_did_str.to_string(),
        proposal_id: "test-proposal-id".to_string(),
        wasm_cid: "test-wasm-cid".to_string(),
        ccl_cid: "test-ccl-cid".to_string(),
        metrics: RuntimeExecutionMetrics {
            host_calls: 1,
            io_bytes: 10,
            mana_cost: Some(100),
        },
        anchored_cids: vec![],
        resource_usage: vec![],
        timestamp: chrono::Utc::now().timestamp() as u64,
        dag_epoch: Some(1),
        receipt_cid: None, // Will be set by anchor_receipt logic if successful
        signature: None, // Will be signed below
    };

    // Simplified signing - in a real scenario, runtime.issue_receipt handles this.
    // For this test, we need a VerifiableReceipt to pass anchor_receipt's internal checks.
    // This requires the receipt to be signed.
    let payload = receipt.signed_payload().expect("Failed to get signed payload");
    let signature_bytes = issuer_keypair.sign(&payload).expect("Failed to sign payload");
    receipt.signature = Some(signature_bytes.to_bytes().to_vec());
    receipt
}


#[tokio::test]
async fn test_anchor_receipt_triggers_reputation_submission_success() -> Result<()> {
    // 1. Setup MockServer for the reputation service
    let reputation_server = MockServer::start();

    // 2. Prepare Runtime components
    let storage = Arc::new(MemStorage::new());
    let dag_store = Arc::new(SharedDagStore::new());
    let issuer_keypair = KeyPair::generate();
    let issuer_did = issuer_keypair.did.clone();
    let issuer_did_str = issuer_did.to_string();
    
    let reputation_service_url = reputation_server.base_url();
    let rep_scoring_config = ReputationScoringConfig::default();

    // Create HttpReputationUpdater configured with the mock server
    let http_reputation_updater = Arc::new(HttpReputationUpdater::new_with_config(
        reputation_service_url.clone(),
        issuer_did.clone(), // The DID of the runtime/updater itself
        rep_scoring_config
    ));

    // Build RuntimeContext, providing the HttpReputationUpdater
    let context = RuntimeContextBuilder::new()
        .with_identity(issuer_keypair.clone()) // Runtime's own identity
        .with_executor_id(issuer_did_str.clone()) // Also runtime's DID for this context
        .with_dag_store(dag_store.clone())
        .with_reputation_updater(http_reputation_updater.clone()) // Inject our mock-targeted updater
        .build();

    let runtime = Runtime::with_context(storage, Arc::new(context));

    // 3. Create a signed RuntimeExecutionReceipt
    // The receipt issuer for reputation purposes is the 'executor_did' who "did the work".
    // In this test, we'll have a separate keypair for the receipt's issuer.
    let receipt_issuer_keypair = KeyPair::generate();
    let receipt_issuer_did_str = receipt_issuer_keypair.did.to_string();
    let mut receipt_to_anchor = create_signed_dummy_receipt(&receipt_issuer_keypair, &receipt_issuer_did_str);
    
    // anchor_receipt expects the receipt's CID to be None initially, it calculates it.
    // It also populates receipt.receipt_cid with its own CID.
    // The current create_signed_dummy_receipt sets signature but not receipt_cid, which is correct.

    // 4. Mock the expected HTTP POST to the reputation service
    let rep_submission_mock = reputation_server.mock(|when, then| {
        when.method(httpmock::Method::POST)
            .path("/")
            .header("content-type", "application/json");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({ "status": "ok" }));
    });

    // 5. Call runtime.anchor_receipt(...)
    let anchor_result = runtime.anchor_receipt(&receipt_to_anchor).await;

    // 6. Assertions
    assert!(anchor_result.is_ok(), "anchor_receipt failed: {:?}", anchor_result.err());
    
    // Verify that the reputation service (mock) was called
    rep_submission_mock.assert();

    // Optional: More detailed assertions on the submitted ReputationRecord if needed,
    // by capturing the request body in the mock.
    let submitted_data = rep_submission_mock.requests()[0].body_json::<serde_json::Value>().unwrap();
    assert_eq!(submitted_data["subject"], receipt_issuer_did_str);
    assert_eq!(submitted_data["success"], true); // because is_successful is true by default in HttpReputationUpdater if not an error state
                                                 // And we are calling anchor_receipt directly not from a failed job context.
                                                 // The `is_successful` in HttpReputationUpdater::submit_receipt_based_reputation
                                                 // is passed from `anchor_receipt`, which passes `true`.

    Ok(())
} 