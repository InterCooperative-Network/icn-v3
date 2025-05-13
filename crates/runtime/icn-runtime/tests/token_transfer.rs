#![allow(dead_code)]
use icn_economics::{ResourceType, LedgerKey};
use icn_identity::{Did, KeyPair, ScopeKey};
use icn_runtime::{Runtime, RuntimeContext, RuntimeContextBuilder, VmContext, RuntimeStorage, Proposal, ProposalState, QuorumStatus};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use wasm_encoder::{CodeSection, ExportKind, ExportSection, Function, FunctionSection, ImportSection, Instruction, Module, TypeSection, ValType, ConstExpr};
use std::pin::Pin;
use std::future::Future;
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};

#[derive(Clone, Default)]
struct MockRuntimeStorage {
    proposals: Arc<Mutex<HashMap<String, Proposal>>>,
    wasm_modules: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    receipts: Arc<Mutex<HashMap<String, RuntimeExecutionReceipt>>>,
    anchored_cids: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl RuntimeStorage for MockRuntimeStorage {
    async fn load_proposal(&self, id: &str) -> Result<Proposal> {
        self.proposals.lock().unwrap().get(id).cloned().ok_or_else(|| anyhow!("Proposal not found"))
    }

    async fn update_proposal(&self, proposal: &Proposal) -> Result<()> {
        let mut proposals = self.proposals.lock().unwrap();
        proposals.insert(proposal.id.clone(), proposal.clone());
        Ok(())
    }

    async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
        self.wasm_modules.lock().unwrap().get(cid).cloned().ok_or_else(|| anyhow!("WASM not found"))
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

    async fn anchor_to_dag(&self, cid: &str) -> Result<String> {
        self.anchored_cids.lock().unwrap().push(cid.to_string());
        Ok(format!("anchor-{}", cid))
    }
}

#[tokio::test]
async fn test_transfer_tokens_wasm() -> Result<()> {
    let sender_keypair = KeyPair::generate();
    let sender_did = sender_keypair.did.clone();
    let receiver_keypair = KeyPair::generate();
    let receiver_did = receiver_keypair.did.clone();

    let storage = Arc::new(MockRuntimeStorage::default());
    let mut runtime = Runtime::new(storage.clone());
    
    let context = RuntimeContextBuilder::new()
        .with_executor_id(sender_did.to_string())
        .build();
    
    // --- Mana Manager Interaction (Commented out credit, fixed balance key) ---
    // This assumes the test setup implicitly provides funds or the transfer logic handles insufficient funds.
    // let mana_mgr = context.mana_manager.lock().unwrap();
    // mana_mgr.credit(&LedgerKey { // Method 'credit' might not exist or be public
    //     did: sender_did.to_string(),
    //     resource_type: ResourceType::Token,
    //     coop_id: None,
    //     community_id: None,
    // }, 100);
    // ----------------------------------------------------------------------
    
    let wasm_bytes = build_transfer_tokens_wasm(&receiver_did.to_string(), 50)?;
    storage.store_wasm("transfer-wasm-cid", &wasm_bytes).await?;

    let vm_context = VmContext {
        executor_did: sender_did.to_string(),
        scope: None,
        epoch: None,
        code_cid: Some("transfer-wasm-cid".to_string()),
        resource_limits: None,
        coop_id: None,
        community_id: None,
    };

    let _result = runtime.execute_wasm(&wasm_bytes, "_start".to_string(), Vec::new()).await?;

    let mut final_mana_mgr = context.mana_manager.lock().unwrap();
    // Use ScopeKey instead of LedgerKey for balance check
    let sender_scope_key = ScopeKey::Individual(sender_did.to_string());
    let receiver_scope_key = ScopeKey::Individual(receiver_did.to_string());
    
    let sender_balance = final_mana_mgr.balance(&sender_scope_key).unwrap_or(0);
    let receiver_balance = final_mana_mgr.balance(&receiver_scope_key).unwrap_or(0);

    assert_eq!(sender_balance, 50, "Sender should have 50 tokens remaining");
    assert_eq!(receiver_balance, 50, "Receiver should have 50 tokens");

    Ok(())
}

fn build_transfer_tokens_wasm(receiver_did_str: &str, amount: u64) -> Result<Vec<u8>> {
    let mut module = Module::new();

    let params = vec![ValType::I32, ValType::I32, ValType::I64];
    let results = vec![ValType::I32];
    let transfer_sig_idx = 0;
    let mut types = TypeSection::new();
    types.function(params, results);

    let params_start = vec![];
    let results_start = vec![];
    let start_sig_idx = 1;
    types.function(params_start, results_start);
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import(
        "icn_host",
        "host_transfer_tokens",
        wasm_encoder::EntityType::Function(transfer_sig_idx),
    );
    let transfer_func_idx = 0;
    module.section(&imports);

    let mut functions = FunctionSection::new();
    let start_func_local_idx = 0;
    functions.function(start_sig_idx);
    module.section(&functions);

    let mut memory = wasm_encoder::MemorySection::new();
    memory.memory(wasm_encoder::MemoryType { minimum: 1, maximum: None, memory64: false, shared: false });
    module.section(&memory);

    let mut exports = ExportSection::new();
    exports.export("_start", ExportKind::Func, start_func_local_idx + transfer_func_idx + 1 );
    exports.export("memory", ExportKind::Memory, 0);
    module.section(&exports);

    let mut data = wasm_encoder::DataSection::new();
    let receiver_did_bytes = receiver_did_str.as_bytes();
    let memory_offset = 0;
    data.active(0, &ConstExpr::i32_const(memory_offset), receiver_did_bytes.to_vec());
    module.section(&data);

    let mut code = CodeSection::new();
    let locals = vec![];
    let mut f = Function::new(locals);
    f.instruction(&Instruction::I32Const(memory_offset));
    f.instruction(&Instruction::I32Const(receiver_did_bytes.len() as i32));
    f.instruction(&Instruction::I64Const(amount as i64));
    f.instruction(&Instruction::Call(transfer_func_idx));
    f.instruction(&Instruction::Drop);
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    Ok(module.finish())
} 