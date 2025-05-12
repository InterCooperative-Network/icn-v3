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
async fn test_mint_token_with_governance_guard() -> Result<()> {
    // Define a CCL script that tries to mint tokens
    let ccl_source = r#"
    // Token minting test
    title: "Token Minting Test";
    
    execution {
        // This will only succeed in a governance context
        mint_token {
            type "test_token"
            amount 50
            recipient "did:icn:recipient-user"
        }
    }
    "#;
    
    // Compile the CCL to WASM
    let compiler = CclCompiler::new()?;
    let wasm_bytes = compiler.compile_to_wasm(ccl_source)?;
    
    // 1. First, test with regular (non-governance) context - should fail
    let runtime = runtime_with_default_economics();
    
    // Create a VM context with a test DID
    let caller_did = "did:icn:test-user";
    let recipient_did = "did:icn:recipient-user";
    let vm_context = VmContext {
        executor_did: caller_did.to_string(),
        scope: None,
        epoch: None,
        code_cid: None,
        resource_limits: None,
    };
    
    // Execute the WASM module in non-governance context
    let result = runtime.execute_wasm(&wasm_bytes, vm_context.clone())?;
    
    // Verify no tokens were minted (since we're not in governance context)
    let resource_ledger = runtime.context().resource_ledger.clone();
    let economics = runtime.context().economics.clone();
    
    let recipient_usage = economics.get_usage(
        &Did::from_str(recipient_did).unwrap(),
        ResourceType::Token,
        &resource_ledger
    ).await;
    
    // Tokens shouldn't have been minted (usage should still be 0)
    assert_eq!(recipient_usage, 0, "Non-governance context should not mint tokens");
    
    // 2. Now test with a governance context - should succeed
    let runtime = runtime_with_default_economics();
    
    // Set up a pre-existing token usage for the recipient to verify minting decreases it
    {
        let ledger = runtime.context().resource_ledger.clone();
        let mut ledger_write = ledger.write().await;
        ledger_write.insert(
            icn_economics::LedgerKey {
                did: recipient_did.to_string(),
                resource_type: ResourceType::Token,
            },
            100 // Initial token usage
        );
    }
    
    // Now execute in governance context
    let governance_result = runtime.governance_execute_wasm(&wasm_bytes, vm_context.clone())?;
    
    // Verify the token usage was updated
    let recipient_usage = economics.get_usage(
        &Did::from_str(recipient_did).unwrap(),
        ResourceType::Token,
        &resource_ledger
    ).await;
    
    // 100 initial usage - 50 minted = 50 remaining usage
    assert_eq!(recipient_usage, 50, 
               "Recipient's token usage should be reduced by minting operation");
    
    Ok(())
} 