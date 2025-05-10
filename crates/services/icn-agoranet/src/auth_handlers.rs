use axum::{
    extract::{Path as AxumPath, State},
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::handlers::Db;
use crate::auth::{
    AuthenticatedRequest, AuthError, 
    TokenIssueRequest, TokenResponse,
    issue_token, ensure_federation_admin,
    JwtConfig, revocation::TokenRevocationStore
};

/// Process a request to issue a new JWT token for a user with specific organization scopes
/// This endpoint is only accessible by federation coordinators with admin role
pub async fn issue_jwt_token_handler(
    State((db, _, jwt_config)): State<(Db, crate::websocket::WebSocketState, Arc<JwtConfig>)>,
    auth: AuthenticatedRequest,
    AxumPath(federation_id): AxumPath<String>,
    Json(payload): Json<TokenIssueRequest>,
) -> Result<Json<TokenResponse>, AuthError> {
    // Ensure the requesting user has federation admin role as a coordinator
    ensure_federation_admin(auth, &federation_id).await?;
    
    // Verify that user isn't trying to grant access to federations they don't coordinate
    if let Some(fed_ids) = &payload.federation_ids {
        for fed_id in fed_ids {
            if fed_id != &federation_id {
                return Err(AuthError::UnauthorizedOrgAccess);
            }
        }
    }
    
    // Get the federation issuer
    let issuer = Some(format!("did:icn:federation:{}", federation_id));
    
    // Issue the token
    let token_response = issue_token(&payload, issuer, &jwt_config)?;
    
    // Log token issuance action
    tracing::info!(
        "JWT token issued for {} by federation coordinator, expiring at {}",
        payload.subject,
        token_response.expires_at
    );
    
    Ok(Json(token_response))
}

/// Revoke a JWT token
/// This endpoint is only accessible by federation coordinators with admin role
pub async fn revoke_token_handler(
    State((db, _, jwt_config, revocation_store)): State<(Db, crate::websocket::WebSocketState, Arc<JwtConfig>, Arc<dyn TokenRevocationStore>)>,
    auth: AuthenticatedRequest,
    AxumPath(federation_id): AxumPath<String>,
    Json(payload): Json<crate::auth::revocation::RevokeTokenRequest>,
) -> Result<Json<crate::auth::revocation::RevokeTokenResponse>, AuthError> {
    // Ensure the requesting user has federation admin role as a coordinator
    ensure_federation_admin(auth.clone(), &federation_id).await?;
    
    // We need either a jti or a subject to revoke
    if payload.jti.is_none() && payload.subject.is_none() {
        return Err(AuthError::InvalidTokenFormat);
    }
    
    let now = Utc::now();
    let mut revoked = false;
    let mut revoked_jti = None;
    let mut revoked_subject = None;
    
    // If we have a JTI, revoke that specific token
    if let Some(jti) = &payload.jti {
        let revoked_token = crate::auth::revocation::RevokedToken {
            jti: jti.clone(),
            subject: payload.subject.clone().unwrap_or_else(|| "unknown".to_string()),
            issuer: Some(format!("did:icn:federation:{}", federation_id)),
            revoked_at: now,
            reason: payload.reason.clone(),
            revoked_by: auth.claims.sub.clone(),
        };
        
        revoked = revocation_store.revoke_token(revoked_token);
        revoked_jti = Some(jti.clone());
    } 
    // If we have a subject, revoke all tokens for that subject
    else if let Some(subject) = &payload.subject {
        // Create a dummy token with the subject
        let revoked_token = crate::auth::revocation::RevokedToken {
            jti: format!("revoked-{}-{}", subject, Uuid::new_v4()),
            subject: subject.clone(),
            issuer: Some(format!("did:icn:federation:{}", federation_id)),
            revoked_at: now,
            reason: payload.reason.clone(),
            revoked_by: auth.claims.sub.clone(),
        };
        
        revoked = revocation_store.revoke_token(revoked_token);
        revoked_subject = Some(subject.clone());
    }
    
    // Log the revocation action
    if revoked {
        tracing::info!(
            "Token revoked by federation coordinator {} for federation {}: jti={:?}, subject={:?}, reason={:?}",
            auth.claims.sub,
            federation_id,
            revoked_jti,
            revoked_subject,
            payload.reason
        );
    }
    
    // Return the response
    let response = crate::auth::revocation::RevokeTokenResponse {
        revoked,
        revoked_at: now,
        jti: revoked_jti,
        subject: revoked_subject,
    };
    
    Ok(Json(response))
}

/// Rotate a JWT token (revoke old and issue new)
/// This endpoint is only accessible by federation coordinators with admin role
pub async fn rotate_token_handler(
    State((db, _, jwt_config, revocation_store)): State<(Db, crate::websocket::WebSocketState, Arc<JwtConfig>, Arc<dyn TokenRevocationStore>)>,
    auth: AuthenticatedRequest,
    AxumPath(federation_id): AxumPath<String>,
    Json(payload): Json<crate::auth::revocation::RotateTokenRequest>,
) -> Result<Json<TokenResponse>, AuthError> {
    // Ensure the requesting user has federation admin role as a coordinator
    ensure_federation_admin(auth.clone(), &federation_id).await?;
    
    // Verify that user isn't trying to grant access to federations they don't coordinate
    if let Some(fed_ids) = &payload.federation_ids {
        for fed_id in fed_ids {
            if fed_id != &federation_id {
                return Err(AuthError::UnauthorizedOrgAccess);
            }
        }
    }
    
    // First, revoke the old token
    let revoked_token = crate::auth::revocation::RevokedToken {
        jti: payload.current_jti.clone(),
        subject: payload.subject.clone(),
        issuer: Some(format!("did:icn:federation:{}", federation_id)),
        revoked_at: Utc::now(),
        reason: payload.reason.clone().or(Some("Token rotation".to_string())),
        revoked_by: auth.claims.sub.clone(),
    };
    
    let revoked = revocation_store.revoke_token(revoked_token);
    
    if !revoked {
        tracing::warn!("Failed to revoke token {} during rotation", payload.current_jti);
        // Continue anyway since we're issuing a new token
    }
    
    // Now, issue a new token
    let token_request = TokenIssueRequest {
        subject: payload.subject.clone(),
        expires_in: payload.expires_in,
        federation_ids: payload.federation_ids.clone(),
        coop_ids: payload.coop_ids.clone(),
        community_ids: payload.community_ids.clone(),
        roles: payload.roles.clone(),
    };
    
    let issuer = Some(format!("did:icn:federation:{}", federation_id));
    let token_response = issue_token(&token_request, issuer, &jwt_config)?;
    
    // Log the token rotation
    tracing::info!(
        "Token rotated by federation coordinator {} for subject {} in federation {}: old_jti={}, new_jti={:?}",
        auth.claims.sub,
        payload.subject,
        federation_id,
        payload.current_jti,
        token_response.token_id
    );
    
    Ok(Json(token_response))
}

/// Start periodic cleanup of expired revocations
pub fn start_revocation_cleanup(revocation_store: Arc<dyn TokenRevocationStore>) {
    use tokio::time::{interval, Duration};
    
    let cleanup_interval = Duration::from_secs(3600); // Once per hour
    let retention_period = Duration::from_secs(86400 * 30); // 30 days
    
    tokio::spawn(async move {
        let mut interval = interval(cleanup_interval);
        
        loop {
            interval.tick().await;
            
            // Calculate the cutoff time (now - retention period)
            let cutoff = Utc::now() - chrono::Duration::seconds(retention_period.as_secs() as i64);
            
            // Perform the cleanup
            let removed = revocation_store.clear_expired_revocations(cutoff);
            
            if removed > 0 {
                tracing::info!("Cleaned up {} expired token revocations", removed);
            }
        }
    });
} 