// Integration tests for organization-scoped WebSockets
use futures::{SinkExt, StreamExt};
use icn_agoranet::{
    app::create_app,
    handlers::Db,
    models::{ExecutionReceiptSummary, TokenTransaction},
    websocket::{WebSocketEvent, WebSocketState},
};
use serde_json::Value;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

// Helper function to spawn the app in the background
async fn spawn_app() -> (String, JoinHandle<()>, Db, WebSocketState) {
    let store = Db::default();
    let ws_state = WebSocketState::new();
    let app = create_app(store.clone(), ws_state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap(); // Bind to a random available port
    let local_addr = listener.local_addr().unwrap();
    let server_url = format!("http://{}", local_addr);
    let ws_url = format!("ws://{}", local_addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (ws_url, handle, store, ws_state)
}

// Helper function to connect to a WebSocket
async fn connect_to_ws(url: &str) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect to WebSocket");
    ws_stream
}

// Helper to wait for and extract the first message from WebSocket
async fn get_first_message(stream: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>) -> Option<Value> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(msg) = stream.next().await {
            if let Ok(Message::Text(text)) = msg {
                return serde_json::from_str(&text).ok();
            }
        }
        None
    }).await.unwrap_or(None)
}

#[tokio::test]
async fn test_federation_channel_subscription() {
    // 1. Spawn app and get WebSocket state
    let (ws_url, _handle, _db, ws_state) = spawn_app().await;
    
    // 2. Connect to federation-specific WebSocket
    let fed_id = "fed1";
    let ws_fed_url = format!("{}/ws?federation_id={}", ws_url, fed_id);
    let mut ws_stream = connect_to_ws(&ws_fed_url).await;
    
    // 3. Broadcast a federation-scoped event
    let receipt = ExecutionReceiptSummary {
        cid: "test-cid".to_string(),
        executor: "test-executor".to_string(),
        resource_usage: std::collections::HashMap::new(),
        timestamp: chrono::Utc::now(),
        coop_id: Some("coop1".to_string()),
        community_id: Some("comm1".to_string()),
    };
    
    let event = WebSocketEvent::ReceiptCreated(receipt);
    ws_state.broadcast_event(
        Some(fed_id),
        Some("coop1"),
        Some("comm1"),
        event
    );
    
    // 4. Wait for message and verify it's received in the federation channel
    let message = get_first_message(&mut ws_stream).await;
    assert!(message.is_some(), "Should receive message in federation channel");
    
    // Close the WebSocket
    ws_stream.close(None).await.unwrap();
}

#[tokio::test]
async fn test_cooperative_channel_subscription() {
    // 1. Spawn app and get WebSocket state
    let (ws_url, _handle, _db, ws_state) = spawn_app().await;
    
    // 2. Connect to coop-specific WebSocket
    let fed_id = "fed1";
    let coop_id = "coop1";
    let ws_coop_url = format!("{}/ws?federation_id={}&coop_id={}", ws_url, fed_id, coop_id);
    let mut ws_stream = connect_to_ws(&ws_coop_url).await;
    
    // 3. Broadcast an event scoped to a different cooperative
    let tx = TokenTransaction {
        id: "test-tx".to_string(),
        from_did: "test-from".to_string(),
        to_did: "test-to".to_string(),
        amount: 100,
        operation: "transfer".to_string(),
        timestamp: chrono::Utc::now(),
        from_coop_id: Some("coop2".to_string()),
        from_community_id: None,
        to_coop_id: Some("coop2".to_string()),
        to_community_id: None,
    };
    
    let event = WebSocketEvent::TokenTransferred(tx);
    ws_state.broadcast_event(
        Some(fed_id),
        Some("coop2"),
        None,
        event
    );
    
    // 4. Verify no message is received for a different coop's event
    let message = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(msg) = ws_stream.next().await {
            if let Ok(Message::Text(_)) = msg {
                return Some(true);
            }
        }
        None
    }).await;
    
    assert!(message.is_err() || message.unwrap().is_none(), "Should not receive message from different coop");
    
    // 5. Now broadcast an event for the subscribed coop
    let tx2 = TokenTransaction {
        id: "test-tx2".to_string(),
        from_did: "test-from".to_string(),
        to_did: "test-to".to_string(),
        amount: 100,
        operation: "transfer".to_string(),
        timestamp: chrono::Utc::now(),
        from_coop_id: Some(coop_id.to_string()),
        from_community_id: None,
        to_coop_id: Some(coop_id.to_string()),
        to_community_id: None,
    };
    
    let event2 = WebSocketEvent::TokenTransferred(tx2);
    ws_state.broadcast_event(
        Some(fed_id),
        Some(coop_id),
        None,
        event2
    );
    
    // 6. Verify the message for the correct coop is received
    let message = get_first_message(&mut ws_stream).await;
    assert!(message.is_some(), "Should receive message for correct coop");
    
    // Close the WebSocket
    ws_stream.close(None).await.unwrap();
}

#[tokio::test]
async fn test_community_channel_subscription() {
    // 1. Spawn app and get WebSocket state
    let (ws_url, _handle, _db, ws_state) = spawn_app().await;
    
    // 2. Connect to community-specific WebSocket
    let fed_id = "fed1";
    let coop_id = "coop1";
    let comm_id = "comm1";
    let ws_comm_url = format!("{}/ws?federation_id={}&coop_id={}&community_id={}", 
        ws_url, fed_id, coop_id, comm_id);
    let mut ws_stream = connect_to_ws(&ws_comm_url).await;
    
    // 3. Broadcast an event scoped to this community
    let tx = TokenTransaction {
        id: "test-tx".to_string(),
        from_did: "test-from".to_string(),
        to_did: "test-to".to_string(),
        amount: 100,
        operation: "mint".to_string(),
        timestamp: chrono::Utc::now(),
        from_coop_id: None,
        from_community_id: None,
        to_coop_id: Some(coop_id.to_string()),
        to_community_id: Some(comm_id.to_string()),
    };
    
    let event = WebSocketEvent::TokenMinted(tx);
    ws_state.broadcast_event(
        Some(fed_id),
        Some(coop_id),
        Some(comm_id),
        event
    );
    
    // 4. Verify the community-scoped message is received
    let message = get_first_message(&mut ws_stream).await;
    assert!(message.is_some(), "Should receive message for community channel");
    
    // Close the WebSocket
    ws_stream.close(None).await.unwrap();
}

#[tokio::test]
async fn test_invalid_org_scope_hierarchy() {
    // 1. Spawn app
    let (ws_url, _handle, _db, _ws_state) = spawn_app().await;
    
    // 2. Attempt to connect with invalid hierarchy (community without coop)
    let invalid_ws_url = format!("{}/ws?federation_id=fed1&community_id=comm1", ws_url);
    
    // 3. Connect should fail or return an error status
    let result = connect_async(&invalid_ws_url).await;
    
    // Either connection fails or we get a close message with an error
    match result {
        Ok((mut stream, _)) => {
            let msg = stream.next().await;
            // Check if we received a close message
            match msg {
                Some(Ok(Message::Close(_))) => {
                    // This is expected - connection closed due to invalid params
                },
                Some(Ok(Message::Text(text))) => {
                    // Some implementations might send an error message
                    assert!(text.contains("error") || text.contains("invalid"), 
                            "Should receive error message for invalid hierarchy");
                },
                _ => {
                    panic!("Expected connection to be rejected for invalid hierarchy");
                }
            }
        },
        Err(_) => {
            // Connection failure is also an acceptable response to invalid params
        }
    }
}

#[tokio::test]
async fn test_broadcast_hierarchical_propagation() {
    // 1. Spawn app and get WebSocket state
    let (ws_url, _handle, _db, ws_state) = spawn_app().await;
    
    // 2. Connect to different channel levels
    let fed_id = "fed1";
    let coop_id = "coop1";
    let comm_id = "comm1";
    
    // Connect to federation channel
    let fed_ws_url = format!("{}/ws?federation_id={}", ws_url, fed_id);
    let mut fed_stream = connect_to_ws(&fed_ws_url).await;
    
    // Connect to coop channel
    let coop_ws_url = format!("{}/ws?federation_id={}&coop_id={}", ws_url, fed_id, coop_id);
    let mut coop_stream = connect_to_ws(&coop_ws_url).await;
    
    // Connect to community channel
    let comm_ws_url = format!("{}/ws?federation_id={}&coop_id={}&community_id={}", 
        ws_url, fed_id, coop_id, comm_id);
    let mut comm_stream = connect_to_ws(&comm_ws_url).await;
    
    // 3. Broadcast an event at community level
    let tx = TokenTransaction {
        id: "test-tx".to_string(),
        from_did: "test-from".to_string(),
        to_did: "test-to".to_string(),
        amount: 100,
        operation: "transfer".to_string(),
        timestamp: chrono::Utc::now(),
        from_coop_id: Some(coop_id.to_string()),
        from_community_id: Some(comm_id.to_string()),
        to_coop_id: Some(coop_id.to_string()),
        to_community_id: Some(comm_id.to_string()),
    };
    
    let event = WebSocketEvent::TokenTransferred(tx);
    ws_state.broadcast_event(
        Some(fed_id),
        Some(coop_id),
        Some(comm_id),
        event
    );
    
    // 4. Verify all three channels receive the event
    let comm_msg = get_first_message(&mut comm_stream).await;
    assert!(comm_msg.is_some(), "Community channel should receive its own events");
    
    let coop_msg = get_first_message(&mut coop_stream).await;
    assert!(coop_msg.is_some(), "Coop channel should receive events from its communities");
    
    let fed_msg = get_first_message(&mut fed_stream).await;
    assert!(fed_msg.is_some(), "Federation channel should receive events from its coops and communities");
    
    // Close WebSockets
    comm_stream.close(None).await.unwrap();
    coop_stream.close(None).await.unwrap();
    fed_stream.close(None).await.unwrap();
} 