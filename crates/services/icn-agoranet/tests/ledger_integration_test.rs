// Integration tests for the PostgreSQL-backed ledger APIs
//
// These tests verify the functionality of the ledger APIs with a real PostgreSQL database.
// The tests use a Docker container to run PostgreSQL, so Docker must be installed and running.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use icn_agoranet::{
    app::create_app,
    ledger::{self, PostgresLedgerStore},
    models::{
        EntityRef, EntityType, Transfer, TransferRequest, TransferResponse,
        BatchTransferRequest, BatchTransferResponse,
    },
    auth::{JwtConfig, Claims, Role, create_jwt},
    handlers::Db,
    websocket::WebSocketState,
    transfers::TransferQuery,
};
use reqwest::Client;
use serde_json::json;
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
async fn create_test_app() -> (String, JoinHandle<()>, Db, Arc<JwtConfig>) {
    // Create the ledger store
    let ledger_store = Arc::new(create_test_ledger_store().await);
    
    // Initialize in-memory store with the ledger
    let mut store = icn_agoranet::handlers::InMemoryStore::new();
    store.set_ledger(ledger_store);
    let db: Db = Arc::new(RwLock::new(store));
    
    // Initialize WebSocket state
    let ws_state = WebSocketState::new();
    
    // Initialize JWT config
    let jwt_config = Arc::new(JwtConfig::new(JWT_SECRET.to_string()));
    
    // Create a token revocation store
    let revocation_store = Arc::new(icn_agoranet::auth::revocation::InMemoryRevocationStore::new());
    
    // Create the app
    let app = create_app(
        db.clone(),
        ws_state,
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
    
    (server_url, handle, db, jwt_config)
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
    
    // Ensure entities exist
    ledger.ensure_entity_exists(&federation, TEST_FEDERATION_ID);
    ledger.ensure_entity_exists(&cooperative, TEST_FEDERATION_ID);
    ledger.ensure_entity_exists(&community, TEST_FEDERATION_ID);
    ledger.ensure_entity_exists(&user1, TEST_FEDERATION_ID);
    ledger.ensure_entity_exists(&user2, TEST_FEDERATION_ID);
    
    // Initialize balances by making transfers from federation (minting tokens)
    // Federation starts with unlimited balance in this test model
    
    // Give 1000 tokens to cooperative
    let _ = ledger.process_transfer(Transfer {
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
    });
    
    // Give 500 tokens to community
    let _ = ledger.process_transfer(Transfer {
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
    });
    
    // Give 100 tokens to user1
    let _ = ledger.process_transfer(Transfer {
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
    });
    
    // Give 50 tokens to user2
    let _ = ledger.process_transfer(Transfer {
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
    });
}

#[tokio::test]
async fn test_single_transfer_success() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config) = create_test_app().await;
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
    let store = db.read().unwrap();
    let ledger = store.ledger.clone().unwrap();
    drop(store);
    
    let ledger = ledger.read().unwrap();
    let user1_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    });
    
    let user2_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::User,
        id: "test_user2".to_string(),
    });
    
    assert_eq!(user1_balance, 90);
    assert_eq!(user2_balance, 60);
    
    // Clean up
    handle.abort();
}

#[tokio::test]
async fn test_single_transfer_insufficient_balance() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config) = create_test_app().await;
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
    let store = db.read().unwrap();
    let ledger = store.ledger.clone().unwrap();
    drop(store);
    
    let ledger = ledger.read().unwrap();
    let user1_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    });
    
    let user2_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::User,
        id: "test_user2".to_string(),
    });
    
    // Balances should be unchanged
    assert_eq!(user1_balance, 100);
    assert_eq!(user2_balance, 50);
    
    // Clean up
    handle.abort();
}

#[tokio::test]
async fn test_single_transfer_entity_not_found() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config) = create_test_app().await;
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
    let store = db.read().unwrap();
    let ledger = store.ledger.clone().unwrap();
    drop(store);
    
    let ledger = ledger.read().unwrap();
    let user1_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    });
    
    let nonexistent_user_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::User,
        id: "nonexistent_user".to_string(),
    });
    
    assert_eq!(user1_balance, 90);
    assert_eq!(nonexistent_user_balance, 10);
    
    // Clean up
    handle.abort();
}

#[tokio::test]
async fn test_authorization_federation_admin() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config) = create_test_app().await;
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
    let store = db.read().unwrap();
    let ledger = store.ledger.clone().unwrap();
    drop(store);
    
    let ledger = ledger.read().unwrap();
    let user1_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    });
    
    assert_eq!(user1_balance, 150); // 100 + 50
    
    // Clean up
    handle.abort();
}

#[tokio::test]
async fn test_batch_transfers() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config) = create_test_app().await;
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
    
    // Create a batch of transfer requests
    let batch_request = BatchTransferRequest {
        transfers: vec![
            // Valid transfer: user1 -> community
            TransferRequest {
                from: EntityRef {
                    entity_type: EntityType::User,
                    id: "test_user1".to_string(),
                },
                to: EntityRef {
                    entity_type: EntityType::Community,
                    id: "test_community".to_string(),
                },
                amount: 10,
                memo: Some("Batch transfer 1".to_string()),
                metadata: None,
            },
            // Valid transfer: cooperative -> user2
            TransferRequest {
                from: EntityRef {
                    entity_type: EntityType::Cooperative,
                    id: "test_cooperative".to_string(),
                },
                to: EntityRef {
                    entity_type: EntityType::User,
                    id: "test_user2".to_string(),
                },
                amount: 20,
                memo: Some("Batch transfer 2".to_string()),
                metadata: None,
            },
            // Invalid transfer: user1 tries to send more than they have
            TransferRequest {
                from: EntityRef {
                    entity_type: EntityType::User,
                    id: "test_user1".to_string(),
                },
                to: EntityRef {
                    entity_type: EntityType::Community,
                    id: "test_community".to_string(),
                },
                amount: 200, // User1 only has 100 tokens (or 90 after the first transfer)
                memo: Some("Batch transfer 3 (should fail)".to_string()),
                metadata: None,
            },
        ],
    };
    
    // Send the batch transfer request
    let response = client
        .post(format!("{}/api/v1/federation/{}/transfers/batch", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&batch_request)
        .send()
        .await
        .expect("Failed to send batch transfer request");
    
    // Check the response status (should be OK even if some transfers fail)
    assert_eq!(response.status(), StatusCode::OK);
    
    // Parse the response
    let batch_response: BatchTransferResponse = response.json().await.expect("Failed to parse response");
    
    // Check the response - should have 2 successful and 1 failed transfer
    assert_eq!(batch_response.successful_ids.len(), 2);
    assert_eq!(batch_response.failed_transfers.len(), 1);
    
    // Check that the balances were updated correctly
    let store = db.read().unwrap();
    let ledger = store.ledger.clone().unwrap();
    drop(store);
    
    let ledger = ledger.read().unwrap();
    
    // User1 should have 90 tokens (100 - 10)
    let user1_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::User,
        id: "test_user1".to_string(),
    });
    assert_eq!(user1_balance, 90);
    
    // Community should have 510 tokens (500 + 10)
    let community_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::Community,
        id: "test_community".to_string(),
    });
    assert_eq!(community_balance, 510);
    
    // User2 should have 70 tokens (50 + 20)
    let user2_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::User,
        id: "test_user2".to_string(),
    });
    assert_eq!(user2_balance, 70);
    
    // Cooperative should have 980 tokens (1000 - 20)
    let coop_balance = ledger.get_balance(&EntityRef {
        entity_type: EntityType::Cooperative,
        id: "test_cooperative".to_string(),
    });
    assert_eq!(coop_balance, 980);
    
    // Clean up
    handle.abort();
}

#[tokio::test]
async fn test_query_transfers() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config) = create_test_app().await;
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
    
    // Make some transfers to query
    // Transfer 1: user1 -> user2
    let transfer1 = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::User,
            id: "test_user1".to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::User,
            id: "test_user2".to_string(),
        },
        amount: 10,
        memo: Some("Query test transfer 1".to_string()),
        metadata: None,
    };
    
    client
        .post(format!("{}/api/v1/federation/{}/transfers", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&transfer1)
        .send()
        .await
        .expect("Failed to send transfer request");
    
    // Transfer 2: cooperative -> user1
    let transfer2 = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::Cooperative,
            id: "test_cooperative".to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::User,
            id: "test_user1".to_string(),
        },
        amount: 20,
        memo: Some("Query test transfer 2".to_string()),
        metadata: None,
    };
    
    client
        .post(format!("{}/api/v1/federation/{}/transfers", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&transfer2)
        .send()
        .await
        .expect("Failed to send transfer request");
    
    // Transfer 3: community -> user2
    let transfer3 = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::Community,
            id: "test_community".to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::User,
            id: "test_user2".to_string(),
        },
        amount: 30,
        memo: Some("Query test transfer 3".to_string()),
        metadata: None,
    };
    
    client
        .post(format!("{}/api/v1/federation/{}/transfers", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&transfer3)
        .send()
        .await
        .expect("Failed to send transfer request");
    
    // Now query all transfers
    let response = client
        .get(format!("{}/api/v1/federation/{}/transfers/query", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send query request");
    
    // Check the response status
    assert_eq!(response.status(), StatusCode::OK);
    
    // Parse the response
    let transfers: Vec<Transfer> = response.json().await.expect("Failed to parse response");
    
    // Should have at least 3 transfers (plus the initial setup transfers)
    assert!(transfers.len() >= 3);
    
    // Query transfers for user1 (both as sender and receiver)
    let response = client
        .get(format!(
            "{}/api/v1/federation/{}/transfers/query?entity_id=test_user1",
            server_url, TEST_FEDERATION_ID
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send query request");
    
    // Check the response status
    assert_eq!(response.status(), StatusCode::OK);
    
    // Parse the response
    let user1_transfers: Vec<Transfer> = response.json().await.expect("Failed to parse response");
    
    // Should have at least 2 transfers involving user1
    assert!(user1_transfers.len() >= 2);
    
    // Query transfers for user1 as sender only
    let response = client
        .get(format!(
            "{}/api/v1/federation/{}/transfers/query?entity_id=test_user1&from_only=true",
            server_url, TEST_FEDERATION_ID
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send query request");
    
    // Check the response status
    assert_eq!(response.status(), StatusCode::OK);
    
    // Parse the response
    let user1_from_transfers: Vec<Transfer> = response.json().await.expect("Failed to parse response");
    
    // Should have at least 1 transfer where user1 is the sender
    assert!(!user1_from_transfers.is_empty());
    assert!(user1_from_transfers.iter().all(|t| t.from.id == "test_user1"));
    
    // Clean up
    handle.abort();
}

#[tokio::test]
async fn test_ledger_stats() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config) = create_test_app().await;
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
    
    // Make some transfers to generate stats
    // Transfer 1: user1 -> user2
    let transfer1 = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::User,
            id: "test_user1".to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::User,
            id: "test_user2".to_string(),
        },
        amount: 10,
        memo: Some("Stats test transfer 1".to_string()),
        metadata: None,
    };
    
    client
        .post(format!("{}/api/v1/federation/{}/transfers", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&transfer1)
        .send()
        .await
        .expect("Failed to send transfer request");
    
    // Transfer 2: cooperative -> community
    let transfer2 = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::Cooperative,
            id: "test_cooperative".to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::Community,
            id: "test_community".to_string(),
        },
        amount: 100,
        memo: Some("Stats test transfer 2".to_string()),
        metadata: None,
    };
    
    client
        .post(format!("{}/api/v1/federation/{}/transfers", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&transfer2)
        .send()
        .await
        .expect("Failed to send transfer request");
    
    // Now get the federation ledger stats
    let response = client
        .get(format!("{}/api/v1/federation/{}/ledger/stats", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send stats request");
    
    // Check the response status
    assert_eq!(response.status(), StatusCode::OK);
    
    // Parse the response
    let stats: icn_agoranet::ledger::LedgerStats = response.json().await.expect("Failed to parse response");
    
    // Check basic stats
    assert!(stats.total_transfers >= 2);  // At least the 2 transfers we just made
    assert!(stats.total_volume >= 110);   // At least 10 + 100
    
    // Clean up
    handle.abort();
}

#[tokio::test]
async fn test_authorization_coop_operator() {
    // Skip tests if no database is available
    if env::var("SKIP_DB_TESTS").is_ok() {
        println!("Skipping database tests");
        return;
    }
    
    // Setup
    let (server_url, handle, db, jwt_config) = create_test_app().await;
    init_test_data(&db).await;
    
    // Create an HTTP client
    let client = Client::new();
    
    // Create a cooperative operator token
    let token = create_test_token(
        "coop_operator",
        vec![COOP_OPERATOR_ROLE.to_string()],
        Some(TEST_FEDERATION_ID.to_string()),
        Some("test_cooperative".to_string()),
        None,
        &jwt_config,
    );
    
    // Coop operator should be able to transfer from their cooperative
    let transfer_request = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::Cooperative,
            id: "test_cooperative".to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::User,
            id: "test_user1".to_string(),
        },
        amount: 50,
        memo: Some("Cooperative transfer".to_string()),
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
    
    // Now try to transfer from a different cooperative (should fail)
    let bad_transfer_request = TransferRequest {
        from: EntityRef {
            entity_type: EntityType::Cooperative,
            id: "different_cooperative".to_string(),
        },
        to: EntityRef {
            entity_type: EntityType::User,
            id: "test_user1".to_string(),
        },
        amount: 50,
        memo: Some("Unauthorized cooperative transfer".to_string()),
        metadata: None,
    };
    
    // Send the transfer request
    let response = client
        .post(format!("{}/api/v1/federation/{}/transfers", server_url, TEST_FEDERATION_ID))
        .header("Authorization", format!("Bearer {}", token))
        .json(&bad_transfer_request)
        .send()
        .await
        .expect("Failed to send transfer request");
    
    // Check that we get a 403 Forbidden response
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    
    // Clean up
    handle.abort();
}

// Add more tests here for community officials and regular users
// - test_authorization_community_official
// - test_authorization_regular_user
// - test_ledger_stats
// - etc. 