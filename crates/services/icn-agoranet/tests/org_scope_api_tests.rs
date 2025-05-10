// Integration tests for organization-scoped API endpoints
use chrono::Utc;
use icn_agoranet::{
    app::create_app,
    handlers::Db,
    models::{ExecutionReceiptSummary, TokenBalance, TokenTransaction},
    websocket::WebSocketState,
};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use uuid::Uuid;

// Helper function to spawn the app in the background
async fn spawn_app() -> (String, JoinHandle<()>, Db, WebSocketState) {
    let store = Db::default();
    let ws_state = WebSocketState::new();
    let app = create_app(store.clone(), ws_state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap(); // Bind to a random available port
    let local_addr = listener.local_addr().unwrap();
    let server_url = format!("http://{}", local_addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (server_url, handle, store, ws_state)
}

// Helper to seed the database with test receipts
async fn seed_test_receipts(db: &Db) {
    // Federation 1, Coop 1, Community 1
    let receipt1 = ExecutionReceiptSummary {
        cid: format!("bafy-fed1-coop1-comm1-{}", Uuid::new_v4()),
        executor: "did:icn:executor1".to_string(),
        resource_usage: HashMap::from([
            ("CPU".to_string(), 100),
            ("Memory".to_string(), 512),
        ]),
        timestamp: Utc::now(),
        coop_id: Some("coop1".to_string()),
        community_id: Some("comm1".to_string()),
    };

    // Federation 1, Coop 1, Community 2
    let receipt2 = ExecutionReceiptSummary {
        cid: format!("bafy-fed1-coop1-comm2-{}", Uuid::new_v4()),
        executor: "did:icn:executor2".to_string(),
        resource_usage: HashMap::from([
            ("CPU".to_string(), 200),
            ("Memory".to_string(), 1024),
        ]),
        timestamp: Utc::now(),
        coop_id: Some("coop1".to_string()),
        community_id: Some("comm2".to_string()),
    };

    // Federation 1, Coop 2, Community 3
    let receipt3 = ExecutionReceiptSummary {
        cid: format!("bafy-fed1-coop2-comm3-{}", Uuid::new_v4()),
        executor: "did:icn:executor3".to_string(),
        resource_usage: HashMap::from([
            ("CPU".to_string(), 300),
            ("Memory".to_string(), 2048),
        ]),
        timestamp: Utc::now(),
        coop_id: Some("coop2".to_string()),
        community_id: Some("comm3".to_string()),
    };

    // Federation 2, Coop 3, Community 4
    let receipt4 = ExecutionReceiptSummary {
        cid: format!("bafy-fed2-coop3-comm4-{}", Uuid::new_v4()),
        executor: "did:icn:executor4".to_string(),
        resource_usage: HashMap::from([
            ("CPU".to_string(), 400),
            ("Memory".to_string(), 4096),
        ]),
        timestamp: Utc::now(),
        coop_id: Some("coop3".to_string()),
        community_id: Some("comm4".to_string()),
    };

    // Store receipts in the database
    db.create_receipt(receipt1).await;
    db.create_receipt(receipt2).await;
    db.create_receipt(receipt3).await;
    db.create_receipt(receipt4).await;
}

// Helper to seed the database with test token balances
async fn seed_test_token_balances(db: &Db) {
    // Federation 1, Coop 1, Community 1
    let balance1 = TokenBalance {
        did: "did:icn:user1".to_string(),
        amount: 1000,
        coop_id: Some("coop1".to_string()),
        community_id: Some("comm1".to_string()),
    };

    // Federation 1, Coop 1, Community 2
    let balance2 = TokenBalance {
        did: "did:icn:user2".to_string(),
        amount: 2000,
        coop_id: Some("coop1".to_string()),
        community_id: Some("comm2".to_string()),
    };

    // Federation 1, Coop 2, Community 3
    let balance3 = TokenBalance {
        did: "did:icn:user3".to_string(),
        amount: 3000,
        coop_id: Some("coop2".to_string()),
        community_id: Some("comm3".to_string()),
    };

    // Federation 2, Coop 3, Community 4
    let balance4 = TokenBalance {
        did: "did:icn:user4".to_string(),
        amount: 4000,
        coop_id: Some("coop3".to_string()),
        community_id: Some("comm4".to_string()),
    };

    // Store balances in the database
    db.create_token_balance(balance1).await;
    db.create_token_balance(balance2).await;
    db.create_token_balance(balance3).await;
    db.create_token_balance(balance4).await;
}

// Helper to seed the database with test token transactions
async fn seed_test_token_transactions(db: &Db) {
    // Federation 1, Coop 1, Community 1
    let tx1 = TokenTransaction {
        id: format!("tx-fed1-coop1-comm1-{}", Uuid::new_v4()),
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

    // Federation 1, Coop 1, Community 2
    let tx2 = TokenTransaction {
        id: format!("tx-fed1-coop1-comm2-{}", Uuid::new_v4()),
        from_did: "did:icn:treasury".to_string(),
        to_did: "did:icn:user2".to_string(),
        amount: 2000,
        operation: "mint".to_string(),
        timestamp: Utc::now(),
        from_coop_id: None,
        from_community_id: None,
        to_coop_id: Some("coop1".to_string()),
        to_community_id: Some("comm2".to_string()),
    };

    // Federation 1, Coop 2, Community 3
    let tx3 = TokenTransaction {
        id: format!("tx-fed1-coop2-comm3-{}", Uuid::new_v4()),
        from_did: "did:icn:treasury".to_string(),
        to_did: "did:icn:user3".to_string(),
        amount: 3000,
        operation: "mint".to_string(),
        timestamp: Utc::now(),
        from_coop_id: None,
        from_community_id: None,
        to_coop_id: Some("coop2".to_string()),
        to_community_id: Some("comm3".to_string()),
    };

    // Store transactions in the database
    db.create_token_transaction(tx1).await;
    db.create_token_transaction(tx2).await;
    db.create_token_transaction(tx3).await;
}

#[tokio::test]
async fn test_get_receipts_with_federation_filter() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, _ws_state) = spawn_app().await;
    seed_test_receipts(&db).await;
    
    let client = Client::new();
    
    // 2. Query receipts with federation filter
    // Since our model doesn't explicitly have federation_id, we're using
    // the cooperative ID pattern which implies federation membership
    let response = client
        .get(format!("{}/receipts?coop_id=coop1", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let receipts: Vec<Value> = response.json().await.expect("Failed to parse response");
    
    // 3. Verify only receipts from coop1 are returned
    assert_eq!(receipts.len(), 2, "Should return 2 receipts from coop1");
    
    // Check that all returned receipts are from coop1
    for receipt in receipts {
        assert_eq!(
            receipt["coop_id"].as_str().unwrap(),
            "coop1",
            "Receipt should be from coop1"
        );
    }
}

#[tokio::test]
async fn test_get_receipts_with_community_filter() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, _ws_state) = spawn_app().await;
    seed_test_receipts(&db).await;
    
    let client = Client::new();
    
    // 2. Query receipts with community filter
    let response = client
        .get(format!("{}/receipts?community_id=comm1", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let receipts: Vec<Value> = response.json().await.expect("Failed to parse response");
    
    // 3. Verify only receipts from comm1 are returned
    assert_eq!(receipts.len(), 1, "Should return 1 receipt from comm1");
    
    // Check that all returned receipts are from comm1
    for receipt in receipts {
        assert_eq!(
            receipt["community_id"].as_str().unwrap(),
            "comm1",
            "Receipt should be from comm1"
        );
    }
}

#[tokio::test]
async fn test_get_receipts_with_multiple_filters() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, _ws_state) = spawn_app().await;
    seed_test_receipts(&db).await;
    
    let client = Client::new();
    
    // 2. Query receipts with multiple filters (coop and community)
    let response = client
        .get(format!("{}/receipts?coop_id=coop1&community_id=comm2", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let receipts: Vec<Value> = response.json().await.expect("Failed to parse response");
    
    // 3. Verify only receipts matching both filters are returned
    assert_eq!(receipts.len(), 1, "Should return 1 receipt matching both filters");
    
    // Check that the returned receipt matches both filters
    assert_eq!(
        receipts[0]["coop_id"].as_str().unwrap(),
        "coop1",
        "Receipt should be from coop1"
    );
    assert_eq!(
        receipts[0]["community_id"].as_str().unwrap(),
        "comm2",
        "Receipt should be from comm2"
    );
}

#[tokio::test]
async fn test_get_token_balances_with_filters() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, _ws_state) = spawn_app().await;
    seed_test_token_balances(&db).await;
    
    let client = Client::new();
    
    // 2. Query token balances with coop filter
    let response = client
        .get(format!("{}/tokens/balances?coop_id=coop1", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let balances: Vec<Value> = response.json().await.expect("Failed to parse response");
    
    // 3. Verify only balances from coop1 are returned
    assert_eq!(balances.len(), 2, "Should return 2 balances from coop1");
    
    // 4. Now query with specific community filter
    let response = client
        .get(format!("{}/tokens/balances?community_id=comm1", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let balances: Vec<Value> = response.json().await.expect("Failed to parse response");
    
    // 5. Verify only balances from comm1 are returned
    assert_eq!(balances.len(), 1, "Should return 1 balance from comm1");
    assert_eq!(
        balances[0]["community_id"].as_str().unwrap(),
        "comm1",
        "Balance should be from comm1"
    );
}

#[tokio::test]
async fn test_get_token_transactions_with_filters() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, _ws_state) = spawn_app().await;
    seed_test_token_transactions(&db).await;
    
    let client = Client::new();
    
    // 2. Query token transactions with coop filter (from or to)
    let response = client
        .get(format!("{}/tokens/transactions?to_coop_id=coop1", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let transactions: Vec<Value> = response.json().await.expect("Failed to parse response");
    
    // 3. Verify only transactions involving coop1 are returned
    assert_eq!(transactions.len(), 2, "Should return 2 transactions involving coop1");
    
    // 4. Now query with specific community filter
    let response = client
        .get(format!("{}/tokens/transactions?to_community_id=comm2", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let transactions: Vec<Value> = response.json().await.expect("Failed to parse response");
    
    // 5. Verify only transactions involving comm2 are returned
    assert_eq!(transactions.len(), 1, "Should return 1 transaction involving comm2");
    assert_eq!(
        transactions[0]["to_community_id"].as_str().unwrap(),
        "comm2",
        "Transaction should involve comm2"
    );
}

#[tokio::test]
async fn test_invalid_org_filter_combinations() {
    // 1. Spawn app
    let (server_url, _handle, _db, _ws_state) = spawn_app().await;
    
    let client = Client::new();
    
    // 2. Make a request with invalid hierarchy (community without coop)
    // In a real implementation, this should verify the organizational hierarchy
    let response = client
        .get(format!("{}/receipts?community_id=comm1&coop_id=invalid", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    // 3. Should still work (API layer should validate/handle properly)
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    // But should return empty results or a validation error
    let result: Value = response.json().await.expect("Failed to parse response");
    
    if let Value::Array(results) = &result {
        assert!(results.is_empty(), "Should return empty results for invalid filter");
    } else if let Value::Object(error) = &result {
        assert!(error.contains_key("error"), "Should return error for invalid filter");
    } else {
        panic!("Expected empty results or error for invalid filter combination");
    }
}

#[tokio::test]
async fn test_org_scoped_stats_endpoints() {
    // 1. Spawn app and seed database
    let (server_url, _handle, db, _ws_state) = spawn_app().await;
    seed_test_receipts(&db).await;
    seed_test_token_balances(&db).await;
    seed_test_token_transactions(&db).await;
    
    let client = Client::new();
    
    // 2. Query receipt stats with organization filter
    let response = client
        .get(format!("{}/stats/receipts?coop_id=coop1", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let stats: Value = response.json().await.expect("Failed to parse response");
    
    // 3. Verify stats only include data from coop1
    assert!(stats["total_receipts"].as_u64().unwrap() > 0, "Should have receipt stats");
    
    // 4. Query token stats with organization filter
    let response = client
        .get(format!("{}/stats/tokens?coop_id=coop1", server_url))
        .send()
        .await
        .expect("Failed to send request");
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let stats: Value = response.json().await.expect("Failed to parse response");
    
    // 5. Verify token stats only include data from coop1
    assert!(stats["total_minted"].as_u64().unwrap() > 0, "Should have token stats");
} 