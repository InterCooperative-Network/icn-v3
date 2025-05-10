use std::collections::HashMap;
use axum::{
    async_trait,
    extract::{FromRequestParts, Query},
    http::{request::Parts, StatusCode, header},
    response::{IntoResponse, Response},
    RequestPartsExt,
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use uuid;
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, EncodingKey, Header};

// Add the revocation module
pub mod revocation;
use self::revocation::TokenRevocationStore;

/// JWT configuration
#[derive(Clone, Debug)]
pub struct JwtConfig {
    /// The secret key used to verify JWT signatures
    pub secret_key: String,
    /// The issuer expected in JWT claims
    pub issuer: Option<String>,
    /// The expected audience in JWT claims
    pub audience: Option<String>,
    /// JWT validation settings
    pub validation: Validation,
}

impl Default for JwtConfig {
    fn default() -> Self {
        let mut validation = Validation::default();
        // By default, require the sub claim
        validation.set_required_spec_claims(&["sub", "exp"]);
        
        Self {
            secret_key: "change_this_to_a_secure_secret_key_in_production".to_string(),
            issuer: None,
            audience: None,
            validation,
        }
    }
}

/// The claims structure inside the JWT
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject (user DID)
    pub sub: String,
    /// Issuer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Audience
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Expiration time (as numeric date)
    pub exp: usize,
    /// Issued at (as numeric date)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<usize>,
    /// Not before (as numeric date)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<usize>,
    /// JWT ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    
    /// Federation IDs the user has access to
    #[serde(default)]
    pub federation_ids: Vec<String>,
    /// Cooperative IDs the user has access to
    #[serde(default)]
    pub coop_ids: Vec<String>,
    /// Community IDs the user has access to
    #[serde(default)]
    pub community_ids: Vec<String>,
    /// Roles by organization ID
    #[serde(default)]
    pub roles: HashMap<String, Vec<String>>,
}

/// The scope claims extracted from a JWT
#[derive(Debug, Clone)]
pub struct ScopeClaims {
    /// Subject (user DID)
    pub sub: String,
    /// Federation IDs the user has access to
    pub federation_ids: Vec<String>,
    /// Cooperative IDs the user has access to
    pub coop_ids: Vec<String>,
    /// Community IDs the user has access to
    pub community_ids: Vec<String>,
    /// Roles by organization ID
    pub roles: HashMap<String, Vec<String>>,
}

impl From<Claims> for ScopeClaims {
    fn from(claims: Claims) -> Self {
        Self {
            sub: claims.sub,
            federation_ids: claims.federation_ids,
            coop_ids: claims.coop_ids,
            community_ids: claims.community_ids,
            roles: claims.roles,
        }
    }
}

impl ScopeClaims {
    /// Check if the user has access to the specified federation
    pub fn has_federation_access(&self, federation_id: &str) -> bool {
        self.federation_ids.contains(&federation_id.to_string())
    }

    /// Check if the user has access to the specified cooperative (economic engine)
    pub fn has_coop_access(&self, coop_id: &str) -> bool {
        // You have access if the cooperative ID is in your allowed list
        self.coop_ids.contains(&coop_id.to_string())
    }

    /// Check if the user has access to the specified community (governance body)
    pub fn has_community_access(&self, community_id: &str) -> bool {
        // You have access if the community ID is in your allowed list
        self.community_ids.contains(&community_id.to_string())
    }

    /// Check if the user has the specified role in the organization
    pub fn has_role(&self, org_id: &str, role: &str) -> bool {
        self.roles
            .get(org_id)
            .map(|roles| roles.contains(&role.to_string()))
            .unwrap_or(false)
    }

    /// Check if the user has access to the specified organization scope
    pub fn has_org_scope_access(
        &self,
        federation_id: Option<&str>,
        coop_id: Option<&str>,
        community_id: Option<&str>,
    ) -> bool {
        // Check federation access if specified
        if let Some(fed_id) = federation_id {
            if !self.has_federation_access(fed_id) {
                return false;
            }
        }

        // Check cooperative access if specified
        if let Some(coop_id) = coop_id {
            if !self.has_coop_access(coop_id) {
                return false;
            }
        }

        // Check community access if specified
        if let Some(comm_id) = community_id {
            if !self.has_community_access(comm_id) {
                return false;
            }
        }

        true
    }
    
    /// Check if the user has economic operator role for a cooperative
    pub fn has_coop_operator_role(&self, coop_id: &str) -> bool {
        self.has_role(coop_id, ROLE_COOP_OPERATOR)
    }
    
    /// Check if the user has community official role for governance actions
    pub fn has_community_official_role(&self, community_id: &str) -> bool {
        self.has_role(community_id, ROLE_COMMUNITY_OFFICIAL)
    }
    
    /// Check if the user has federation admin role for coordination actions
    pub fn has_federation_admin_role(&self, federation_id: &str) -> bool {
        self.has_role(federation_id, ROLE_FEDERATION_ADMIN)
    }
}

/// Authentication errors
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("missing authorization header")]
    MissingAuthHeader,
    
    #[error("invalid token format")]
    InvalidTokenFormat,
    
    #[error("token validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("missing required scope")]
    MissingScope,
    
    #[error("token expired")]
    TokenExpired,
    
    #[error("token has been revoked")]
    TokenRevoked,
    
    #[error("unauthorized organization access")]
    UnauthorizedOrgAccess,
    
    #[error("missing required role for this operation")]
    MissingRole,
    
    #[error("operation only available to cooperative operators")]
    NotCoopOperator,
    
    #[error("operation only available to community officials")]
    NotCommunityOfficial,
    
    #[error("operation only available to federation coordinators")]
    NotFederationCoordinator,
    
    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingAuthHeader => (StatusCode::UNAUTHORIZED, "Missing authorization header".to_string()),
            AuthError::InvalidTokenFormat => (StatusCode::BAD_REQUEST, "Invalid token format".to_string()),
            AuthError::ValidationFailed(msg) => (StatusCode::UNAUTHORIZED, msg),
            AuthError::MissingScope => (StatusCode::FORBIDDEN, "Missing required organization scope".to_string()),
            AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "Token expired".to_string()),
            AuthError::TokenRevoked => (StatusCode::FORBIDDEN, "Token has been revoked".to_string()),
            AuthError::UnauthorizedOrgAccess => (StatusCode::FORBIDDEN, "Unauthorized organization access".to_string()),
            AuthError::MissingRole => (StatusCode::FORBIDDEN, "Missing required role for this operation".to_string()),
            AuthError::NotCoopOperator => (StatusCode::FORBIDDEN, "This operation requires cooperative operator role".to_string()),
            AuthError::NotCommunityOfficial => (StatusCode::FORBIDDEN, "This operation requires community official role".to_string()),
            AuthError::NotFederationCoordinator => (StatusCode::FORBIDDEN, "This operation requires federation coordinator role".to_string()),
            AuthError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Internal server error: {}", msg)),
        };

        // Create the response with the appropriate status code and message
        match status {
            StatusCode::UNAUTHORIZED => {
                (status, [(header::WWW_AUTHENTICATE, "Bearer")], message).into_response()
            }
            _ => (status, message).into_response()
        }
    }
}

/// Organization scope parameters for API endpoints
#[derive(Debug, Deserialize, Clone)]
pub struct OrgScopeParams {
    /// Federation ID to scope the request
    pub federation_id: Option<String>,
    /// Cooperative ID to scope the request
    pub coop_id: Option<String>,
    /// Community ID to scope the request
    pub community_id: Option<String>,
}

/// Helper function to validate and decode a JWT
pub fn validate_token(token: &str, config: &JwtConfig) -> Result<Claims, AuthError> {
    let decoding_key = DecodingKey::from_secret(config.secret_key.as_bytes());
    
    // Create a custom validation with the provided config
    let mut validation = config.validation.clone();
    
    // Set issuer if provided
    if let Some(issuer) = &config.issuer {
        validation.set_issuer(&[issuer]);
    }
    
    // Set audience if provided
    if let Some(audience) = &config.audience {
        validation.set_audience(&[audience]);
    }
    
    // Decode and validate the token
    match decode::<Claims>(token, &decoding_key, &validation) {
        Ok(token_data) => Ok(token_data.claims),
        Err(err) => {
            tracing::warn!("JWT validation failed: {}", err);
            Err(AuthError::ValidationFailed(err.to_string()))
        }
    }
}

/// Extractor for authenticated requests with scoped claims
#[derive(Debug, Clone)]
pub struct AuthenticatedRequest {
    /// The validated scope claims
    pub claims: ScopeClaims,
    /// The original JWT token ID (jti)
    pub token_id: Option<String>,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthenticatedRequest
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Extract the JWT config from the state
        let jwt_config = parts.extensions
            .get::<Arc<JwtConfig>>()
            .ok_or_else(|| AuthError::Internal("JWT config not found".to_string()))?
            .clone();
            
        // Try to get the revocation store (if available)
        let revocation_store = parts.extensions
            .get::<Arc<dyn TokenRevocationStore>>();

        // Try to extract bearer token from Authorization header
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .ok_or(AuthError::MissingAuthHeader)?;
            
        let auth_header_str = auth_header
            .to_str()
            .map_err(|_| AuthError::InvalidTokenFormat)?;
            
        // Check if it's a Bearer token and extract the token value
        if !auth_header_str.starts_with("Bearer ") {
            return Err(AuthError::InvalidTokenFormat);
        }
        
        let token = &auth_header_str["Bearer ".len()..];
        
        // Validate the token
        let claims = validate_token(token, &jwt_config)?;
        
        // Check if token is revoked (if we have a revocation store)
        if let Some(store) = revocation_store {
            revocation::check_token_not_revoked(store.as_ref(), &claims.jti)?;
        }
        
        Ok(AuthenticatedRequest {
            claims: claims.clone().into(),
            token_id: claims.jti,
        })
    }
}

/// Middleware to enforce organization scope access
pub async fn enforce_org_scope_access(
    auth: AuthenticatedRequest,
    Query(params): Query<OrgScopeParams>,
) -> Result<AuthenticatedRequest, AuthError> {
    // Check if the user has access to the requested organization scope
    if !auth.claims.has_org_scope_access(
        params.federation_id.as_deref(),
        params.coop_id.as_deref(),
        params.community_id.as_deref(),
    ) {
        return Err(AuthError::UnauthorizedOrgAccess);
    }
    
    Ok(auth)
}

/// Token issuance request
#[derive(Debug, Deserialize)]
pub struct TokenIssueRequest {
    /// Subject (user DID) to issue the token for
    pub subject: String,
    /// Expiration time in seconds from now
    pub expires_in: Option<u64>,
    /// Federation IDs to grant access to 
    pub federation_ids: Option<Vec<String>>,
    /// Cooperative IDs to grant access to
    pub coop_ids: Option<Vec<String>>,
    /// Community IDs to grant access to
    pub community_ids: Option<Vec<String>>,
    /// Roles to assign by organization ID
    pub roles: Option<HashMap<String, Vec<String>>>,
}

/// Token response 
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    /// The issued JWT token
    pub token: String,
    /// Token expiration timestamp
    pub expires_at: u64,
    /// Token ID (jti) for revocation
    pub token_id: Option<String>,
}

/// Federation admin role required to issue tokens
pub const ROLE_FEDERATION_ADMIN: &str = "federation_admin";
/// Cooperative operator role for economic operations
pub const ROLE_COOP_OPERATOR: &str = "coop_operator";
/// Community official role for governance actions
pub const ROLE_COMMUNITY_OFFICIAL: &str = "community_official";

/// Issue a new JWT token for a user with specified scopes
pub fn issue_token(
    req: &TokenIssueRequest, 
    issuer: Option<String>,
    config: &JwtConfig,
) -> Result<TokenResponse, AuthError> {
    // Calculate expiration time
    let now = Utc::now();
    let expires_in = req.expires_in.unwrap_or(3600 * 24); // Default: 24 hours
    let expiration = now + Duration::seconds(expires_in as i64);
    
    // Create the claims with a unique jti
    let claims = Claims {
        sub: req.subject.clone(),
        iss: issuer,
        aud: config.audience.clone(),
        exp: expiration.timestamp() as usize,
        iat: Some(now.timestamp() as usize),
        nbf: None,
        jti: Some(format!("jti-{}", uuid::Uuid::new_v4())),
        federation_ids: req.federation_ids.clone().unwrap_or_default(),
        coop_ids: req.coop_ids.clone().unwrap_or_default(),
        community_ids: req.community_ids.clone().unwrap_or_default(),
        roles: req.roles.clone().unwrap_or_default(),
    };
    
    // Encode the token
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.secret_key.as_bytes()),
    ).map_err(|e| AuthError::Internal(format!("Failed to encode JWT: {}", e)))?;
    
    Ok(TokenResponse {
        token,
        expires_at: expiration.timestamp() as u64,
        token_id: claims.jti,
    })
}

/// Handler that ensures the requesting user has federation admin role
pub async fn ensure_federation_admin(
    auth: AuthenticatedRequest,
    federation_id: &str,
) -> Result<AuthenticatedRequest, AuthError> {
    // Check if the user has federation admin role
    if !auth.claims.has_federation_access(federation_id) {
        return Err(AuthError::UnauthorizedOrgAccess);
    }
    
    if !auth.claims.has_role(federation_id, ROLE_FEDERATION_ADMIN) {
        return Err(AuthError::NotFederationCoordinator);
    }
    
    Ok(auth)
}

/// Handler that ensures the requesting user has cooperative operator role
pub async fn ensure_coop_operator(
    auth: AuthenticatedRequest,
    coop_id: &str,
) -> Result<AuthenticatedRequest, AuthError> {
    // Check if the user has cooperative access
    if !auth.claims.has_coop_access(coop_id) {
        return Err(AuthError::UnauthorizedOrgAccess);
    }
    
    // Check if they have the operator role
    if !auth.claims.has_role(coop_id, ROLE_COOP_OPERATOR) {
        return Err(AuthError::NotCoopOperator);
    }
    
    Ok(auth)
}

/// Handler that ensures the requesting user has community official role
pub async fn ensure_community_official(
    auth: AuthenticatedRequest,
    community_id: &str,
) -> Result<AuthenticatedRequest, AuthError> {
    // Check if the user has community access
    if !auth.claims.has_community_access(community_id) {
        return Err(AuthError::UnauthorizedOrgAccess);
    }
    
    // Check if they have the official role
    if !auth.claims.has_role(community_id, ROLE_COMMUNITY_OFFICIAL) {
        return Err(AuthError::NotCommunityOfficial);
    }
    
    Ok(auth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    #[test]
    fn test_scope_claims_access_checks() {
        // Create test claims
        let mut claims = Claims {
            sub: "did:icn:user123".to_string(),
            iss: Some("icn-auth-service".to_string()),
            aud: None,
            exp: 2000000000, // Some time in the future
            iat: None,
            nbf: None,
            jti: None,
            federation_ids: vec!["fed1".to_string(), "fed2".to_string()],
            coop_ids: vec!["coop1".to_string(), "coop2".to_string()],
            community_ids: vec!["comm1".to_string()],
            roles: HashMap::new(),
        };
        
        // Add roles
        claims.roles.insert("coop1".to_string(), vec!["admin".to_string()]);
        claims.roles.insert("comm1".to_string(), vec!["member".to_string()]);
        
        let scope_claims: ScopeClaims = claims.into();
        
        // Test federation access
        assert!(scope_claims.has_federation_access("fed1"));
        assert!(scope_claims.has_federation_access("fed2"));
        assert!(!scope_claims.has_federation_access("fed3"));
        
        // Test coop access
        assert!(scope_claims.has_coop_access("coop1"));
        assert!(scope_claims.has_coop_access("coop2"));
        assert!(!scope_claims.has_coop_access("coop3"));
        
        // Test community access
        assert!(scope_claims.has_community_access("comm1"));
        assert!(!scope_claims.has_community_access("comm2"));
        
        // Test role checks
        assert!(scope_claims.has_role("coop1", "admin"));
        assert!(!scope_claims.has_role("coop1", "member"));
        assert!(scope_claims.has_role("comm1", "member"));
        assert!(!scope_claims.has_role("comm1", "admin"));
        
        // Test combined org scope access
        assert!(scope_claims.has_org_scope_access(Some("fed1"), Some("coop1"), Some("comm1")));
        assert!(scope_claims.has_org_scope_access(Some("fed1"), Some("coop1"), None));
        assert!(scope_claims.has_org_scope_access(Some("fed1"), None, None));
        assert!(!scope_claims.has_org_scope_access(Some("fed3"), None, None));
        assert!(!scope_claims.has_org_scope_access(Some("fed1"), Some("coop3"), None));
        assert!(!scope_claims.has_org_scope_access(Some("fed1"), Some("coop1"), Some("comm2")));
    }

    #[test]
    fn test_token_validation() {
        // Create a test JWT config
        let jwt_config = JwtConfig {
            secret_key: "test_secret".to_string(),
            issuer: Some("icn-test".to_string()),
            audience: None,
            validation: Validation::default(),
        };
        
        // Create claims with test data
        let claims = Claims {
            sub: "did:icn:user123".to_string(),
            iss: Some("icn-test".to_string()),
            aud: None,
            exp: 2000000000, // Some time in the future
            iat: None,
            nbf: None,
            jti: None,
            federation_ids: vec!["fed1".to_string()],
            coop_ids: vec!["coop1".to_string()],
            community_ids: vec!["comm1".to_string()],
            roles: HashMap::new(),
        };
        
        // Create a token
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(jwt_config.secret_key.as_bytes()),
        ).unwrap();
        
        // Validate the token
        let validated_claims = validate_token(&token, &jwt_config).unwrap();
        
        // Check that claims were preserved
        assert_eq!(validated_claims.sub, "did:icn:user123");
        assert_eq!(validated_claims.federation_ids, vec!["fed1"]);
        assert_eq!(validated_claims.coop_ids, vec!["coop1"]);
        assert_eq!(validated_claims.community_ids, vec!["comm1"]);
        
        // Check validation with expired token
        let expired_claims = Claims {
            sub: "did:icn:user123".to_string(),
            iss: Some("icn-test".to_string()),
            aud: None,
            exp: 1600000000, // Some time in the past
            iat: None,
            nbf: None,
            jti: None,
            federation_ids: vec![],
            coop_ids: vec![],
            community_ids: vec![],
            roles: HashMap::new(),
        };
        
        let expired_token = encode(
            &Header::default(),
            &expired_claims,
            &EncodingKey::from_secret(jwt_config.secret_key.as_bytes()),
        ).unwrap();
        
        let result = validate_token(&expired_token, &jwt_config);
        assert!(result.is_err());
        
        // Check validation with wrong issuer
        let wrong_issuer_claims = Claims {
            sub: "did:icn:user123".to_string(),
            iss: Some("wrong-issuer".to_string()),
            aud: None,
            exp: 2000000000,
            iat: None,
            nbf: None,
            jti: None,
            federation_ids: vec![],
            coop_ids: vec![],
            community_ids: vec![],
            roles: HashMap::new(),
        };
        
        let wrong_issuer_token = encode(
            &Header::default(),
            &wrong_issuer_claims,
            &EncodingKey::from_secret(jwt_config.secret_key.as_bytes()),
        ).unwrap();
        
        let result = validate_token(&wrong_issuer_token, &jwt_config);
        assert!(result.is_err());
    }
} 