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
use crate::models::TokenTransaction;

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
        from_did: payload.from_did.clone(),
        to_did: payload.to_did.clone(),
        amount: payload.amount,
        operation: "transfer".to_string(),
        timestamp: Utc::now(),
        from_coop_id: Some(coop_id.clone()),
        from_community_id: None,
        to_coop_id: Some(coop_id.clone()),
        to_community_id: None,
    };
    
    // In a real implementation, we would add this to a database
    // For now, we'll just log the transaction
    tracing::info!(
        "Token transfer created: ID={}, from={}, to={}, amount={}, coop={}",
        transaction.id,
        transaction.from_did,
        transaction.to_did,
        transaction.amount,
        coop_id
    );
    
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