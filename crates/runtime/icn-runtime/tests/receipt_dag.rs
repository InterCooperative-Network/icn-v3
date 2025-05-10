use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use icn_economics::ResourceType;
use icn_identity::{Did, KeyPair};
use icn_mesh_receipts::{ExecutionReceipt, sign_receipt};
use icn_runtime::{Runtime, RuntimeContext, VmContext};
use icn_types::dag_store::SharedDagStore;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use uuid::Uuid;
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, 
    ImportSection, Instruction, Module, TypeSection, ValType, DataSection, DataSegment,
    MemorySection, MemoryType,
};

// Mock storage for testing
#[derive(Clone)]
struct MockStorage {
    receipts: Arc<Mutex<Vec<String>>>,
    anchored: Arc<RwLock<Vec<String>>>,
}

impl MockStorage {
    pub fn new() -> Self {
        Self {
            receipts: Arc::new(Mutex::new(Vec::new())),
            anchored: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait]
impl icn_runtime::RuntimeStorage for MockStorage {
    async fn load_proposal(&self, _id: &str) -> Result<icn_runtime::Proposal> {
        Err(anyhow!("Not implemented for test"))
    }

    async fn update_proposal(&self, _proposal: &icn_runtime::Proposal) -> Result<()> {
        Err(anyhow!("Not implemented for test"))
    }

    async fn load_wasm(&self, _cid: &str) -> Result<Vec<u8>> {
        Err(anyhow!("Not implemented for test"))
    }

    async fn store_receipt(&self, _receipt: &icn_runtime::ExecutionReceipt) -> Result<String> {
        let receipt_id = format!("receipt-{}", Uuid::new_v4());
        self.receipts.lock().unwrap().push(receipt_id.clone());
        Ok(receipt_id)
    }

    async fn anchor_to_dag(&self, cid: &str) -> Result<String> {
        self.anchored.write().await.push(cid.to_string());
        Ok(format!("anchor-{}", Uuid::new_v4()))
    }
}

#[tokio::test]
async fn test_anchor_receipt_in_dag() -> Result<()> {
    // 1. Generate a keypair for signing
    let kp = KeyPair::generate();
    
    // 2. Create a shared DAG store
    let receipt_store = Arc::new(SharedDagStore::new());
    
    // 3. Create runtime context with receipt store and federation ID
    let ctx = RuntimeContext::builder()
        .with_receipt_store(receipt_store.clone())
        .with_federation_id("test-federation")
        .with_executor_id(kp.did.to_string())
        .build();
    
    // 4. Create runtime with mock storage
    let storage = Arc::new(MockStorage::new());
    let runtime = Runtime::with_context(storage, ctx);
    
    // 5. Create a test receipt
    let mut usage = HashMap::new();
    usage.insert(ResourceType::Cpu, 500);
    
    let mut receipt = ExecutionReceipt {
        task_cid: "bafybeideputvakentavfc".to_string(),
        executor: kp.did.clone(),
        resource_usage: usage,
        timestamp: Utc::now(),
        signature: Vec::new(), // Will be filled after signing
    };
    
    // 6. Sign the receipt
    let signature = sign_receipt(&receipt, &kp)?;
    receipt.signature = signature.to_bytes().to_vec();
    
    // 7. Create WASM module to call host_anchor_receipt
    let receipt_cbor = serde_cbor::to_vec(&receipt)?;
    
    // Create a minimal WASM module using wasm_encoder
    let mut module = Module::new();
    
    // Define the type (i32, i32) -> i32 for host_anchor_receipt
    let mut types = TypeSection::new();
    types.function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    module.section(&types);
    
    // Import host_anchor_receipt
    let mut imports = ImportSection::new();
    imports.import(
        "icn_host",
        "host_anchor_receipt",
        EntityType::Function(0),
    );
    module.section(&imports);
    
    // Define our main function
    let mut functions = FunctionSection::new();
    functions.function(0); // Using type 0
    module.section(&functions);
    
    // Create the code for our function
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // Call host_anchor_receipt(0, cbor.len())
    func.instruction(&Instruction::I32Const(0)); // Pointer to data
    func.instruction(&Instruction::I32Const(receipt_cbor.len() as i32)); // Length
    func.instruction(&Instruction::Call(0)); // Call import 0 (host_anchor_receipt)
    func.instruction(&Instruction::End);
    code.function(&func);
    module.section(&code);
    
    // Add data section to include our CBOR bytes
    let mut data = DataSection::new();
    data.segment(DataSegment::new(0, &[Instruction::I32Const(0), Instruction::End], receipt_cbor.clone()));
    module.section(&data);
    
    // Add exports for memory and function
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("_start", ExportKind::Func, 1); // Export our function
    module.section(&exports);
    
    // Add memory section
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: None,
        shared: false,
    });
    module.section(&memories);
    
    // Compile the WASM module
    let wasm = module.finish();
    
    // 8. Execute the WASM module
    let vm_ctx = VmContext {
        executor_did: kp.did.to_string(),
        scope: Some("test-scope".to_string()),
        epoch: Some("2023-01-01".to_string()),
        code_cid: Some("test-cid".to_string()),
        resource_limits: None,
    };
    
    let result = runtime.execute_wasm(&wasm, vm_ctx)?;
    
    // 9. Verify the receipt was anchored
    let dag_nodes = receipt_store.list().await?;
    
    // There should be at least one DAG node in the receipt store
    assert!(!dag_nodes.is_empty(), "No DAG nodes found in receipt store");
    
    // The DAG node should have Receipt event type
    assert_eq!(
        dag_nodes[0].event_type,
        icn_types::dag::DagEventType::Receipt,
        "DAG node should have Receipt event type"
    );
    
    // The scope should start with "receipt/"
    assert!(
        dag_nodes[0].scope_id.starts_with("receipt/"),
        "DAG node scope should start with receipt/"
    );
    
    // Success
    Ok(())
} 