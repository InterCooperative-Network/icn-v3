use axum::{extract::State, Json};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// Assuming these types are accessible. Add to Cargo.toml if not.
use icn_identity::Did; // For Did
use icn_types::mesh::JobId as IcnJobId; // For IcnJobId (usually type JobId = String)
use cid::Cid; // For Cid

/// Represents a single announced execution receipt.
#[derive(Serialize, Debug, Clone)]
pub struct AnnouncedReceiptResponseItem {
    pub job_id: IcnJobId,
    pub receipt_cid: String,
    pub executor_did: String,
}

// This is the type of the shared state we expect for this handler.
// It should be part of the AppState tuple.
pub type DiscoveredReceiptsState = Arc<RwLock<HashMap<IcnJobId, (Cid, Did)>>>;

/// Handles GET /api/v1/mesh/receipts/announced
/// Returns a list of all execution receipt announcements discovered by the node.
pub async fn list_announced_receipts_handler(
    State(discovered_receipts): State<DiscoveredReceiptsState>,
) -> Json<Vec<AnnouncedReceiptResponseItem>> {
    let announcements_map_guard = discovered_receipts.read().await;

    let response_list: Vec<AnnouncedReceiptResponseItem> = announcements_map_guard
        .iter()
        .map(|(job_id, (receipt_cid, executor_did))| AnnouncedReceiptResponseItem {
            job_id: job_id.clone(),
            receipt_cid: receipt_cid.to_string(),
            executor_did: executor_did.to_string(), // Assumes Did impls ToString
        })
        .collect();

    Json(response_list)
} 