#![allow(dead_code)]
use std::sync::Arc;
use icn_runtime::{Runtime, RuntimeContextBuilder, VmContext};
use icn_types::dag_store::SharedDagStore;
use icn_identity::KeyPair;
use anyhow::Result;
use wat::parse_str;

/// Mock storage for testing
struct MockStorage;

impl MockStorage {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl icn_runtime::RuntimeStorage for MockStorage {
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
        Ok("test-cid".to_string())
    }

    async fn anchor_to_dag(&self, _cid: &str) -> Result<String> {
        Ok("dag-cid".to_string())
    }
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
    let did = keypair.did;

    // Set up runtime context
    let ctx = RuntimeContextBuilder::new()
        .with_dag_store(Arc::new(SharedDagStore::new()))
        .with_executor_id(did.to_string())
        .build();
    
    // Create runtime with mock storage
    let storage = Arc::new(MockStorage::new());
    let runtime = Runtime::with_context(storage, ctx.clone());
    
    // Create VM context
    let vm_context = VmContext {
        executor_did: did.to_string(),
        scope: None,
        epoch: None,
        code_cid: None,
        resource_limits: None,
    };
    
    // Execute the WASM
    let result = runtime.execute_wasm(&wasm, vm_context)?;
    
    // Verify execution result
    assert_eq!(result.resource_usage.len(), 1, "Expected one resource usage record");
    assert_eq!(result.resource_usage[0].0, "token", "Expected token resource type");
    assert_eq!(result.resource_usage[0].1, 10, "Expected 10 tokens recorded");
    
    // The next step would be to check our ledger, but the current core VM system doesn't 
    // integrate with our economics module yet - that would require modifying core-vm
    // Future enhancement: ctx.resource_ledger.read().await contains ResourceType::Token with value 10
    
    Ok(())
} 