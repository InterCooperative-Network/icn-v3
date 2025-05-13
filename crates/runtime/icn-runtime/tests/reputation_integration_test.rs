#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use anyhow::{Result, anyhow};
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