#![allow(unused_imports)] // Allow unused imports for now during scaffolding
use anyhow::Result;
use icn_runtime::{
    Runtime, RuntimeContext, RuntimeContextBuilder, RuntimeStorage,
    reputation_integration::{ReputationUpdater, HttpReputationUpdater, NoopReputationUpdater}, // Assuming these are pub
    MemStorage, // Assuming MemStorage is pub or accessible
};
use icn_identity::{Did, KeyPair as IcnKeyPair};
use icn_types::{
    runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics},
    VerifiableReceipt, // For receipt.cid() and sign_receipt_in_place if used
};
use std::sync::{Arc, Mutex};
use std::str::FromStr;
use async_trait::async_trait;
use chrono::Utc;
use serde_cbor; 
use cid::Cid;

// --- Mock Reputation Updater for Mana Deduction ---

#[derive(Debug, Clone)]
struct ManaDeductionCall {
    executor_did: Did,
    amount: u64,
    coop_id: String,
    community_id: String,
}

#[derive(Clone, Debug, Default)]
struct MockManaReputationUpdater {
    mana_deductions: Arc<Mutex<Vec<ManaDeductionCall>>>,
    // We can add tracking for submit_receipt_based_reputation if needed by other tests
    // For now, focusing on mana deduction.
}

impl MockManaReputationUpdater {
    fn new() -> Self {
        Default::default()
    }

    fn get_mana_deductions(&self) -> Vec<ManaDeductionCall> {
        self.mana_deductions.lock().unwrap().clone()
    }
}

#[async_trait]
impl ReputationUpdater for MockManaReputationUpdater {
    async fn submit_receipt_based_reputation(
        &self,
        _receipt: &RuntimeExecutionReceipt,
        _is_successful: bool,
        _coop_id: &str,
        _community_id: &str,
    ) -> Result<()> {
        // No-op for this mock, or add basic logging if desired
        tracing::debug!("[MockManaReputationUpdater] submit_receipt_based_reputation called, doing nothing.");
        Ok(())
    }

    async fn submit_mana_deduction(
        &self,
        executor_did: &Did,
        amount: u64,
        coop_id: &str,
        community_id: &str,
    ) -> Result<()> {
        tracing::debug!(
            "[MockManaReputationUpdater] submit_mana_deduction called for DID: {}, Amount: {}, Coop: {}, Comm: {}",
            executor_did, amount, coop_id, community_id
        );
        self.mana_deductions.lock().unwrap().push(ManaDeductionCall {
            executor_did: executor_did.clone(),
            amount,
            coop_id: coop_id.to_string(),
            community_id: community_id.to_string(),
        });
        Ok(())
    }
}

// --- Test Helper for Runtime Setup ---

fn create_test_runtime_with_mock_updater() -> (Runtime, Arc<MockManaReputationUpdater>) {
    let storage = Arc::new(MemStorage::new());
    let mock_updater = Arc::new(MockManaReputationUpdater::new());

    // Create a default identity for the runtime itself
    let runtime_keypair = IcnKeyPair::generate();
    let runtime_did_str = runtime_keypair.did.to_string();

    let context = Arc::new(
        RuntimeContextBuilder::new()
            .with_identity(runtime_keypair) // Runtime's own identity
            .with_executor_id(runtime_did_str) // Runtime's DID as executor (can be overridden in VmContext if needed)
            .with_federation_id("test-federation-for-scope".to_string()) // Used for coop/community labels for now
            // .with_dag_store(...) // RuntimeContextBuilder might need a DagStore
            // If SharedDagStore::new() is available and takes no args:
            .with_dag_store(Arc::new(icn_types::dag_store::SharedDagStore::new()))
            .build()
    );

    let runtime = Runtime::new(storage.clone()) // Runtime::new might need adjustment if it errors
        .expect("Failed to create test runtime")
        .with_reputation_updater(mock_updater.clone() as Arc<dyn ReputationUpdater>);
    
    (runtime, mock_updater)
}

// Helper to create a signed test receipt
fn create_signed_test_receipt(issuer_did_str: &str, mana_cost: Option<u64>, keypair_for_signing: &IcnKeyPair) -> RuntimeExecutionReceipt {
    let mut receipt = RuntimeExecutionReceipt {
        id: uuid::Uuid::new_v4().to_string(),
        issuer: issuer_did_str.to_string(),
        proposal_id: "test-proposal".to_string(),
        wasm_cid: "test-wasm-cid".to_string(),
        ccl_cid: "test-ccl-cid".to_string(),
        metrics: RuntimeExecutionMetrics {
            host_calls: 0,
            io_bytes: 0,
            mana_cost,
        },
        anchored_cids: vec![],
        resource_usage: vec![],
        timestamp: Utc::now().timestamp_micros() as u64,
        dag_epoch: Some(1),
        receipt_cid: None, // Will be set by receipt.cid() before anchoring, or by anchor_receipt itself
        signature: None,   // Will be set by signing
    };

    // Sign the receipt (mimicking sign_runtime_receipt_in_place)
    let payload_to_sign = receipt.get_payload_for_signing().expect("Failed to get payload for signing");
    let bytes_to_sign = serde_cbor::to_vec(&payload_to_sign).expect("Failed to serialize payload");
    let signature = keypair_for_signing.sign(&bytes_to_sign);
    receipt.signature = Some(signature.to_bytes().to_vec());
    
    // Set its own CID (anchor_receipt also does this, but good for consistency)
    // Note: If receipt.cid() is called *after* signature is set, and signature is part of CID calculation,
    // this might differ from what anchor_receipt calculates if it re-generates CID *before* storing signature internally.
    // The current RuntimeExecutionReceipt::cid implementation excludes receipt_cid and signature from its own hash.
    let cid = receipt.cid().expect("Failed to generate CID for test receipt");
    receipt.receipt_cid = Some(cid.to_string());

    receipt
}


// --- Test Cases ---

#[tokio::test]
async fn test_anchor_receipt_with_positive_mana_cost_deducts_mana() {
    let (runtime, mock_updater) = create_test_runtime_with_mock_updater();
    
    let executor_keypair = IcnKeyPair::generate();
    let executor_did = executor_keypair.did.clone();
    let executor_did_str = executor_did.to_string();

    // The receipt must be signed by its issuer (the executor)
    let test_receipt = create_signed_test_receipt(&executor_did_str, Some(100), &executor_keypair);

    // Before calling anchor_receipt, ensure the runtime's storage has the DID's key registered
    // if verify_signature() in anchor_receipt tries to look it up.
    // For this test, we assume verify_signature passes if the signature is arithmetically valid
    // against the public key derivable from the DID string in receipt.issuer (if did:key).

    match runtime.anchor_receipt(&test_receipt).await {
        Ok(receipt_cid_str) => {
            println!("Receipt anchored successfully: {}", receipt_cid_str);
        }
        Err(e) => {
            // If signature verification fails, this might be the cause.
            // The `issuer` DID in the receipt must be verifiable.
            // For `did:key`, the key is in the DID itself.
            // Ensure VerifiableReceipt::verify_signature can handle this.
            panic!("anchor_receipt failed: {:?}. Ensure receipt signature is valid and verifiable.", e);
        }
    }

    let deductions = mock_updater.get_mana_deductions();
    assert_eq!(deductions.len(), 1, "Expected one mana deduction call");

    let deduction = &deductions[0];
    assert_eq!(deduction.executor_did, executor_did);
    assert_eq!(deduction.amount, 100);
    assert_eq!(deduction.coop_id, "test-federation-for-scope"); // From RuntimeContext federation_id
    assert_eq!(deduction.community_id, "test-federation-for-scope"); // From RuntimeContext federation_id
}

#[tokio::test]
async fn test_anchor_receipt_with_none_mana_cost_no_deduction() {
    let (runtime, mock_updater) = create_test_runtime_with_mock_updater();
    
    let executor_keypair = IcnKeyPair::generate();
    let executor_did_str = executor_keypair.did.to_string();

    let test_receipt = create_signed_test_receipt(&executor_did_str, None, &executor_keypair);

    let anchor_result = runtime.anchor_receipt(&test_receipt).await;
    assert!(anchor_result.is_ok(), "anchor_receipt should succeed even with no mana_cost");

    let deductions = mock_updater.get_mana_deductions();
    assert!(deductions.is_empty(), "Expected no mana deduction calls when mana_cost is None");
}

#[tokio::test]
async fn test_anchor_receipt_with_zero_mana_cost_no_deduction() {
    let (runtime, mock_updater) = create_test_runtime_with_mock_updater();
    
    let executor_keypair = IcnKeyPair::generate();
    let executor_did_str = executor_keypair.did.to_string();

    let test_receipt = create_signed_test_receipt(&executor_did_str, Some(0), &executor_keypair);

    let anchor_result = runtime.anchor_receipt(&test_receipt).await;
    assert!(anchor_result.is_ok(), "anchor_receipt should succeed with zero mana_cost");

    let deductions = mock_updater.get_mana_deductions();
    assert!(deductions.is_empty(), "Expected no mana deduction calls when mana_cost is zero");
}

#[tokio::test]
async fn test_anchor_receipt_failure_before_deduction_no_deduction() {
    let (runtime, mock_updater) = create_test_runtime_with_mock_updater();
    
    let executor_keypair = IcnKeyPair::generate();
    let executor_did_str = executor_keypair.did.to_string();

    // Create a receipt but with an invalid signature (e.g., sign with a different keypair)
    let mut test_receipt = create_signed_test_receipt(&executor_did_str, Some(100), &executor_keypair);
    
    // Tamper with the signature to make it invalid
    if let Some(sig) = &mut test_receipt.signature {
        if !sig.is_empty() {
            sig[0] = sig[0].wrapping_add(1); // Invalidate the signature by changing a byte
        }
    }

    let anchor_result = runtime.anchor_receipt(&test_receipt).await;
    assert!(anchor_result.is_err(), "anchor_receipt should fail due to invalid signature");

    let deductions = mock_updater.get_mana_deductions();
    assert!(deductions.is_empty(), "Expected no mana deduction calls when anchoring fails");
}


// TODO: Add more test cases:
// 1. Test with different coop_id / community_id if a mechanism to set them is available
