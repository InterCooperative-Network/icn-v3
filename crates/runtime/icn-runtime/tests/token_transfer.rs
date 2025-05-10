use anyhow::Result;
use icn_economics::{Economics, ResourceAuthorizationPolicy, ResourceType, LedgerKey};
use icn_identity::{Did, KeyPair};
use icn_runtime::{VmContext, Runtime};
use std::collections::HashMap;
use std::str::FromStr;
use tokio::sync::RwLock;
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction, Module, TypeSection, ValType,
};

#[tokio::test]
async fn test_token_transfer() -> Result<()> {
    // Create DIDs for the test
    let sender_did = "did:icn:sender";
    let recipient_did = "did:icn:recipient";
    
    // Create a runtime
    let runtime = Runtime::new(Default::default())?;
    
    // Get the economics engine and resource ledger
    let economics = runtime.context().economics.clone();
    let resource_ledger = runtime.context().resource_ledger.clone();
    
    // Set up initial token balances
    // First mint tokens to the sender (in a governance context)
    {
        let mut l = resource_ledger.write().await;
        // Sender starts with 100 tokens (0 usage)
        l.insert(
            LedgerKey {
                did: sender_did.to_string(),
                resource_type: ResourceType::Token,
            },
            0 // 0 usage = 100 tokens
        );
        
        // Recipient starts with 0 tokens (default)
    }
    
    // Create a simple WAT module that transfers tokens
    let wasm_bytes = create_transfer_token_wasm(sender_did, recipient_did, 40)?;
    
    // Create execution context
    let vm_context = VmContext {
        executor_did: sender_did.to_string(),
        wasm_cid: "test_transfer_cid".to_string(),
        ..Default::default()
    };
    
    // Execute the WASM module
    let _execution_result = runtime.execute_wasm(&wasm_bytes, vm_context)?;
    
    // Verify the token balances
    let sender_usage = economics.get_usage(
        &Did::from_str(sender_did)?,
        ResourceType::Token,
        &resource_ledger
    ).await;
    
    let recipient_usage = economics.get_usage(
        &Did::from_str(recipient_did)?,
        ResourceType::Token,
        &resource_ledger
    ).await;
    
    // Sender should have 60 tokens left (40 usage)
    assert_eq!(sender_usage, 40, "Sender should have 40 usage (60 tokens left)");
    
    // Recipient should have received 40 tokens (0 usage)
    assert_eq!(recipient_usage, 0, "Recipient should have 0 usage (40 tokens)");
    
    Ok(())
}

#[tokio::test]
async fn test_token_transfer_insufficient_funds() -> Result<()> {
    // Create DIDs for the test
    let sender_did = "did:icn:sender";
    let recipient_did = "did:icn:recipient";
    
    // Create a runtime
    let runtime = Runtime::new(Default::default())?;
    
    // Get the economics engine and resource ledger
    let economics = runtime.context().economics.clone();
    let resource_ledger = runtime.context().resource_ledger.clone();
    
    // Set up initial token balances
    // Sender has only 20 tokens (80 usage)
    {
        let mut l = resource_ledger.write().await;
        l.insert(
            LedgerKey {
                did: sender_did.to_string(),
                resource_type: ResourceType::Token,
            },
            80 // 80 usage = 20 tokens
        );
    }
    
    // Create a simple WAT module that tries to transfer 40 tokens (should fail)
    let wasm_bytes = create_transfer_token_wasm(sender_did, recipient_did, 40)?;
    
    // Create execution context
    let vm_context = VmContext {
        executor_did: sender_did.to_string(),
        wasm_cid: "test_transfer_cid".to_string(),
        ..Default::default()
    };
    
    // Execute the WASM module
    let _execution_result = runtime.execute_wasm(&wasm_bytes, vm_context)?;
    
    // Verify the token balances
    let sender_usage = economics.get_usage(
        &Did::from_str(sender_did)?,
        ResourceType::Token,
        &resource_ledger
    ).await;
    
    let recipient_usage = economics.get_usage(
        &Did::from_str(recipient_did)?,
        ResourceType::Token,
        &resource_ledger
    ).await;
    
    // Sender should still have 20 tokens (80 usage, unchanged)
    assert_eq!(sender_usage, 80, "Sender should still have 80 usage (20 tokens)");
    
    // Recipient should have received 0 tokens
    assert_eq!(recipient_usage, 0, "Recipient should have 0 tokens");
    
    Ok(())
}

// Helper function to create a WASM module that calls host_transfer_token
fn create_transfer_token_wasm(sender: &str, recipient: &str, amount: u64) -> Result<Vec<u8>> {
    let mut module = Module::new();
    
    // Define the types section - void -> void for the main function
    let mut types = TypeSection::new();
    types.function(Vec::new(), Vec::new());
    module.section(&types);
    
    // Define the imports section - host functions
    let mut imports = ImportSection::new();
    
    // Import host_transfer_token(sender_ptr, sender_len, recipient_ptr, recipient_len, amount) -> i32
    imports.import(
        "icn_host",
        "host_transfer_token",
        EntityType::Function { 
            ty: types.len() as u32 
        }
    );
    module.section(&imports);
    
    // Define the functions section - we just have one function
    let mut functions = FunctionSection::new();
    functions.function(0); // Type 0 (void -> void)
    module.section(&functions);
    
    // Define the code section with our function
    let mut code = CodeSection::new();
    let mut f = Function::new(vec![]);
    
    // Encode the sender string in memory
    let sender_bytes = sender.as_bytes();
    let sender_len = sender_bytes.len();
    
    // Encode the recipient string in memory
    let recipient_bytes = recipient.as_bytes();
    let recipient_len = recipient_bytes.len();
    
    // Call host_transfer_token
    // Sender string
    f.instruction(&Instruction::I32Const(0)); // Sender pointer (placeholder)
    f.instruction(&Instruction::I32Const(sender_len as i32)); // Sender length
    
    // Recipient string
    f.instruction(&Instruction::I32Const(sender_len as i32)); // Recipient pointer (placeholder)
    f.instruction(&Instruction::I32Const(recipient_len as i32)); // Recipient length
    
    // Amount
    f.instruction(&Instruction::I64Const(amount as i64));
    
    // Call the host function
    f.instruction(&Instruction::Call(0)); // Call host_transfer_token (import index 0)
    
    // Drop the result
    f.instruction(&Instruction::Drop);
    
    // End the function
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);
    
    // Define the exports section
    let mut exports = ExportSection::new();
    exports.export("_start", ExportKind::Func, 1); // Export the function at index 1
    module.section(&exports);
    
    Ok(module.finish())
} 