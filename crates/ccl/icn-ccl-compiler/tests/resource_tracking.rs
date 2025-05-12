#![cfg(feature = "full_host_abi")]
use anyhow::Result;
use icn_ccl_compiler::CclCompiler;
use icn_economics::{Economics, ResourceAuthorizationPolicy, ResourceType};
use icn_identity::Did;
use icn_runtime::{Runtime, RuntimeContext, VmContext};
use std::{str::FromStr, sync::Arc};
use uuid::Uuid;

// Simple mock storage implementation for testing
struct MockStorage;

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

    async fn store_receipt(
        &self,
        _receipt: &icn_runtime::ExecutionReceipt,
    ) -> Result<String> {
        unimplemented!("Not needed for this test")
    }

    async fn anchor_to_dag(&self, _cid: &str) -> Result<String> {
        unimplemented!("Not needed for this test")
    }
}

// Helper to create a runtime with default economics configuration
fn runtime_with_default_economics() -> Runtime {
    // Create a policy that allows 1000 units of each resource type
    let policy = ResourceAuthorizationPolicy {
        max_cpu: 1000,
        max_memory: 1000,
        token_allowance: 1000,
    };
    
    let economics = Arc::new(Economics::new(policy));
    let context = RuntimeContext::builder()
        .with_economics(economics)
        .build();
        
    Runtime::with_context(Arc::new(MockStorage), context)
}

#[tokio::test]
async fn test_perform_metered_action() -> Result<()> {
    // Define a simple CCL script with perform_metered_action
    let ccl_source = r#"
    // Simple resource usage test
    title: "Resource Metering Test";
    
    execution {
        perform_metered_action("compute_hash", ResourceType.CPU, 25);
        perform_metered_action("store_data", ResourceType.MEMORY, 50);
        perform_metered_action("publish_result", ResourceType.TOKEN, 10);
    }
    "#;
    
    // Compile the CCL to WASM
    let compiler = CclCompiler::new()?;
    let wasm_bytes = compiler.compile_to_wasm(ccl_source)?;
    
    // Create a runtime with default economics
    let runtime = runtime_with_default_economics();
    
    // Create a VM context with a test DID
    let test_did = "did:icn:test-user";
    let vm_context = VmContext {
        executor_did: test_did.to_string(),
        scope: None,
        epoch: None,
        code_cid: None,
        resource_limits: None,
    };
    
    // Execute the WASM module
    let result = runtime.execute_wasm(&wasm_bytes, vm_context)?;
    
    // Verify resource usage was recorded
    let resource_ledger = runtime.context().resource_ledger.clone();
    let economics = runtime.context().economics.clone();
    
    // Check CPU usage
    let cpu_usage = economics.get_usage(
        &Did::from_str(test_did).unwrap(),
        ResourceType::Cpu,
        &resource_ledger
    ).await;
    assert_eq!(cpu_usage, 25, "Expected 25 units of CPU usage");
    
    // Check Memory usage
    let memory_usage = economics.get_usage(
        &Did::from_str(test_did).unwrap(),
        ResourceType::Memory,
        &resource_ledger
    ).await;
    assert_eq!(memory_usage, 50, "Expected 50 units of Memory usage");
    
    // Check Token usage
    let token_usage = economics.get_usage(
        &Did::from_str(test_did).unwrap(),
        ResourceType::Token,
        &resource_ledger
    ).await;
    assert_eq!(token_usage, 10, "Expected 10 units of Token usage");
    
    Ok(())
} 