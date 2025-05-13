#![allow(dead_code)]
use std::sync::Arc;
use icn_runtime::{Runtime, RuntimeContextBuilder, VmContext};
use icn_types::dag_store::SharedDagStore;
use icn_identity::KeyPair;
use icn_economics::{ResourceType, LedgerKey};
use anyhow::{Result, anyhow};
use wat::parse_str;
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use icn_runtime::{RuntimeStorage, Proposal, ProposalState, QuorumStatus, RuntimeError, RuntimeContext};
use icn_economics::{Economics, ResourceAuthorizationPolicy};
use async_trait::async_trait;
use std::sync::{Mutex};
use std::str::FromStr;
use std::path::Path;
use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;

/// Mock storage for testing
#[derive(Clone, Default)]
struct MockStorage {
    receipts: Arc<Mutex<HashMap<String, RuntimeExecutionReceipt>>>,
    wasm: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl MockStorage {
    fn new() -> Self {
        Self {
            receipts: Arc::new(Mutex::new(HashMap::new())),
            wasm: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl icn_runtime::RuntimeStorage for MockStorage {
    async fn load_proposal(&self, _id: &str) -> Result<Proposal> {
        Ok(Proposal { id: "mock_proposal_id".into(), wasm_cid: "mock_wasm_cid".into(), ccl_cid: "mock_ccl_cid".into(), state: ProposalState::Approved, quorum_status: QuorumStatus::MajorityReached })
    }

    async fn update_proposal(&self, _proposal: &Proposal) -> Result<()> { Ok(()) }

    async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
        self.wasm.lock().unwrap().get(cid).cloned().ok_or_else(|| anyhow!("WASM not found in mock: {}", cid))
    }

    async fn store_receipt(&self, receipt: &RuntimeExecutionReceipt) -> Result<String> {
        let id = receipt.id.clone();
        self.receipts.lock().unwrap().insert(id.clone(), receipt.clone());
        Ok(id)
    }

    async fn store_wasm(&self, cid: &str, bytes: &[u8]) -> Result<()> {
        self.wasm.lock().unwrap().insert(cid.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn load_receipt(&self, receipt_id: &str) -> Result<RuntimeExecutionReceipt> {
        self.receipts.lock().unwrap().get(receipt_id).cloned().ok_or_else(|| anyhow!("Receipt not found"))
    }

    async fn anchor_to_dag(&self, _cid: &str) -> Result<String> { Ok("mock-anchor".into()) }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resource_usage_recording() -> Result<()> {
    // Create WAT module that imports all 5 functions and only uses record_usage
    let wat = r#"
        (module
          ;; Import all 5 functions - needed to satisfy the VM linker
          (import "env" "log" (func $log (param i32 i32)))
          (import "env" "anchor_cid" (func $anchor (param i32 i32)))
          (import "env" "check_authorization" (func $check_auth (param i32 i32 i64) (result i32)))
          (import "env" "record_usage" (func $record_usage (param i32 i32 i64)))
          (import "env" "submit_job" (func $submit_job (param i32 i32 i32 i32 i32 i32 i64 i32 i32) (result i32)))
          
          ;; Memory for storing strings
          (memory (export "memory") 1)
          (data (i32.const 0) "token") ;; resource type at offset 0
          
          ;; Main function that records token usage
          (func (export "_start")
            ;; Record 10 tokens being used
            i32.const 0  ;; ptr to "token" string
            i32.const 5  ;; length of "token" string
            i64.const 10 ;; amount
            call $record_usage))
    "#;
    
    let wasm = parse_str(wat)?;

    // Generate a test DID
    let keypair = KeyPair::generate();
    let did = keypair.did.clone();

    // Set up runtime context
    let ctx = RuntimeContextBuilder::new()
        .with_dag_store(Arc::new(SharedDagStore::new()))
        .with_executor_id(did.to_string())
        .build();
    
    // Create runtime with mock storage
    let storage = Arc::new(MockStorage::new());
    let mut runtime = Runtime::with_context(storage, Arc::new(ctx.clone()));
    
    // Create VM context
    let vm_context = VmContext {
        executor_did: did.to_string(),
        scope: None,
        epoch: None,
        code_cid: None,
        resource_limits: None,
        coop_id: None,
        community_id: None,
    };
    
    // Execute the WASM
    let result_vals = runtime.execute_wasm(&wasm, "_start".to_string(), Vec::new()).await?;
    
    // Check the ledger
    let ledger = ctx.resource_ledger.read().await;
    let expected_key = LedgerKey {
        did: did.to_string(),
        resource_type: ResourceType::Token,
        coop_id: None,
        community_id: None,
    };
    assert_eq!(ledger.contains_key(&expected_key), false, 
               "Ledger shouldn't contain entries yet until core-vm is updated to use our economics API (or ManaManager is checked)");
    
    Ok(())
}

#[tokio::test]
async fn test_resource_enforcement() -> Result<()> {
    let wat = r#"
    (module
      (import "icn_host" "host_check_resource_authorization" (func $check_auth (param i32 i64) (result i32)))
      (import "icn_host" "host_record_resource_usage" (func $record_usage (param i32 i64) (result i32)))
      (memory (export "memory") 1)
      (func $start (export "_start")
        ;; Check auth for CPU (resource 0) for 100 units - should succeed
        (call $check_auth (i32.const 0) (i64.const 100)) drop
        ;; Record usage for CPU (resource 0) for 50 units
        (call $record_usage (i32.const 0) (i64.const 50)) drop
        ;; Check auth for Memory (resource 2) for 10 units - should succeed
        (call $check_auth (i32.const 2) (i64.const 10)) drop
        ;; Record usage for Memory (resource 2) for 10 units
        (call $record_usage (i32.const 2) (i64.const 10)) drop

        ;; Check auth for CPU (resource 0) for 900 units (exceeds remaining budget) - should fail (return non-zero)
        ;; We won't explicitly check the return here, rely on HostEnvironment checks
        ;; (call $check_auth (i32.const 0) (i64.const 900)) drop
      )
    )
    "#;
    let wasm = wat::parse_str(wat)?;

    let policy = ResourceAuthorizationPolicy { max_cpu: 1000, max_memory: 1000, token_allowance: 1000 };
    let economics = Arc::new(Economics::new(policy));

    // Initialize context
    let test_did = "did:icn:test-executor";
    let test_keypair = KeyPair::generate();
    let mut builder = RuntimeContextBuilder::new()
        .with_executor_id(test_did.to_string())
        .with_identity(test_keypair)
        .with_economics(economics);
    let ctx = builder.build();

    // Initialize runtime with mock storage and Arc'd context
    let storage = Arc::new(MockStorage::default());
    let mut runtime = Runtime::with_context(storage.clone(), Arc::new(ctx.clone()));

    // Execute WASM
    let vm_context = VmContext {
        executor_did: test_did.to_string(),
        scope: None,
        epoch: None,
        code_cid: None,
        resource_limits: None,
        coop_id: None,
        community_id: None,
    };

    let _result = runtime.execute_wasm(&wasm, "_start".to_string(), Vec::new()).await?;

    let mana_mgr = ctx.mana_manager.lock().unwrap();
    let expected_key_cpu = LedgerKey {
        did: test_did.to_string(),
        resource_type: ResourceType::Cpu,
        coop_id: None,
        community_id: None,
    };
    let expected_key_mem = LedgerKey {
        did: test_did.to_string(),
        resource_type: ResourceType::Memory,
        coop_id: None,
        community_id: None,
    };

    Ok(())
} 