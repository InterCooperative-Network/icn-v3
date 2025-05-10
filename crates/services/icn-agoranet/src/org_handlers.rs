use axum::{
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use chrono::{DateTime, Duration};

use crate::auth::{AuthenticatedRequest, AuthError};
use crate::handlers::Db;

/// Data structures for token transfers (economic operations)
#[derive(Debug, Deserialize)]
pub struct TokenTransferRequest {
    pub from_did: String,
    pub to_did: String,
    pub amount: u64,
    pub memo: Option<String>,
}

/// Data structures for governance actions
#[derive(Debug, Deserialize)]
pub struct GovernanceActionRequest {
    pub action_type: String,
    pub parameters: HashMap<String, Value>,
    pub justification: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GovernanceActionResponse {
    pub id: String,
    pub status: String,
    pub timestamp: DateTime<Utc>,
}

/// Token transaction model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransaction {
    pub id: String,
    pub from_did: String,
    pub to_did: String,
    pub amount: u64,
    pub operation: String,
    pub timestamp: DateTime<Utc>,
    pub from_coop_id: Option<String>,
    pub from_community_id: Option<String>,
    pub to_coop_id: Option<String>,
    pub to_community_id: Option<String>,
}

/// Endpoint for processing token transfer operations (economic action)
pub async fn process_token_transfer(
    auth: AuthenticatedRequest,
    AxumPath(coop_id): AxumPath<String>,
    Json(payload): Json<TokenTransferRequest>,
    State(db): State<Db>,
) -> Result<Json<TokenTransaction>, AuthError> {
    // Ensure the user has cooperative operator role for economic operations
    crate::auth::ensure_coop_operator(auth.clone(), &coop_id).await?;
    
    // Process the token transfer between accounts within the cooperative
    let transaction = TokenTransaction {
        id: format!("tx-{}", Uuid::new_v4()),
        from_did: payload.from_did,
        to_did: payload.to_did,
        amount: payload.amount,
        operation: "transfer".to_string(),
        timestamp: Utc::now(),
        from_coop_id: Some(coop_id.clone()),
        from_community_id: None,
        to_coop_id: Some(coop_id.clone()),
        to_community_id: None,
    };
    
    // Record the transaction
    let mut store = db.write()
        .map_err(|_| AuthError::Internal("Failed to acquire write lock".to_string()))?;
    
    // Add transaction to store
    store.token_transactions.push(transaction.clone());
    
    // Update balances (simplified for example)
    // In a real application, this would involve database transactions
    
    // Log the economic action
    tracing::info!(
        "Token transfer of {} processed by operator {} in cooperative {}",
        payload.amount,
        auth.claims.sub,
        coop_id
    );
    
    Ok(Json(transaction))
}

/// Endpoint for processing community governance actions
pub async fn process_community_governance_action(
    auth: AuthenticatedRequest,
    AxumPath(community_id): AxumPath<String>,
    Json(payload): Json<GovernanceActionRequest>,
    State(db): State<Db>,
) -> Result<Json<GovernanceActionResponse>, AuthError> {
    // Ensure the user has community official role for governance operations
    crate::auth::ensure_community_official(auth.clone(), &community_id).await?;
    
    // Process the governance action
    let action_id = format!("gov-action-{}", Uuid::new_v4());
    
    // Log the governance action
    tracing::info!(
        "Governance action {} processed by official {} in community {}",
        payload.action_type,
        auth.claims.sub,
        community_id
    );
    
    // Return response
    let response = GovernanceActionResponse {
        id: action_id,
        status: "approved".to_string(),
        timestamp: Utc::now(),
    };
    
    Ok(Json(response))
} 