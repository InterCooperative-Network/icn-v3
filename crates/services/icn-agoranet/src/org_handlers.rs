use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthenticatedRequest, AuthError};
use crate::error::ApiError;
use crate::handlers::Db;
use crate::models::{EntityType, EntityRef, Transfer};
use crate::websocket::WebSocketState;

/// Request for transferring tokens within a cooperative
#[derive(Debug, Deserialize)]
pub struct CoopTransferRequest {
    /// Destination DID
    pub to_did: String,
    /// Amount to transfer
    pub amount: u64,
    /// Optional memo
    pub memo: Option<String>,
}

/// Response for a cooperative transfer
#[derive(Debug, Serialize)]
pub struct CoopTransferResponse {
    /// Transaction ID
    pub tx_id: Uuid,
    /// New balance of the source
    pub new_balance: u64,
}

/// Process a token transfer within a cooperative
pub async fn process_token_transfer(
    State((db, ws_state)): State<(Db, WebSocketState)>,
    auth: AuthenticatedRequest,
    Path(coop_id): Path<String>,
    Json(request): Json<CoopTransferRequest>,
) -> Result<Json<CoopTransferResponse>, ApiError> {
    // Validate that the user has cooperative operator role
    if !auth.claims.has_coop_operator_role(&coop_id) {
        return Err(ApiError::Forbidden("Cooperative operator role required".to_string()));
    }
    
    // For now, return a placeholder response
    // In a real implementation, this would update balances and record the transfer
    let response = CoopTransferResponse {
        tx_id: Uuid::new_v4(),
        new_balance: 1000, // Mock value
    };
    
    Ok(Json(response))
}

/// Request for a community governance action
#[derive(Debug, Deserialize)]
pub struct GovernanceActionRequest {
    /// Type of governance action
    pub action_type: String,
    /// Target of the action
    pub target: String,
    /// Parameters for the action
    pub parameters: serde_json::Value,
}

/// Response for a governance action
#[derive(Debug, Serialize)]
pub struct GovernanceActionResponse {
    /// Action ID
    pub action_id: Uuid,
    /// Status of the action
    pub status: String,
}

/// Process a community governance action
pub async fn process_community_governance_action(
    State((db, ws_state)): State<(Db, WebSocketState)>,
    auth: AuthenticatedRequest,
    Path(community_id): Path<String>,
    Json(request): Json<GovernanceActionRequest>,
) -> Result<Json<GovernanceActionResponse>, ApiError> {
    // Validate that the user has community official role
    if !auth.claims.has_community_official_role(&community_id) {
        return Err(ApiError::Forbidden("Community official role required".to_string()));
    }
    
    // For now, return a placeholder response
    // In a real implementation, this would execute the governance action
    let response = GovernanceActionResponse {
        action_id: Uuid::new_v4(),
        status: "pending".to_string(),
    };
    
    Ok(Json(response))
} 