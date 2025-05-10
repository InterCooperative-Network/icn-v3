use chrono::Utc;
use icn_agoranet::ledger::{create_pg_ledger_store, LedgerStore, LedgerError};
use icn_agoranet::models::{EntityRef, EntityType, Transfer, TransferRequest};
use std::env;
use tokio::test;
use uuid::Uuid;

// This helper function will initialize a PostgreSQL connection for tests
async fn setup_test_db() -> Option<impl LedgerStore> {
    let database_url = env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        // Default connection string for local testing
        "postgres://postgres:postgres@localhost:5432/icn_ledger_test".to_string()
    });
    
    // Skip tests if TEST_PG environment variable is not set to "true"
    if env::var("TEST_PG").unwrap_or_else(|_| "false".to_string()) != "true" {
        println!("Skipping PostgreSQL integration tests. Set TEST_PG=true to run them.");
        return None;
    }
    
    // Create the PostgreSQL store with the test database
    match create_pg_ledger_store(&database_url).await {
        Ok(store) => Some(store),
        Err(e) => {
            panic!("Failed to initialize test database: {}", e);
        }
    }
}

#[test]
async fn test_entity_creation_and_balance() {
    let store = match setup_test_db().await {
        Some(store) => store,
        None => return, // Skip test if database is not available
    };
    
    // Test entities
    let federation = EntityRef {
        entity_type: EntityType::Federation,
        id: format!("test-federation-{}", Uuid::new_v4()),
    };
    
    let user = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:user-{}", Uuid::new_v4()),
    };
    
    // Ensure entities exist
    store.ensure_entity_exists(&federation, &federation.id).await.unwrap();
    store.ensure_entity_exists(&user, &federation.id).await.unwrap();
    
    // Check initial balances
    let fed_balance = store.get_balance(&federation).await.unwrap();
    let user_balance = store.get_balance(&user).await.unwrap();
    
    assert_eq!(fed_balance, 0, "New entity should have zero balance");
    assert_eq!(user_balance, 0, "New entity should have zero balance");
}

#[test]
async fn test_single_transfer() {
    let store = match setup_test_db().await {
        Some(store) => store,
        None => return, // Skip test
    };
    
    // Create test entities
    let federation = EntityRef {
        entity_type: EntityType::Federation,
        id: format!("test-federation-{}", Uuid::new_v4()),
    };
    
    let user = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:user-{}", Uuid::new_v4()),
    };
    
    // Initialize entities with some federation balance
    store.ensure_entity_exists(&federation, &federation.id).await.unwrap();
    
    // Create an initial transfer to fund the federation
    let initial_transfer = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: federation.id.clone(),
        from: federation.clone(), // Self-funding for testing
        to: federation.clone(),
        amount: 1_000_000,
        fee: 0, // No fee for initial setup
        initiator: "system".to_string(),
        timestamp: Utc::now(),
        memo: Some("Initial funding".to_string()),
    };
    
    // Process the initial transfer (this is a special case just for testing)
    let _ = store.process_transfer(initial_transfer).await.unwrap();
    
    // Ensure user entity exists
    store.ensure_entity_exists(&user, &federation.id).await.unwrap();
    
    // Create a transfer from federation to user
    let transfer_amount = 5000;
    let fee = 50;
    
    let transfer = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: federation.id.clone(),
        from: federation.clone(),
        to: user.clone(),
        amount: transfer_amount,
        fee,
        initiator: "test-initiator".to_string(),
        timestamp: Utc::now(),
        memo: Some("Test transfer".to_string()),
    };
    
    // Process the transfer
    let processed_transfer = store.process_transfer(transfer.clone()).await.unwrap();
    
    // Verify the processed transfer
    assert_eq!(processed_transfer.tx_id, transfer.tx_id, "Transaction ID should match");
    assert_eq!(processed_transfer.amount, transfer_amount, "Transfer amount should match");
    
    // Check updated balances
    let fed_balance = store.get_balance(&federation).await.unwrap();
    let user_balance = store.get_balance(&user).await.unwrap();
    
    assert_eq!(fed_balance, 1_000_000 - transfer_amount - fee, "Federation balance should be reduced by amount + fee");
    assert_eq!(user_balance, transfer_amount, "User balance should increase by transfer amount");
    
    // Retrieve the transfer by ID
    let retrieved_transfer = store.find_transfer(&transfer.tx_id).await.unwrap().unwrap();
    
    assert_eq!(retrieved_transfer.tx_id, transfer.tx_id, "Retrieved transfer ID should match");
    assert_eq!(retrieved_transfer.amount, transfer_amount, "Retrieved transfer amount should match");
}

#[test]
async fn test_insufficient_balance() {
    let store = match setup_test_db().await {
        Some(store) => store,
        None => return, // Skip test
    };
    
    // Create test entities
    let sender = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:sender-{}", Uuid::new_v4()),
    };
    
    let receiver = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:receiver-{}", Uuid::new_v4()),
    };
    
    let federation_id = format!("test-federation-{}", Uuid::new_v4());
    
    // Ensure entities exist
    store.ensure_entity_exists(&sender, &federation_id).await.unwrap();
    store.ensure_entity_exists(&receiver, &federation_id).await.unwrap();
    
    // Create a transfer that exceeds available balance
    let transfer = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id,
        from: sender.clone(),
        to: receiver.clone(),
        amount: 1000, // Sender has 0 balance
        fee: 10,
        initiator: "test-initiator".to_string(),
        timestamp: Utc::now(),
        memo: Some("Should fail due to insufficient balance".to_string()),
    };
    
    // Process the transfer - should fail with InsufficientBalance
    let result = store.process_transfer(transfer).await;
    
    assert!(result.is_err(), "Transfer should fail due to insufficient balance");
    
    match result {
        Err(LedgerError::InsufficientBalance) => {
            // This is the expected error
        },
        _ => {
            panic!("Expected InsufficientBalance error, got: {:?}", result);
        }
    }
}

#[test]
async fn test_batch_transfer() {
    let store = match setup_test_db().await {
        Some(store) => store,
        None => return, // Skip test
    };
    
    // Create test entities
    let federation = EntityRef {
        entity_type: EntityType::Federation,
        id: format!("test-federation-{}", Uuid::new_v4()),
    };
    
    let user1 = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:user1-{}", Uuid::new_v4()),
    };
    
    let user2 = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:user2-{}", Uuid::new_v4()),
    };
    
    // Initialize entities with some federation balance
    store.ensure_entity_exists(&federation, &federation.id).await.unwrap();
    
    // Create an initial transfer to fund the federation
    let initial_transfer = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: federation.id.clone(),
        from: federation.clone(), // Self-funding for testing
        to: federation.clone(),
        amount: 1_000_000,
        fee: 0, // No fee for initial setup
        initiator: "system".to_string(),
        timestamp: Utc::now(),
        memo: Some("Initial funding".to_string()),
    };
    
    // Process the initial transfer
    let _ = store.process_transfer(initial_transfer).await.unwrap();
    
    // Ensure user entities exist
    store.ensure_entity_exists(&user1, &federation.id).await.unwrap();
    store.ensure_entity_exists(&user2, &federation.id).await.unwrap();
    
    // Create a batch of transfers
    let transfers = vec![
        // Transfer 1: Valid transfer from federation to user1
        Transfer {
            tx_id: Uuid::new_v4(),
            federation_id: federation.id.clone(),
            from: federation.clone(),
            to: user1.clone(),
            amount: 5000,
            fee: 50,
            initiator: "test-initiator".to_string(),
            timestamp: Utc::now(),
            memo: Some("Batch transfer 1".to_string()),
        },
        // Transfer 2: Valid transfer from federation to user2
        Transfer {
            tx_id: Uuid::new_v4(),
            federation_id: federation.id.clone(),
            from: federation.clone(),
            to: user2.clone(),
            amount: 3000,
            fee: 30,
            initiator: "test-initiator".to_string(),
            timestamp: Utc::now(),
            memo: Some("Batch transfer 2".to_string()),
        },
        // Transfer 3: Invalid transfer from user1 with insufficient balance
        Transfer {
            tx_id: Uuid::new_v4(),
            federation_id: federation.id.clone(),
            from: user1.clone(),
            to: user2.clone(),
            amount: 10000, // User1 only has 5000
            fee: 100,
            initiator: "test-initiator".to_string(),
            timestamp: Utc::now(),
            memo: Some("Should fail - insufficient balance".to_string()),
        },
    ];
    
    // Process the batch transfer
    let batch_result = store.process_batch_transfer(transfers).await.unwrap();
    
    // Verify batch results
    assert_eq!(batch_result.successful, 2, "Two transfers should succeed");
    assert_eq!(batch_result.failed, 1, "One transfer should fail");
    assert_eq!(batch_result.successful_ids.len(), 2, "Should have 2 successful IDs");
    assert_eq!(batch_result.failed_transfers.len(), 1, "Should have 1 failed transfer");
    assert_eq!(batch_result.total_transferred, 5000 + 3000, "Total transferred should be sum of successful amounts");
    assert_eq!(batch_result.total_fees, 50 + 30, "Total fees should be sum of successful fees");
    
    // Check updated balances
    let fed_balance = store.get_balance(&federation).await.unwrap();
    let user1_balance = store.get_balance(&user1).await.unwrap();
    let user2_balance = store.get_balance(&user2).await.unwrap();
    
    assert_eq!(fed_balance, 1_000_000 - 5000 - 50 - 3000 - 30, "Federation balance should be reduced by successful transfers");
    assert_eq!(user1_balance, 5000, "User1 balance should be 5000");
    assert_eq!(user2_balance, 3000, "User2 balance should be 3000");
}

#[test]
async fn test_query_transfers() {
    let store = match setup_test_db().await {
        Some(store) => store,
        None => return, // Skip test
    };
    
    // Create test entities
    let federation = EntityRef {
        entity_type: EntityType::Federation,
        id: format!("test-federation-{}", Uuid::new_v4()),
    };
    
    let user1 = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:user1-{}", Uuid::new_v4()),
    };
    
    let user2 = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:user2-{}", Uuid::new_v4()),
    };
    
    // Initialize entities
    store.ensure_entity_exists(&federation, &federation.id).await.unwrap();
    
    // Create an initial transfer to fund the federation
    let initial_transfer = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: federation.id.clone(),
        from: federation.clone(), // Self-funding for testing
        to: federation.clone(),
        amount: 1_000_000,
        fee: 0, // No fee for initial setup
        initiator: "system".to_string(),
        timestamp: Utc::now(),
        memo: Some("Initial funding".to_string()),
    };
    
    // Process the initial transfer
    let _ = store.process_transfer(initial_transfer).await.unwrap();
    
    // Ensure user entities exist
    store.ensure_entity_exists(&user1, &federation.id).await.unwrap();
    store.ensure_entity_exists(&user2, &federation.id).await.unwrap();
    
    // Create multiple transfers with different amounts and between different entities
    let transfer1 = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: federation.id.clone(),
        from: federation.clone(),
        to: user1.clone(),
        amount: 5000,
        fee: 50,
        initiator: "test-initiator".to_string(),
        timestamp: Utc::now(),
        memo: Some("Transfer 1".to_string()),
    };
    
    let transfer2 = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: federation.id.clone(),
        from: federation.clone(),
        to: user2.clone(),
        amount: 3000,
        fee: 30,
        initiator: "test-initiator".to_string(),
        timestamp: Utc::now(),
        memo: Some("Transfer 2".to_string()),
    };
    
    let transfer3 = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: federation.id.clone(),
        from: user1.clone(),
        to: user2.clone(),
        amount: 1000,
        fee: 10,
        initiator: "test-initiator".to_string(),
        timestamp: Utc::now(),
        memo: Some("Transfer 3".to_string()),
    };
    
    // Process the transfers
    store.process_transfer(transfer1.clone()).await.unwrap();
    store.process_transfer(transfer2.clone()).await.unwrap();
    store.process_transfer(transfer3.clone()).await.unwrap();
    
    // Query transfers by federation
    let query = icn_agoranet::ledger::TransferQuery {
        federation_id: Some(federation.id.clone()),
        entity_id: None,
        entity_type: None,
        from_only: None,
        to_only: None,
        start_date: None,
        end_date: None,
        min_amount: None,
        max_amount: None,
        limit: None,
        offset: None,
    };
    
    let results = store.query_transfers(&query).await.unwrap();
    
    // Verify federation query results (should include all 4 transfers - initial + 3 test transfers)
    assert!(results.len() >= 4, "Should find at least 4 transfers for the federation");
    
    // Query transfers by entity (user1)
    let query = icn_agoranet::ledger::TransferQuery {
        federation_id: Some(federation.id.clone()),
        entity_id: Some(user1.id.clone()),
        entity_type: None,
        from_only: None,
        to_only: None,
        start_date: None,
        end_date: None,
        min_amount: None,
        max_amount: None,
        limit: None,
        offset: None,
    };
    
    let results = store.query_transfers(&query).await.unwrap();
    
    // Verify user1 query results (should include 2 transfers - fed->user1 and user1->user2)
    assert_eq!(results.len(), 2, "Should find 2 transfers for user1");
    
    // Query transfers by entity and direction (user1, from_only)
    let query = icn_agoranet::ledger::TransferQuery {
        federation_id: Some(federation.id.clone()),
        entity_id: Some(user1.id.clone()),
        entity_type: None,
        from_only: Some(true),
        to_only: None,
        start_date: None,
        end_date: None,
        min_amount: None,
        max_amount: None,
        limit: None,
        offset: None,
    };
    
    let results = store.query_transfers(&query).await.unwrap();
    
    // Verify from_only query results (should include 1 transfer - user1->user2)
    assert_eq!(results.len(), 1, "Should find 1 transfer where user1 is sender");
    assert_eq!(results[0].from.id, user1.id, "Sender should be user1");
    assert_eq!(results[0].to.id, user2.id, "Recipient should be user2");
    
    // Query transfers by amount range
    let query = icn_agoranet::ledger::TransferQuery {
        federation_id: Some(federation.id.clone()),
        entity_id: None,
        entity_type: None,
        from_only: None,
        to_only: None,
        start_date: None,
        end_date: None,
        min_amount: Some(3000),
        max_amount: Some(5000),
        limit: None,
        offset: None,
    };
    
    let results = store.query_transfers(&query).await.unwrap();
    
    // Verify amount range query results (should include 2 transfers - fed->user1 and fed->user2)
    assert_eq!(results.len(), 2, "Should find 2 transfers in the specified amount range");
    
    // Query with pagination
    let query = icn_agoranet::ledger::TransferQuery {
        federation_id: Some(federation.id.clone()),
        entity_id: None,
        entity_type: None,
        from_only: None,
        to_only: None,
        start_date: None,
        end_date: None,
        min_amount: None,
        max_amount: None,
        limit: Some(2),
        offset: None,
    };
    
    let results = store.query_transfers(&query).await.unwrap();
    
    // Verify pagination (should limit to 2 transfers)
    assert_eq!(results.len(), 2, "Should return only 2 transfers due to limit");
}

#[test]
async fn test_create_transfer_from_request() {
    let store = match setup_test_db().await {
        Some(store) => store,
        None => return, // Skip test
    };
    
    // Create test entities
    let federation = EntityRef {
        entity_type: EntityType::Federation,
        id: format!("test-federation-{}", Uuid::new_v4()),
    };
    
    let user = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:user-{}", Uuid::new_v4()),
    };
    
    // Initialize entities with some federation balance
    store.ensure_entity_exists(&federation, &federation.id).await.unwrap();
    
    // Create an initial transfer to fund the federation
    let initial_transfer = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: federation.id.clone(),
        from: federation.clone(), // Self-funding for testing
        to: federation.clone(),
        amount: 1_000_000,
        fee: 0, // No fee for initial setup
        initiator: "system".to_string(),
        timestamp: Utc::now(),
        memo: Some("Initial funding".to_string()),
    };
    
    // Process the initial transfer
    let _ = store.process_transfer(initial_transfer).await.unwrap();
    
    // Ensure user entity exists
    store.ensure_entity_exists(&user, &federation.id).await.unwrap();
    
    // Create a transfer request
    let request = TransferRequest {
        from: federation.clone(),
        to: user.clone(),
        amount: 5000,
        memo: Some("Transfer from request".to_string()),
        metadata: None,
    };
    
    // Process the transfer request
    let fee = 50;
    let initiator = "test-initiator".to_string();
    
    let transfer = store.create_transfer(&request, federation.id.clone(), initiator, fee).await.unwrap();
    
    // Verify the created transfer
    assert_eq!(transfer.from.id, federation.id, "From entity should match");
    assert_eq!(transfer.to.id, user.id, "To entity should match");
    assert_eq!(transfer.amount, 5000, "Amount should match");
    assert_eq!(transfer.fee, fee, "Fee should match");
    
    // Check updated balances
    let fed_balance = store.get_balance(&federation).await.unwrap();
    let user_balance = store.get_balance(&user).await.unwrap();
    
    assert_eq!(fed_balance, 1_000_000 - 5000 - fee, "Federation balance should be reduced");
    assert_eq!(user_balance, 5000, "User balance should be increased");
}

#[test]
async fn test_get_federation_stats() {
    let store = match setup_test_db().await {
        Some(store) => store,
        None => return, // Skip test
    };
    
    // Create test entities
    let federation = EntityRef {
        entity_type: EntityType::Federation,
        id: format!("test-federation-{}", Uuid::new_v4()),
    };
    
    let user1 = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:user1-{}", Uuid::new_v4()),
    };
    
    let user2 = EntityRef {
        entity_type: EntityType::User,
        id: format!("did:icn:user2-{}", Uuid::new_v4()),
    };
    
    // Initialize entities
    store.ensure_entity_exists(&federation, &federation.id).await.unwrap();
    
    // Create an initial transfer to fund the federation
    let initial_transfer = Transfer {
        tx_id: Uuid::new_v4(),
        federation_id: federation.id.clone(),
        from: federation.clone(), // Self-funding for testing
        to: federation.clone(),
        amount: 1_000_000,
        fee: 0, // No fee for initial setup
        initiator: "system".to_string(),
        timestamp: Utc::now(),
        memo: Some("Initial funding".to_string()),
    };
    
    // Process the initial transfer
    let _ = store.process_transfer(initial_transfer).await.unwrap();
    
    // Ensure user entities exist
    store.ensure_entity_exists(&user1, &federation.id).await.unwrap();
    store.ensure_entity_exists(&user2, &federation.id).await.unwrap();
    
    // Create multiple transfers
    let transfers = vec![
        // Transfer 1: Federation -> User1
        Transfer {
            tx_id: Uuid::new_v4(),
            federation_id: federation.id.clone(),
            from: federation.clone(),
            to: user1.clone(),
            amount: 5000,
            fee: 50,
            initiator: "test-initiator".to_string(),
            timestamp: Utc::now(),
            memo: Some("Transfer to user1".to_string()),
        },
        // Transfer 2: Federation -> User2
        Transfer {
            tx_id: Uuid::new_v4(),
            federation_id: federation.id.clone(),
            from: federation.clone(),
            to: user2.clone(),
            amount: 3000,
            fee: 30,
            initiator: "test-initiator".to_string(),
            timestamp: Utc::now(),
            memo: Some("Transfer to user2".to_string()),
        },
    ];
    
    // Process the transfers
    for transfer in transfers {
        store.process_transfer(transfer).await.unwrap();
    }
    
    // Get federation stats
    let stats = store.get_federation_stats(&federation.id).await.unwrap().unwrap();
    
    // Verify federation stats
    assert!(stats.total_transfers >= 3, "Should have at least 3 transfers");
    assert!(stats.total_volume >= 1_008_000, "Total volume should include all transfers");
    assert!(stats.total_fees >= 80, "Total fees should be at least 80");
    assert!(stats.total_entities >= 3, "Should have at least 3 entities");
    assert!(stats.active_entities >= 3, "Should have at least 3 active entities");
    assert!(stats.highest_balance > 0, "Highest balance should be positive");
}

// Additional tests for edge cases and error conditions could be added here 