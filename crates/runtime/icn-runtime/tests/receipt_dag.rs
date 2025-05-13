#![allow(dead_code)]
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use cid::Cid;
use icn_identity::{KeyPair, Did};
use icn_mesh_receipts::ExecutionReceipt as MeshExecutionReceipt;
use icn_runtime::{
    Runtime, RuntimeContext, RuntimeContextBuilder, RuntimeStorage, VmContext,
    Proposal, ProposalState, QuorumStatus
};
use icn_types::dag_store::{DagStore, SharedDagStore};
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use icn_types::mesh::JobStatus as MeshJobStatus;
use serde_cbor;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection, Instruction, MemorySection, MemoryType, Module, TypeSection, ValType
};
use chrono::Utc;
use std::pin::Pin;
use std::future::Future;

#[derive(Clone, Default)]
struct MockStorage {
    proposals: Arc<Mutex<HashMap<String, Proposal>>>,
    wasm_modules: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    receipts: Arc<Mutex<HashMap<String, RuntimeExecutionReceipt>>>,
    anchored_cids: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl RuntimeStorage for MockStorage {
    async fn load_proposal(&self, id: &str) -> Result<Proposal> {
        self.proposals.lock().unwrap().get(id).cloned().ok_or_else(|| anyhow!("Proposal not found"))
    }

    async fn update_proposal(&self, proposal: &Proposal) -> Result<()> {
        self.proposals.lock().unwrap().insert(proposal.id.clone(), proposal.clone());
        Ok(())
    }

    async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
        self.wasm_modules.lock().unwrap().get(cid).cloned().ok_or_else(|| anyhow!("WASM not found"))
    }

    async fn store_receipt(&self, receipt: &RuntimeExecutionReceipt) -> Result<String> {
        let id = receipt.id.clone();
        self.receipts.lock().unwrap().insert(id.clone(), receipt.clone());
        Ok(id)
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
async fn test_receipt_dag_anchoring() -> Result<()> {
    let storage = Arc::new(MockStorage::default());
    let receipt_store = Arc::new(SharedDagStore::new());

    let keypair = KeyPair::generate();
    let node_did = keypair.did.clone();

    let ctx = RuntimeContextBuilder::new()
        .with_identity(keypair.clone())
        .with_executor_id(node_did.to_string())
        .with_dag_store(receipt_store.clone())
        .build();

    let mut runtime = Runtime::with_context(storage.clone(), Arc::new(ctx));

    let original_receipt = RuntimeExecutionReceipt {
        id: "test-receipt-id".to_string(),
        issuer: node_did.to_string(),
        proposal_id: "test-proposal".to_string(),
        wasm_cid: "test-wasm-cid".to_string(),
        ccl_cid: "test-ccl-cid".to_string(),
        metrics: RuntimeExecutionMetrics {
            fuel_used: 0,
            host_calls: 0,
            io_bytes: 0,
        },
        anchored_cids: vec![],
        resource_usage: vec![],
        timestamp: Utc::now().timestamp_millis() as u64,
        dag_epoch: Some(1),
        receipt_cid: None,
        signature: None,
    };

    let anchored_cid_str = runtime.anchor_receipt(&original_receipt).await?;
    let anchored_cid = Cid::from_str(&anchored_cid_str)?;

    let dag_nodes = receipt_store.list().await?;
    let found_in_dag = dag_nodes.iter().any(|dag_node| {
        let cid_str_from_node = dag_node.cid().to_string();
        if let Ok(cid_from_store) = Cid::from_str(&cid_str_from_node) {
            cid_from_store == anchored_cid
        } else {
            tracing::warn!("Failed to parse CID {:?} from DAG node", dag_node);
            false
        }
    });
    assert!(found_in_dag, "Anchored CID {} not found in DAG store", anchored_cid_str);

    Ok(())
}

fn build_receipt_wasm_module(receipt_cbor: &[u8]) -> Result<Vec<u8>> {
    let mut module = Module::new();

    let mut types = TypeSection::new();
    types.function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    types.function(vec![], vec![]);
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import("icn_host", "host_anchor_receipt", EntityType::Function(0));
    module.section(&imports);

    let mut functions = FunctionSection::new();
    functions.function(1);
    module.section(&functions);

    let mut memories = MemorySection::new();
    memories.memory(MemoryType { minimum: 1, maximum: None, memory64: false, shared: false });
    module.section(&memories);

    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("_start", ExportKind::Func, 1);
    module.section(&exports);

    let mut data = DataSection::new();
    data.active(0, &ConstExpr::i32_const(0), receipt_cbor.to_vec());
    module.section(&data);

    let mut code = CodeSection::new();
    let mut f = Function::new(vec![]);
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::I32Const(receipt_cbor.len() as i32));
    f.instruction(&Instruction::Call(0));
    f.instruction(&Instruction::Drop);
    f.instruction(&Instruction::End);
    code.function(&f);
    module.section(&code);

    Ok(module.finish())
}

#[tokio::test]
async fn test_wasm_anchors_receipt() -> Result<()> {
    let storage = Arc::new(MockStorage::default());
    let receipt_store = Arc::new(SharedDagStore::new());

    let keypair = KeyPair::generate();
    let node_did = keypair.did.clone();

    let ctx = RuntimeContextBuilder::new()
        .with_identity(keypair.clone())
        .with_executor_id(node_did.to_string())
        .with_dag_store(receipt_store.clone())
        .build();

    let mut runtime = Runtime::with_context(storage.clone(), Arc::new(ctx));

    let mesh_receipt = MeshExecutionReceipt {
        job_id: "job-123".to_string(),
        status: MeshJobStatus::Completed,
        result_data_cid: None,
        logs_cid: None,
        execution_start_time: Utc::now().timestamp_millis() as u64,
        execution_end_time: Utc::now().timestamp_millis() as u64,
        executor: node_did.clone(),
        signature: Vec::new(),
        resource_usage: HashMap::new(),
        coop_id: None,
        community_id: None,
        execution_end_time_dt: Utc::now(),
    };
    let receipt_cbor = serde_cbor::to_vec(&mesh_receipt)?;
    let wasm = build_receipt_wasm_module(&receipt_cbor)?;

    let vm_ctx = VmContext {
        executor_did: node_did.to_string(),
        scope: None,
        epoch: None,
        code_cid: Some("wasm_cid_placeholder_for_test".to_string()),
        resource_limits: None,
        coop_id: None,
        community_id: None,
    };
    let _result = runtime.execute_wasm(&wasm, "_start".to_string(), Vec::new()).await?;

    let dag_nodes = receipt_store.list().await?;
    assert!(!dag_nodes.is_empty(), "Expected DAG store to have at least one entry after WASM execution");

    Ok(())
} 