use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeExecutionMetrics {
    pub fuel_used: u64,
    pub host_calls: u64,
    pub io_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeExecutionReceipt {
    pub id: String,
    pub issuer: String,
    pub proposal_id: String,
    pub wasm_cid: String,
    pub ccl_cid: String,
    pub metrics: RuntimeExecutionMetrics,
    pub anchored_cids: Vec<String>,
    pub resource_usage: Vec<(String, u64)>,
    pub timestamp: u64,
    pub dag_epoch: Option<u64>,
    pub receipt_cid: Option<String>,
    pub signature: Option<Vec<u8>>,
} 