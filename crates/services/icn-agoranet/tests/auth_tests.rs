// Integration tests for JWT-based authorization with organization scoping
use chrono::Utc;
use icn_agoranet::{
    app::create_app,
    auth::{Claims, JwtConfig},
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
    // Create test receipts
    let receipt = ExecutionReceiptSummary {
        cid: format!("bafy-test-{}", Uuid::new_v4()),
        executor: "did:icn:executor1".to_string(),
        resource_usage: HashMap::from([
            ("CPU".to_string(), (100)),
            ("Memory".to_string(), (512)),
        ]),
        timestamp: Utc::now(),
        coop_id: Some("coop1".to_string()),
        community_id: Some("comm1".to_string()),
    };
    
    db.create_receipt(receipt).await;
    
    // Create test token balances
    let balance = TokenBalance {
        did: "did:icn:user1".to_string(),
        amount: 1000,
        coop_id: Some("coop1".to_string()),
        community_id: Some("comm1".to_string()),
    };
    
    db.create_token_balance(balance).await;
    
    // Create test token transactions
    let tx = TokenTransaction {
        id: format!("tx-test-{}", Uuid::new_v4()),
        from_did: "did:icn:treasury".to_string(),
        to_did: "did:icn:user1".to_string(),
        amount: 1000,
        operation: "mint".to_string(),
        timestamp: Utc::now(),
        from_coop_id: None,
        from_community_id: None,
        to_coop_id: Some("coop1".to_string()),
        to_community_id: Some("comm1".to_string()),
    };
    
    db.create_token_transaction(tx).await;
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
    
    // 2. Create a JWT token with federation admin role for federation "fed1"
    let mut roles = HashMap::new();
    roles.insert("fed1".to_string(), vec!["federation_admin".to_string()]);
    
    let admin_token = create_jwt_token(
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
    
    // 4. Make request to issue a token for a user
    let token_request = serde_json::json!({
        "subject": "did:icn:new_user",
        "expires_in": 3600,
        "federation_ids": ["fed1"],
        "coop_ids": ["coop1"],
        "community_ids": ["comm1"],
        "roles": {
            "coop1": ["member"]
        }
    });
    
    let response = client
        .post(format!("{}/api/v1/federation/fed1/tokens", server_url))
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
        .get(format!("{}/api/v1/receipts?coop_id=coop1&community_id=comm1", server_url))
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
    
    // 3. First issue a token to revoke
    let client = Client::builder().build().unwrap();
    
    let token_request = serde_json::json!({
        "subject": "did:icn:revoke_test_user",
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
    let token_to_revoke = token_data["token"].as_str().unwrap().to_string();
    let token_id = token_data["token_id"].as_str().unwrap().to_string();
    
    // 4. Verify the token works before revocation
    let access_response = client
        .get(format!("{}/api/v1/receipts?coop_id=coop1", server_url))
        .header("Authorization", format!("Bearer {}", token_to_revoke))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(access_response.status(), StatusCode::OK);
    
    // 5. Now revoke the token
    let revoke_request = serde_json::json!({
        "jti": token_id,
        "reason": "Test revocation"
    });
    
    let revoke_response = client
        .post(format!("{}/api/v1/federation/fed1/tokens/revoke", server_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .json(&revoke_request)
        .send()
        .await
        .expect("Failed to send token revocation request");
    
    assert_eq!(revoke_response.status(), StatusCode::OK);
    
    let revoke_data: serde_json::Value = revoke_response.json().await.expect("Failed to parse response");
    assert_eq!(revoke_data["revoked"].as_bool().unwrap(), true);
    
    // 6. Verify the token no longer works after revocation
    let access_after_revoke = client
        .get(format!("{}/api/v1/receipts?coop_id=coop1", server_url))
        .header("Authorization", format!("Bearer {}", token_to_revoke))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(access_after_revoke.status(), StatusCode::FORBIDDEN);
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