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

// Database URL for the test database
const TEST_DB_URL: &str = "postgres://postgres:postgres@localhost:5432/icn_ledger_test";

// JWT secret for testing
const JWT_SECRET: &str = "test_jwt_secret_for_integration_tests";

// Test federation ID
const TEST_FEDERATION_ID: &str = "test_federation";

// Test roles
const FEDERATION_ADMIN_ROLE: &str = "federation_admin";

/// Setup function to initialize the database for testing
async fn setup_test_db() -> PgPool {
    // Create a connection pool to the PostgreSQL database
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(TEST_DB_URL)
        .await
        .expect("Failed to connect to test database");
    
    // Drop all tables from previous test runs
    sqlx::query("DROP SCHEMA public CASCADE; CREATE SCHEMA public;")
        .execute(&pool)
        .await
        .expect("Failed to reset database schema");
    
    // Run migrations to create tables
    sqlx::migrate!("./src/ledger/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    
    pool
}

/// Initialize the ledger store for testing
async fn create_test_ledger_store() -> PostgresLedgerStore {
    let pool = setup_test_db().await;
    PostgresLedgerStore::new(pool)
}

/// Create a test app with the PostgreSQL ledger store
async fn create_test_app() -> (String, JoinHandle<()>, Db, Arc<JwtConfig>, Arc<WebSocketState>) {
    // Create the ledger store
    let ledger_store = Arc::new(create_test_ledger_store().await);
    
    // Initialize in-memory store with the ledger
    let mut store = icn_agoranet::handlers::InMemoryStore::new();
    store.set_ledger(ledger_store);
    let db: Db = Arc::new(RwLock::new(store));
    
    // Initialize WebSocket state
    let ws_state = Arc::new(WebSocketState::new());
    
    // Initialize JWT config
    let jwt_config = Arc::new(JwtConfig::new(JWT_SECRET.to_string()));
    
    // Create a token revocation store
    let revocation_store = Arc::new(icn_agoranet::auth::revocation::InMemoryRevocationStore::new());
    
    // Create the app
    let app = create_app(
        db.clone(),
        ws_state.clone(),
        jwt_config.clone(),
        revocation_store,
    );
    
    // Start the server on a random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://{}", addr);
    
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    
    // Wait for the server to start
    tokio::time::sleep(time::Duration::from_millis(100)).await;
    
    (server_url, handle, db, jwt_config, ws_state)
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
    // Create claims
    let mut claims = Claims {
        sub: subject.to_string(),
        roles,
        exp: (Utc::now() + Duration::hours(1)).timestamp() as usize,
        iat: Utc::now().timestamp() as usize,
        iss: "test_issuer".to_string(),
        federation_id,
        cooperative_id: coop_id,
        community_id: community_id,
    };
    
    // Create JWT
    create_jwt(&claims, &jwt_config.secret)
        .expect("Failed to create JWT token")
}

/// Initialize test data in the ledger store
async fn init_test_data(db: &Db) {
    // Access the store
    let store = db.read().unwrap();
    
    // Get the ledger
    let ledger = store.ledger.clone().unwrap();
    
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
    let (server_url, handle, db, jwt_config, ws_state) = create_test_app().await;
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
} 