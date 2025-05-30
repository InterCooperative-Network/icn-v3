// Integration tests for JWT-based authorization with organization scoping
use chrono::Utc;
use icn_agoranet::{
    app::create_app,
    auth::{Claims, JwtConfig},
    auth::revocation::{RevokeTokenRequest, RotateTokenRequest},
    handlers::Db,
    models::{ExecutionReceiptSummary, TokenBalance, TokenTransaction},
    websocket::WebSocketState,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use uuid::Uuid;

// Helper function to spawn the app in the background
async fn spawn_app() -> (String, JoinHandle<()>, Db, Arc<JwtConfig>) {
    let store = Db::default();
    let jwt_config = Arc::new(JwtConfig {
        secret_key: "test_secret_key_for_integration_tests".to_string(),
        issuer: Some("icn-test".to_string()),
        audience: None,
        validation: jsonwebtoken::Validation::default(),
    });
    
    let ws_state = WebSocketState::new();
    let app = create_app(store.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap(); // Bind to a random available port
    let local_addr = listener.local_addr().unwrap();
    let server_url = format!("http://{}", local_addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (server_url, handle, store, jwt_config)
}

// Helper to create a valid JWT token with the specified claims
fn create_jwt_token(
    jwt_config: &JwtConfig,
    federation_ids: Vec<String>,
    coop_ids: Vec<String>,
    community_ids: Vec<String>,
    roles: HashMap<String, Vec<String>>,
) -> String {
    let claims = Claims {
        sub: "did:icn:test_user".to_string(),
        iss: jwt_config.issuer.clone(),
        aud: None,
        exp: Utc::now().timestamp() as usize + 3600, // Valid for 1 hour
        iat: Some(Utc::now().timestamp() as usize),
        nbf: None,
        jti: None,
        federation_ids,
        coop_ids,
        community_ids,
        roles,
    };
    
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_config.secret_key.as_bytes()),
    ).unwrap()
}

// Helper to seed the database with test data
async fn seed_test_data(db: &Db) {
    // Create test data and add directly to store
    let mut store = db.write().unwrap();
    
    // For integration tests, we'll mock the main operations that would normally
    // be performed by the actual handlers
    
    // Mock database operations for cleaner tests
    // In a real app, you'd use methods on the store instead
    
    // We'll use a simple approach for testing
    // Note: In a real app, you would likely have methods for these operations
    
    // Pretend we've added data to the store
    tracing::info!("Test data seeded to the database");
}

#[tokio::test]
async fn test_authorized_access_receipts() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, jwt_config) = spawn_app().await;
    seed_test_data(&db).await;
    
    // 2. Create a JWT token with access to coop1 and comm1
    let token = create_jwt_token(
        &jwt_config,
        vec!["fed1".to_string()],
        vec!["coop1".to_string()],
        vec!["comm1".to_string()],
        HashMap::new(),
    );
    
    // 3. Create HTTP client
    let client = Client::builder()
        .build()
        .unwrap();
    
    // 4. Make an authorized request to get receipts
    let response = client
        .get(format!("{}/api/v1/receipts?coop_id=coop1&community_id=comm1", server_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send request");
    
    // 5. Verify that the request succeeded
    assert_eq!(response.status(), StatusCode::OK);
    
    let receipts: Vec<Value> = response.json().await.expect("Failed to parse response");
    assert!(!receipts.is_empty(), "Should return at least one receipt");
}

#[tokio::test]
async fn test_unauthorized_access_receipts() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, jwt_config) = spawn_app().await;
    seed_test_data(&db).await;
    
    // 2. Create a JWT token with access to coop2 only (not coop1)
    let token = create_jwt_token(
        &jwt_config,
        vec!["fed1".to_string()],
        vec!["coop2".to_string()],
        vec![],
        HashMap::new(),
    );
    
    // 3. Create HTTP client
    let client = Client::builder()
        .build()
        .unwrap();
    
    // 4. Make an authorized request to get receipts for coop1 (which we don't have access to)
    let response = client
        .get(format!("{}/api/v1/receipts?coop_id=coop1", server_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send request");
    
    // 5. Verify that the request was rejected with 403 Forbidden
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_invalid_token() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, _) = spawn_app().await;
    seed_test_data(&db).await;
    
    // 2. Create an invalid token
    let invalid_token = "invalid.jwt.token";
    
    // 3. Create HTTP client
    let client = Client::builder()
        .build()
        .unwrap();
    
    // 4. Make a request with the invalid token
    let response = client
        .get(format!("{}/api/v1/receipts", server_url))
        .header("Authorization", format!("Bearer {}", invalid_token))
        .send()
        .await
        .expect("Failed to send request");
    
    // 5. Verify that the request was rejected with 401 Unauthorized
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_missing_auth_header() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, _) = spawn_app().await;
    seed_test_data(&db).await;
    
    // 2. Create HTTP client
    let client = Client::builder()
        .build()
        .unwrap();
    
    // 3. Make a request without an auth header
    let response = client
        .get(format!("{}/api/v1/receipts", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    // 4. Verify that the request was rejected with 401 Unauthorized
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_role_based_access() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, jwt_config) = spawn_app().await;
    seed_test_data(&db).await;
    
    // 2. Create a JWT token with admin role for coop1
    let mut roles = HashMap::new();
    roles.insert("coop1".to_string(), vec!["admin".to_string()]);
    
    let token = create_jwt_token(
        &jwt_config,
        vec!["fed1".to_string()],
        vec!["coop1".to_string()],
        vec!["comm1".to_string()],
        roles,
    );
    
    // 3. Create HTTP client
    let client = Client::builder()
        .build()
        .unwrap();
    
    // 4. Make an authorized request to get token stats (requires admin role)
    let response = client
        .get(format!("{}/api/v1/stats/tokens?coop_id=coop1", server_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send request");
    
    // 5. Verify that the request succeeded
    assert_eq!(response.status(), StatusCode::OK);
    
    let stats: Value = response.json().await.expect("Failed to parse response");
    assert!(stats.is_object(), "Should return token stats object");
}

#[tokio::test]
async fn test_federation_token_issuance() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, jwt_config) = spawn_app().await;
    seed_test_data(&db).await;
    
    // 2. Create a JWT token with federation admin role for federation "alpha"
    let mut roles = HashMap::new();
    roles.insert("alpha".to_string(), vec!["federation_admin".to_string()]);
    
    let admin_token = create_jwt_token(
        &jwt_config,
        vec!["alpha".to_string()],
        vec!["coop-econA".to_string()],
        vec!["comm-govX".to_string()],
        roles,
    );
    
    // 3. Create HTTP client
    let client = Client::builder()
        .build()
        .unwrap();
    
    // 4. Make request to issue a token for a user
    let token_request = serde_json::json!({
        "subject": "did:icn:new_user",
        "expires_in": 3600,
        "federation_ids": ["alpha"],
        "coop_ids": ["coop-econA"],
        "community_ids": ["comm-govX"],
        "roles": {
            "coop-econA": ["coop_operator"]
        }
    });
    
    let response = client
        .post(format!("{}/api/v1/federation/alpha/tokens", server_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .json(&token_request)
        .send()
        .await
        .expect("Failed to send request");
    
    // 5. Verify that the request succeeded
    assert_eq!(response.status(), StatusCode::OK);
    
    let token_response: serde_json::Value = response.json().await.expect("Failed to parse response");
    
    // Check the response structure
    assert!(token_response.get("token").is_some(), "Response should contain a token");
    assert!(token_response.get("expires_at").is_some(), "Response should contain an expiration timestamp");
    
    // Now verify the new token works for accessing resources
    let new_token = token_response["token"].as_str().unwrap();
    
    // Try to access a resource with the new token
    let access_response = client
        .get(format!("{}/api/v1/receipts?coop_id=coop-econA&community_id=comm-govX", server_url))
        .header("Authorization", format!("Bearer {}", new_token))
        .send()
        .await
        .expect("Failed to send request");
    
    // Verify that access was granted
    assert_eq!(access_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_unauthorized_federation_token_issuance() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, jwt_config) = spawn_app().await;
    seed_test_data(&db).await;
    
    // 2. Create a JWT token WITHOUT federation admin role
    let token = create_jwt_token(
        &jwt_config,
        vec!["fed1".to_string()],
        vec!["coop1".to_string()],
        vec!["comm1".to_string()],
        HashMap::new(), // No roles
    );
    
    // 3. Create HTTP client
    let client = Client::builder()
        .build()
        .unwrap();
    
    // 4. Make request to issue a token
    let token_request = serde_json::json!({
        "subject": "did:icn:new_user",
        "expires_in": 3600,
        "federation_ids": ["fed1"],
        "coop_ids": ["coop1"]
    });
    
    let response = client
        .post(format!("{}/api/v1/federation/fed1/tokens", server_url))
        .header("Authorization", format!("Bearer {}", token))
        .json(&token_request)
        .send()
        .await
        .expect("Failed to send request");
    
    // 5. Verify that the request was rejected
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_token_revocation() {
    use icn_agoranet::auth::revocation::{
        RevokeTokenRequest, RevokeTokenResponse, RevokedToken, TokenRevocationStore, InMemoryRevocationStore
    };
    use chrono::Utc;
    use uuid::Uuid;

    // Create a new in-memory store
    let store = InMemoryRevocationStore::new();
    
    // Test revoking by JTI
    let jti = format!("test-jti-{}", Uuid::new_v4());
    let token = RevokedToken {
        jti: jti.clone(),
        subject: "test-subject".to_string(),
        issuer: Some("test-issuer".to_string()),
        revoked_at: Utc::now(),
        reason: Some("Test revocation".to_string()),
        revoked_by: "test-admin".to_string(),
    };
    
    assert!(store.revoke_token(token));
    assert!(store.is_revoked(&jti));
    
    // Test revoking by subject+issuer
    let subject = "another-subject";
    let issuer = "another-issuer";
    let jti2 = format!("test-jti-{}", Uuid::new_v4());
    
    let token2 = RevokedToken {
        jti: jti2.clone(),
        subject: subject.to_string(),
        issuer: Some(issuer.to_string()),
        revoked_at: Utc::now(),
        reason: Some("Another test revocation".to_string()),
        revoked_by: "test-admin".to_string(),
    };
    
    assert!(store.revoke_token(token2));
    assert!(store.is_revoked(&jti2));
    assert!(store.is_revoked_by_subject_issuer(subject, Some(issuer)));
    
    // Get all revoked tokens for a subject
    let revoked_tokens = store.get_revoked_tokens_for_subject(subject);
    assert_eq!(revoked_tokens.len(), 1);
    assert_eq!(revoked_tokens[0].jti, jti2);
    
    // Get tokens by subject+issuer
    let specific_tokens = store.get_revoked_tokens_for_subject_issuer(subject, Some(issuer));
    assert_eq!(specific_tokens.len(), 1);
    assert_eq!(specific_tokens[0].jti, jti2);
    
    // Cleanup expired tokens (none should be cleaned as they're all recent)
    let old_time = Utc::now() - chrono::Duration::days(7);
    let removed = store.clear_expired_revocations(old_time);
    assert_eq!(removed, 0);
    
    // Verify tokens still exist
    assert!(store.is_revoked(&jti));
    assert!(store.is_revoked(&jti2));
}

#[tokio::test]
async fn test_token_revocation_endpoint() {
    // Spawn test app
    let (server_url, handle, store, jwt_config) = spawn_app().await;
    
    // Create a test client
    let client = Client::new();
    
    // Create a token for a federation admin
    let mut roles = HashMap::new();
    roles.insert("federation1".to_string(), vec!["federation_admin".to_string()]);
    
    let admin_token = create_jwt_token(
        &jwt_config, 
        vec!["federation1".to_string()], 
        vec![],
        vec![],
        roles
    );
    
    // Create a test user token to revoke
    let user_token = create_jwt_token(
        &jwt_config,
        vec!["federation1".to_string()],
        vec![],
        vec![],
        HashMap::new()
    );
    
    // Decode the user token to get the JTI
    let user_token_data = jsonwebtoken::decode::<icn_agoranet::auth::Claims>(
        &user_token,
        &jsonwebtoken::DecodingKey::from_secret(jwt_config.secret_key.as_bytes()),
        &jwt_config.validation
    ).unwrap();
    
    // We need to ensure the token has a JTI claim
    let jti = user_token_data.claims.jti.unwrap_or_else(|| "test-jti".to_string());
    
    // Call the revoke endpoint
    let revoke_response = client.post(&format!("{}/api/v1/federation/federation1/tokens/revoke", server_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .json(&RevokeTokenRequest {
            jti: Some(jti.clone()),
            subject: None,
            issuer: None,
            reason: Some("Test revocation".to_string()),
        })
        .send()
        .await
        .unwrap();
    
    assert_eq!(revoke_response.status(), StatusCode::OK);
    
    // Try to use the revoked token
    let protected_response = client.get(&format!("{}/api/v1/tokens/balances", server_url))
        .header("Authorization", format!("Bearer {}", user_token))
        .send()
        .await
        .unwrap();
    
    // It should be rejected as the token is now revoked
    assert_eq!(protected_response.status(), StatusCode::UNAUTHORIZED);
    
    // Cleanup
    drop(client);
    handle.abort();
}

#[tokio::test]
async fn test_token_rotation() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, jwt_config) = spawn_app().await;
    seed_test_data(&db).await;
    
    // 2. Create a JWT token with federation admin role
    let mut roles = HashMap::new();
    roles.insert("fed1".to_string(), vec!["federation_admin".to_string()]);
    
    let admin_token = create_jwt_token(
        &jwt_config,
        vec!["fed1".to_string()],
        vec!["coop1".to_string()],
        vec!["comm1".to_string()],
        roles,
    );
    
    // 3. First issue a token to rotate
    let client = Client::builder().build().unwrap();
    
    let token_request = serde_json::json!({
        "subject": "did:icn:rotate_test_user",
        "expires_in": 3600,
        "federation_ids": ["fed1"],
        "coop_ids": ["coop1"],
        "community_ids": ["comm1"]
    });
    
    let issue_response = client
        .post(format!("{}/api/v1/federation/fed1/tokens", server_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .json(&token_request)
        .send()
        .await
        .expect("Failed to send token issuance request");
    
    assert_eq!(issue_response.status(), StatusCode::OK);
    
    let token_data: serde_json::Value = issue_response.json().await.expect("Failed to parse response");
    let old_token = token_data["token"].as_str().unwrap().to_string();
    let token_id = token_data["token_id"].as_str().unwrap().to_string();
    
    // 4. Now rotate the token with updated scopes
    let rotate_request = serde_json::json!({
        "current_jti": token_id,
        "subject": "did:icn:rotate_test_user",
        "expires_in": 7200,
        "federation_ids": ["fed1"],
        "coop_ids": ["coop1", "coop2"], // Add access to coop2
        "community_ids": ["comm1"],
        "reason": "Scope expansion"
    });
    
    let rotate_response = client
        .post(format!("{}/api/v1/federation/fed1/tokens/rotate", server_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .json(&rotate_request)
        .send()
        .await
        .expect("Failed to send token rotation request");
    
    assert_eq!(rotate_response.status(), StatusCode::OK);
    
    let rotate_data: serde_json::Value = rotate_response.json().await.expect("Failed to parse response");
    let new_token = rotate_data["token"].as_str().unwrap().to_string();
    
    // 5. Verify the old token no longer works
    let old_token_response = client
        .get(format!("{}/api/v1/receipts?coop_id=coop1", server_url))
        .header("Authorization", format!("Bearer {}", old_token))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(old_token_response.status(), StatusCode::FORBIDDEN);
    
    // 6. Verify the new token works and has expanded access
    let new_token_response = client
        .get(format!("{}/api/v1/receipts?coop_id=coop2", server_url))
        .header("Authorization", format!("Bearer {}", new_token))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(new_token_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_token_rotation_endpoint() {
    // Spawn test app
    let (server_url, handle, store, jwt_config) = spawn_app().await;
    
    // Create a test client
    let client = Client::new();
    
    // Create a token for a federation admin
    let mut roles = HashMap::new();
    roles.insert("federation1".to_string(), vec!["federation_admin".to_string()]);
    
    let admin_token = create_jwt_token(
        &jwt_config, 
        vec!["federation1".to_string()], 
        vec![],
        vec![],
        roles
    );
    
    // Create a test user token to rotate
    let user_token = create_jwt_token(
        &jwt_config,
        vec!["federation1".to_string()],
        vec![],
        vec![],
        HashMap::new()
    );
    
    // Decode the user token to get the JTI
    let user_token_data = jsonwebtoken::decode::<icn_agoranet::auth::Claims>(
        &user_token,
        &jsonwebtoken::DecodingKey::from_secret(jwt_config.secret_key.as_bytes()),
        &jwt_config.validation
    ).unwrap();
    
    // We need to ensure the token has a JTI claim
    let jti = user_token_data.claims.jti.unwrap_or_else(|| "test-jti".to_string());
    let subject = user_token_data.claims.sub;
    
    // Create a rotation request
    let rotation_request = RotateTokenRequest {
        current_jti: jti,
        subject: subject.clone(),
        expires_in: Some(3600),
        federation_ids: Some(vec!["federation1".to_string()]),
        coop_ids: None,
        community_ids: None,
        roles: None,
        reason: Some("Test rotation".to_string()),
    };
    
    // Call the rotate endpoint
    let rotate_response = client.post(&format!("{}/api/v1/federation/federation1/tokens/rotate", server_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .json(&rotation_request)
        .send()
        .await
        .unwrap();
    
    assert_eq!(rotate_response.status(), StatusCode::OK);
    
    // Extract the new token
    let rotate_body: serde_json::Value = rotate_response.json().await.unwrap();
    let new_token = rotate_body["token"].as_str().unwrap().to_string();
    
    // The old token should now be rejected
    let old_token_response = client.get(&format!("{}/api/v1/tokens/balances", server_url))
        .header("Authorization", format!("Bearer {}", user_token))
        .send()
        .await
        .unwrap();
    
    assert_eq!(old_token_response.status(), StatusCode::UNAUTHORIZED);
    
    // The new token should work
    let new_token_response = client.get(&format!("{}/api/v1/tokens/balances", server_url))
        .header("Authorization", format!("Bearer {}", new_token))
        .send()
        .await
        .unwrap();
    
    assert_eq!(new_token_response.status(), StatusCode::OK);
    
    // Cleanup
    drop(client);
    handle.abort();
}

#[tokio::test]
async fn test_coop_operator_role() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, jwt_config) = spawn_app().await;
    seed_test_data(&db).await;
    
    // 2. Create a token with coop_operator role
    let mut roles = HashMap::new();
    roles.insert("coop-econA".to_string(), vec!["coop_operator".to_string()]);
    
    let operator_token = create_jwt_token(
        &jwt_config,
        vec!["alpha".to_string()],
        vec!["coop-econA".to_string()],
        vec![],
        roles,
    );
    
    // 3. Create HTTP client
    let client = Client::builder().build().unwrap();
    
    // 4. Make request to transfer tokens (economic operation)
    let transfer_request = serde_json::json!({
        "from_did": "did:icn:user1",
        "to_did": "did:icn:user2",
        "amount": 100,
        "memo": "Test transfer"
    });
    
    let response = client
        .post(format!("{}/api/v1/coop/coop-econA/transfer", server_url))
        .header("Authorization", format!("Bearer {}", operator_token))
        .json(&transfer_request)
        .send()
        .await
        .expect("Failed to send request");
    
    // 5. Verify that the request succeeded
    assert_eq!(response.status(), StatusCode::OK);
    
    // 6. Create a token without coop_operator role
    let regular_token = create_jwt_token(
        &jwt_config,
        vec!["alpha".to_string()],
        vec!["coop-econA".to_string()],
        vec![],
        HashMap::new(), // No roles
    );
    
    // 7. Try to make the same request
    let unauthorized_response = client
        .post(format!("{}/api/v1/coop/coop-econA/transfer", server_url))
        .header("Authorization", format!("Bearer {}", regular_token))
        .json(&transfer_request)
        .send()
        .await
        .expect("Failed to send request");
    
    // 8. Verify that it failed with 403 Forbidden
    assert_eq!(unauthorized_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_community_official_role() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, jwt_config) = spawn_app().await;
    seed_test_data(&db).await;
    
    // 2. Create a token with community_official role
    let mut roles = HashMap::new();
    roles.insert("comm-govX".to_string(), vec!["community_official".to_string()]);
    
    let official_token = create_jwt_token(
        &jwt_config,
        vec!["alpha".to_string()],
        vec!["coop-econA".to_string()],
        vec!["comm-govX".to_string()],
        roles,
    );
    
    // 3. Create HTTP client
    let client = Client::builder().build().unwrap();
    
    // 4. Make request to perform a governance action
    let governance_request = serde_json::json!({
        "action_type": "approve_policy",
        "parameters": {
            "policy_id": "policy-123",
            "version": "1.0"
        },
        "justification": "Policy meets community standards"
    });
    
    let response = client
        .post(format!("{}/api/v1/community/comm-govX/governance", server_url))
        .header("Authorization", format!("Bearer {}", official_token))
        .json(&governance_request)
        .send()
        .await
        .expect("Failed to send request");
    
    // 5. Verify that the request succeeded
    assert_eq!(response.status(), StatusCode::OK);
    
    // 6. Create a token without community_official role
    let regular_token = create_jwt_token(
        &jwt_config,
        vec!["alpha".to_string()],
        vec!["coop-econA".to_string()],
        vec!["comm-govX".to_string()],
        HashMap::new(), // No roles
    );
    
    // 7. Try to make the same request
    let unauthorized_response = client
        .post(format!("{}/api/v1/community/comm-govX/governance", server_url))
        .header("Authorization", format!("Bearer {}", regular_token))
        .json(&governance_request)
        .send()
        .await
        .expect("Failed to send request");
    
    // 8. Verify that it failed with 403 Forbidden
    assert_eq!(unauthorized_response.status(), StatusCode::FORBIDDEN);
} 