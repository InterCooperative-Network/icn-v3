// Integration tests for the PostgreSQL-backed ledger APIs
//
// These tests verify the functionality of the ledger APIs with a real PostgreSQL database.
// The tests use a Docker container to run PostgreSQL, so Docker must be installed and running.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use icn_agoranet::{
    app::create_app,
    ledger::{self, PostgresLedgerStore, LedgerStore, LedgerStats},
    models::{
        EntityRef, EntityType, Transfer, TransferRequest, TransferResponse,
        BatchTransferRequest, BatchTransferResponse,
    },
    auth::{JwtConfig, Claims, create_jwt, revocation::InMemoryRevocationStore},
    handlers::Db,
    websocket::WebSocketState,
    transfers::TransferQuery,
};
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use sqlx::{PgPool, postgres::PgPoolOptions};
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
const COOP_OPERATOR_ROLE: &str = "coop_operator";
const COMMUNITY_OFFICIAL_ROLE: &str = "community_official";
const USER_ROLE: &str = "user";

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
async fn create_test_app() -> (String, JoinHandle<()>, Db, Arc<JwtConfig>, PgPool, String) {
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
    let revocation_store = Arc::new(InMemoryRevocationStore::new());
    
    // Create the app state tuple
    let app_state = (db.clone(), ws_state.clone(), jwt_config.clone(), revocation_store);
    
    // Create the app with state
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
    
    (server_url, handle, db, jwt_config, pool, schema_name)
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

/// Represents an entity in the ledger
#[derive(Debug, Clone)]
struct TestEntity {
    entity_ref: EntityRef,
    balance: u64,
}

/// Initialize test data in the ledger store
async fn init_test_data(db: &Db) {
    // Create a PostgreSQL ledger store with test data
    let (ledger_store, pool, schema_name) = create_test_ledger_store().await;
    
    // Define test entities
    let federation = EntityRef {
        entity_type: EntityType::Federation,
        id: TEST_FEDERATION_ID.to_string(),
    };
    
    let cooperative = EntityRef {
        entity_type: EntityType::Cooperative,
        id: "test_cooperative".to_string(),
    };
    
    let community = EntityRef {
        entity_type: EntityType::Community,
        id: "test_community".to_string(),
    };
    
    let user1 = EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    };
    
    let user2 = EntityRef {
        entity_type: EntityType::User,
        id: "test_user2".to_string(),
    };
    
    // Ensure entities exist and initialize their balances
    ledger_store.ensure_entity_exists(&federation, TEST_FEDERATION_ID).await
        .expect("Failed to create federation entity");
    ledger_store.ensure_entity_exists(&cooperative, TEST_FEDERATION_ID).await
        .expect("Failed to create cooperative entity");
    ledger_store.ensure_entity_exists(&community, TEST_FEDERATION_ID).await
        .expect("Failed to create community entity");
    ledger_store.ensure_entity_exists(&user1, TEST_FEDERATION_ID).await
        .expect("Failed to create user1 entity");
    ledger_store.ensure_entity_exists(&user2, TEST_FEDERATION_ID).await
        .expect("Failed to create user2 entity");
    
    // Give 1000 tokens to cooperative (from federation)
    let transfer1 = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: TEST_FEDERATION_ID.to_string(),
        from: federation.clone(),
        to: cooperative.clone(),
        amount: 1000,
        fee: 0,
        initiator: "system".to_string(),
        timestamp: Utc::now(),
        memo: Some("Initial cooperative balance".to_string()),
        metadata: None,
    };
    ledger_store.process_transfer(transfer1).await.expect("Failed to transfer to cooperative");
    
    // Give 500 tokens to community (from federation)
    let transfer2 = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: TEST_FEDERATION_ID.to_string(),
        from: federation.clone(),
        to: community.clone(),
        amount: 500,
        fee: 0,
        initiator: "system".to_string(),
        timestamp: Utc::now(),
        memo: Some("Initial community balance".to_string()),
        metadata: None,
    };
    ledger_store.process_transfer(transfer2).await.expect("Failed to transfer to community");
    
    // Give 100 tokens to user1 (from federation)
    let transfer3 = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: TEST_FEDERATION_ID.to_string(),
        from: federation.clone(),
        to: user1.clone(),
        amount: 100,
        fee: 0,
        initiator: "system".to_string(),
        timestamp: Utc::now(),
        memo: Some("Initial user1 balance".to_string()),
        metadata: None,
    };
    ledger_store.process_transfer(transfer3).await.expect("Failed to transfer to user1");
    
    // Give 50 tokens to user2 (from federation)
    let transfer4 = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: TEST_FEDERATION_ID.to_string(),
        from: federation,
        to: user2,
        amount: 50,
        fee: 0,
        initiator: "system".to_string(),
        timestamp: Utc::now(),
        memo: Some("Initial user2 balance".to_string()),
        metadata: None,
    };
    ledger_store.process_transfer(transfer4).await.expect("Failed to transfer to user2");
    
    // Update the InMemoryStore with our initialized ledger store
    let mut store = db.write().unwrap();
    store.set_ledger(Arc::new(ledger_store));
    
    // Clean up the temporary pool
    cleanup_test_db(&pool, &schema_name).await;
}

/// Helper function to get balance from the ledger store
async fn get_entity_balance(db: &Db, entity: &EntityRef) -> u64 {
    // Get a reference to the ledger store
    let store = db.read().unwrap();
    let ledger = store.get_ledger().unwrap();
    drop(store); // Release the lock
    
    // Get the ledger implementation and query the balance
    let ledger = ledger.read().unwrap();
    ledger.get_balance(entity)
}

#[tokio::test]
async fn test_single_transfer_success() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config, pool, schema_name) = create_test_app().await;
    init_test_data(&db).await;
    
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
        memo: Some("Test transfer".to_string()),
        metadata: None,
    };
    
    // Send the transfer request
    let response = client
        .post(format!("{}/api/v1/federation/{}/transfers", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&transfer_request)
        .send()
        .await
        .expect("Failed to send transfer request");
    
    // Check the response status
    assert_eq!(response.status(), StatusCode::OK);
    
    // Parse the response
    let transfer_response: TransferResponse = response.json().await.expect("Failed to parse response");
    
    // Check the response
    assert_eq!(transfer_response.transfer.from.id, "test_user1");
    assert_eq!(transfer_response.transfer.to.id, "test_user2");
    assert_eq!(transfer_response.transfer.amount, 10);
    assert_eq!(transfer_response.from_balance, 90); // 100 - 10
    assert_eq!(transfer_response.to_balance, 60);   // 50 + 10
    
    // Check that the transfer is recorded in the ledger
    let user1_balance = get_entity_balance(&db, &EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    }).await;
    
    let user2_balance = get_entity_balance(&db, &EntityRef {
        entity_type: EntityType::User,
        id: "test_user2".to_string(),
    }).await;
    
    assert_eq!(user1_balance, 90);
    assert_eq!(user2_balance, 60);
    
    // Clean up
    handle.abort();
    cleanup_test_db(&pool, &schema_name).await;
}

#[tokio::test]
async fn test_single_transfer_insufficient_balance() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config, pool, schema_name) = create_test_app().await;
    init_test_data(&db).await;
    
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
    
    // Create a transfer request with an amount exceeding the balance
    let transfer_request = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::User,
            id: "test_user1".to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::User,
            id: "test_user2".to_string(),
        },
        amount: 200, // User1 only has 100 tokens
        memo: Some("Test transfer".to_string()),
        metadata: None,
    };
    
    // Send the transfer request
    let response = client
        .post(format!("{}/api/v1/federation/{}/transfers", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&transfer_request)
        .send()
        .await
        .expect("Failed to send transfer request");
    
    // Check that we get a 400 Bad Request response
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    
    // Check that the balances remain unchanged
    let user1_balance = get_entity_balance(&db, &EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    }).await;
    
    let user2_balance = get_entity_balance(&db, &EntityRef {
        entity_type: EntityType::User,
        id: "test_user2".to_string(),
    }).await;
    
    // Balances should be unchanged
    assert_eq!(user1_balance, 100);
    assert_eq!(user2_balance, 50);
    
    // Clean up
    handle.abort();
    cleanup_test_db(&pool, &schema_name).await;
}

#[tokio::test]
async fn test_single_transfer_entity_not_found() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config, pool, schema_name) = create_test_app().await;
    init_test_data(&db).await;
    
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
    
    // Create a transfer request with a non-existent entity
    let transfer_request = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::User,
            id: "test_user1".to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::User,
            id: "nonexistent_user".to_string(),
        },
        amount: 10,
        memo: Some("Test transfer".to_string()),
        metadata: None,
    };
    
    // Send the transfer request
    let response = client
        .post(format!("{}/api/v1/federation/{}/transfers", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&transfer_request)
        .send()
        .await
        .expect("Failed to send transfer request");
    
    // We should get a successful response since the entity will be created automatically
    assert_eq!(response.status(), StatusCode::OK);
    
    // Parse the response
    let transfer_response: TransferResponse = response.json().await.expect("Failed to parse response");
    
    // Check the response
    assert_eq!(transfer_response.transfer.from.id, "test_user1");
    assert_eq!(transfer_response.transfer.to.id, "nonexistent_user");
    assert_eq!(transfer_response.transfer.amount, 10);
    assert_eq!(transfer_response.from_balance, 90); // 100 - 10
    assert_eq!(transfer_response.to_balance, 10);   // 0 + 10
    
    // Check that the transfer is recorded in the ledger
    let user1_balance = get_entity_balance(&db, &EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    }).await;
    
    let nonexistent_user_balance = get_entity_balance(&db, &EntityRef {
        entity_type: EntityType::User,
        id: "nonexistent_user".to_string(),
    }).await;
    
    assert_eq!(user1_balance, 90);
    assert_eq!(nonexistent_user_balance, 10);
    
    // Clean up
    handle.abort();
    cleanup_test_db(&pool, &schema_name).await;
}

#[tokio::test]
async fn test_authorization_federation_admin() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config, pool, schema_name) = create_test_app().await;
    init_test_data(&db).await;
    
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
    
    // Create a transfer request from federation to user
    let transfer_request = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::Federation,
            id: TEST_FEDERATION_ID.to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::User,
            id: "test_user1".to_string(),
        },
        amount: 50,
        memo: Some("Federation admin transfer".to_string()),
        metadata: None,
    };
    
    // Send the transfer request
    let response = client
        .post(format!("{}/api/v1/federation/{}/transfers", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&transfer_request)
        .send()
        .await
        .expect("Failed to send transfer request");
    
    // Check the response status
    assert_eq!(response.status(), StatusCode::OK);
    
    // Parse the response
    let transfer_response: TransferResponse = response.json().await.expect("Failed to parse response");
    
    // Check the response
    assert_eq!(transfer_response.transfer.from.id, TEST_FEDERATION_ID);
    assert_eq!(transfer_response.transfer.to.id, "test_user1");
    assert_eq!(transfer_response.transfer.amount, 50);
    
    // The user1 balance should now be 150
    let user1_balance = get_entity_balance(&db, &EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    }).await;
    
    assert_eq!(user1_balance, 150); // 100 + 50
    
    // Clean up
    handle.abort();
    cleanup_test_db(&pool, &schema_name).await;
}

// Add the remaining test implementations here 