// Integration tests for WebSocket events with PostgreSQL ledger

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use futures_util::{SinkExt, StreamExt};
use icn_agoranet::{
    app::create_app,
    ledger::{self, PostgresLedgerStore},
    models::{
        EntityRef, EntityType, Transfer, TransferRequest, TransferResponse,
    },
    auth::{JwtConfig, Claims, Role, create_jwt},
    handlers::Db,
    websocket::WebSocketState,
};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use uuid::Uuid;
use std::time;
use std::env;
use std::collections::HashMap;

// Database URL for the test database
const TEST_DB_URL: &str = "postgres://postgres:postgres@localhost:5432/icn_ledger_test";

// JWT secret for testing
const JWT_SECRET: &str = "test_jwt_secret_for_integration_tests";

// Test federation ID
const TEST_FEDERATION_ID: &str = "test_federation";

// Test roles
const FEDERATION_ADMIN_ROLE: &str = "federation_admin";

/// Setup function to initialize a unique test database schema
async fn setup_test_db() -> (PgPool, String) {
    // Generate a unique schema name using UUID
    let schema_name = format!("test_{}", Uuid::new_v4().simple());
    
    // Create a connection pool to the PostgreSQL database
    let pool = PgPoolOptions::new()
        .max_connections(3)
        .connect(TEST_DB_URL)
        .await
        .expect("Failed to connect to test database");
    
    // Create a unique schema for this test
    sqlx::query(&format!("CREATE SCHEMA {}", schema_name))
        .execute(&pool)
        .await
        .expect("Failed to create test schema");
        
    // Set the search path to use this schema
    sqlx::query(&format!("SET search_path TO {}", schema_name))
        .execute(&pool)
        .await
        .expect("Failed to set search path");
    
    // Run migrations within this schema
    sqlx::migrate!("./src/ledger/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    
    // Store schema name in the connection
    sqlx::query(&format!("SET app.test_schema = '{}'", schema_name))
        .execute(&pool)
        .await
        .expect("Failed to set schema metadata");
    
    (pool, schema_name)
}

/// Cleanup function to drop the schema after test completion
async fn cleanup_test_db(pool: &PgPool, schema_name: &str) {
    // Drop the schema and all its objects
    sqlx::query(&format!("DROP SCHEMA IF EXISTS {} CASCADE", schema_name))
        .execute(pool)
        .await
        .expect("Failed to drop test schema");
}

/// Initialize the ledger store for testing
async fn create_test_ledger_store() -> (PostgresLedgerStore, PgPool, String) {
    let (pool, schema_name) = setup_test_db().await;
    
    // Set search path for this connection
    sqlx::query(&format!("SET search_path TO {}", schema_name))
        .execute(&pool)
        .await
        .expect("Failed to set search path");
    
    (PostgresLedgerStore::new(pool.clone()), pool, schema_name)
}

/// Create a test app with the PostgreSQL ledger store
async fn create_test_app() -> (String, JoinHandle<()>, Db, Arc<JwtConfig>, Arc<WebSocketState>, PgPool, String) {
    // Create the ledger store
    let (ledger_store, pool, schema_name) = create_test_ledger_store().await;
    
    // Initialize in-memory store with the ledger
    let mut store = icn_agoranet::handlers::InMemoryStore::new();
    store.set_ledger(Arc::new(ledger_store));
    let db: Db = Arc::new(RwLock::new(store));
    
    // Initialize WebSocket state
    let ws_state = Arc::new(WebSocketState::new());
    
    // Initialize JWT config
    let jwt_config = Arc::new(JwtConfig {
        secret_key: JWT_SECRET.to_string(),
        issuer: Some("test_issuer".to_string()),
        audience: None,
        validation: jsonwebtoken::Validation::default(),
    });
    
    // Create a token revocation store
    let revocation_store = Arc::new(icn_agoranet::auth::revocation::InMemoryRevocationStore::new());
    
    // Create the app state tuple
    let app_state = (db.clone(), ws_state.clone(), jwt_config.clone(), revocation_store);
    
    // Create the app
    let app = create_app(app_state);
    
    // Start the server on a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://{}", addr);
    
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    
    // Wait for the server to start
    tokio::time::sleep(time::Duration::from_millis(100)).await;
    
    (server_url, handle, db, jwt_config, ws_state, pool, schema_name)
}

/// Create a JWT token for testing
fn create_test_token(
    subject: &str,
    roles: Vec<String>,
    federation_id: Option<String>,
    coop_id: Option<String>,
    community_id: Option<String>,
    jwt_config: &JwtConfig,
) -> String {
    // Create roles map
    let mut roles_map = HashMap::new();
    
    // Add federation roles if federation ID is provided
    if let Some(ref fed_id) = federation_id {
        roles_map.insert(fed_id.clone(), roles.clone());
    }
    
    // Add cooperative roles if cooperative ID is provided
    if let Some(ref cid) = coop_id {
        roles_map.insert(cid.clone(), roles.clone());
    }
    
    // Add community roles if community ID is provided
    if let Some(ref cmid) = community_id {
        roles_map.insert(cmid.clone(), roles.clone());
    }
    
    // Prepare federation IDs, coop IDs, community IDs
    let federation_ids = federation_id.into_iter().collect::<Vec<_>>();
    let coop_ids = coop_id.into_iter().collect::<Vec<_>>();
    let community_ids = community_id.into_iter().collect::<Vec<_>>();
    
    // Create claims
    let claims = Claims {
        sub: subject.to_string(),
        iss: Some("test_issuer".to_string()),
        aud: None,
        exp: (Utc::now() + Duration::hours(1)).timestamp() as usize,
        iat: Some(Utc::now().timestamp() as usize),
        nbf: None,
        jti: Some(Uuid::new_v4().to_string()),
        federation_ids,
        coop_ids,
        community_ids,
        roles: roles_map,
    };
    
    // Create JWT
    create_jwt(&claims, &jwt_config.secret_key)
        .expect("Failed to create JWT token")
}

/// Initialize test data in the ledger store
async fn init_test_data(db: &Db) {
    // Access the store
    let store = db.read().unwrap();
    
    // Get the ledger using the getter method
    let ledger = store.get_ledger().unwrap();
    
    // Release the read lock
    drop(store);
    
    // Get the actual ledger implementation
    let ledger = ledger.write().unwrap();
    
    // Define test entities
    let federation = EntityRef {
        entity_type: EntityType::Federation,
        id: TEST_FEDERATION_ID.to_string(),
    };
    
    let user1 = EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    };
    
    let user2 = EntityRef {
        entity_type: EntityType::User,
        id: "test_user2".to_string(),
    };
    
    // Ensure entities exist
    ledger.ensure_entity_exists(&federation, TEST_FEDERATION_ID);
    ledger.ensure_entity_exists(&user1, TEST_FEDERATION_ID);
    ledger.ensure_entity_exists(&user2, TEST_FEDERATION_ID);
    
    // Give 100 tokens to user1
    let _ = ledger.process_transfer(Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: TEST_FEDERATION_ID.to_string(),
        from: federation,
        to: user1,
        amount: 100,
        fee: 0,
        initiator: "system".to_string(),
        timestamp: Utc::now(),
        memo: Some("Initial user1 balance".to_string()),
        metadata: None,
    });
}

#[tokio::test]
async fn test_websocket_transfer_events() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config, ws_state, pool, schema_name) = create_test_app().await;
    init_test_data(&db).await;
    
    // Extract the server hostname and port for WebSocket connection
    let server_url_parts: Vec<&str> = server_url.split("://").collect();
    let host_port = server_url_parts[1];
    let ws_url = format!("ws://{}/ws", host_port);
    
    // Create a WebSocket connection
    let (mut ws_stream, _) = connect_async(ws_url).await.expect("Failed to connect to WebSocket");
    
    // Subscribe to user1's channel
    let subscribe_msg = json!({
        "action": "subscribe",
        "channel": "user:test_user1"
    }).to_string();
    
    ws_stream.send(Message::Text(subscribe_msg)).await.expect("Failed to send subscription");
    
    // Create an HTTP client
    let client = Client::new();
    
    // Create a federation admin token
    let token = create_test_token(
        "admin",
        vec![FEDERATION_ADMIN_ROLE.to_string()],
        Some(TEST_FEDERATION_ID.to_string()),
        None,
        None,
        &jwt_config,
    );
    
    // Create a transfer request
    let transfer_request = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::User,
            id: "test_user1".to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::User,
            id: "test_user2".to_string(),
        },
        amount: 10,
        memo: Some("WebSocket test transfer".to_string()),
        metadata: None,
    };
    
    // Send the transfer request in a separate task to avoid blocking
    let client_clone = client.clone();
    let server_url_clone = server_url.clone();
    let token_clone = token.clone();
    let transfer_future = tokio::spawn(async move {
        client_clone
            .post(format!("{}/api/v1/federation/{}/transfers", server_url_clone, TEST_FEDERATION_ID))
            .header("Authorization", format!("Bearer {}", token_clone))
            .json(&transfer_request)
            .send()
            .await
            .expect("Failed to send transfer request")
    });
    
    // Wait for the WebSocket message
    let mut transfer_event_received = false;
    let mut balance_update_received = false;
    
    // Create a timeout for the test
    let timeout = tokio::time::sleep(time::Duration::from_secs(5));
    tokio::pin!(timeout);
    
    loop {
        tokio::select! {
            Some(msg) = ws_stream.next() => {
                match msg {
                    Ok(Message::Text(text)) => {
                        let json: Value = serde_json::from_str(&text).expect("Failed to parse WebSocket message");
                        println!("Received WebSocket message: {}", text);
                        
                        if let Some(event_type) = json.get("event").and_then(|e| e.as_str()) {
                            if event_type == "transfer" {
                                transfer_event_received = true;
                                
                                // Verify the transfer details
                                let transfer = &json["data"]["transfer"];
                                assert_eq!(transfer["from"]["id"].as_str().unwrap(), "test_user1");
                                assert_eq!(transfer["to"]["id"].as_str().unwrap(), "test_user2");
                                assert_eq!(transfer["amount"].as_u64().unwrap(), 10);
                            }
                            else if event_type == "balance_updated" {
                                balance_update_received = true;
                                
                                // Verify the balance update
                                let balance = json["data"]["balance"].as_u64().unwrap();
                                assert_eq!(balance, 90); // 100 - 10
                            }
                        }
                        
                        // If we've received both events, we can break out of the loop
                        if transfer_event_received && balance_update_received {
                            break;
                        }
                    },
                    _ => {
                        println!("Received non-text message");
                    }
                }
            },
            _ = &mut timeout => {
                panic!("Timed out waiting for WebSocket messages");
            }
        }
    }
    
    // Wait for the transfer to complete
    let response = transfer_future.await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Verify we received the expected events
    assert!(transfer_event_received, "Did not receive transfer event");
    assert!(balance_update_received, "Did not receive balance update event");
    
    // Clean up
    ws_stream.send(Message::Close(None)).await.expect("Failed to close WebSocket");
    handle.abort();
    cleanup_test_db(&pool, &schema_name).await;
} 