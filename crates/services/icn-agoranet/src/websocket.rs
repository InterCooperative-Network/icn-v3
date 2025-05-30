use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Path, Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use uuid::Uuid;
use chrono::Utc;

use crate::handlers::Db;
use crate::models::{ExecutionReceiptSummary, TokenTransaction, ResourceType};
use crate::auth::{validate_token, JwtConfig, Claims, ScopeClaims};

// Maximum number of messages to buffer for each channel
const MAX_CHANNEL_CAPACITY: usize = 100;

/// WebSocket event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WebSocketEvent {
    /// New execution receipt
    ReceiptCreated(ExecutionReceiptSummary),
    /// Token transferred between accounts
    TokenTransferred(TokenTransaction),
    /// Token minted to an account
    TokenMinted(TokenTransaction),
    /// Token burned from an account
    TokenBurned(TokenTransaction),
    /// Custom JSON event
    Custom(serde_json::Value),
}

/// WebSocket channel name builder
fn build_channel_name(federation_id: Option<&str>, coop_id: Option<&str>, community_id: Option<&str>) -> String {
    match (federation_id, coop_id, community_id) {
        (Some(fed), Some(coop), Some(comm)) => 
            format!("federation:{}:coop:{}:community:{}", fed, coop, comm),
        (Some(fed), Some(coop), None) => 
            format!("federation:{}:coop:{}", fed, coop),
        (Some(fed), None, None) => 
            format!("federation:{}", fed),
        _ => "global".to_string(),
    }
}

/// Query parameters for WebSocket connections
#[derive(Debug, Deserialize)]
pub struct WebSocketParams {
    /// Optional federation ID to scope the WebSocket channel
    pub federation_id: Option<String>,
    /// Optional cooperative ID to scope the WebSocket channel
    pub coop_id: Option<String>,
    /// Optional community ID to scope the WebSocket channel
    pub community_id: Option<String>,
    /// Optional JWT token for authentication
    pub token: Option<String>,
}

/// Broadcast channels for different organization scopes
#[derive(Debug, Default, Clone)]
pub struct WebSocketState {
    /// Map of channel names to broadcast senders
    channels: Arc<RwLock<HashMap<String, broadcast::Sender<WebSocketEvent>>>>,
}

impl WebSocketState {
    /// Create a new WebSocketState
    pub fn new() -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create a broadcast channel for the given organization scope
    fn get_or_create_channel(&self, channel_name: &str) -> broadcast::Sender<WebSocketEvent> {
        let mut channels = self.channels.write().unwrap();
        channels
            .entry(channel_name.to_string())
            .or_insert_with(|| broadcast::channel(MAX_CHANNEL_CAPACITY).0)
            .clone()
    }

    /// Broadcast an event to a specific channel
    pub fn broadcast_to_channel(&self, channel_name: &str, event: WebSocketEvent) {
        let tx = self.get_or_create_channel(channel_name);
        let _ = tx.send(event); // Ignore errors (no subscribers)
    }

    /// Broadcast an event to multiple channels (e.g., for hierarchical scoping)
    pub fn broadcast_event(
        &self,
        federation_id: Option<&str>,
        coop_id: Option<&str>,
        community_id: Option<&str>,
        event: WebSocketEvent,
    ) {
        // Broadcast to the most specific channel
        let specific_channel = build_channel_name(federation_id, coop_id, community_id);
        self.broadcast_to_channel(&specific_channel, event.clone());

        // If we have community_id, also broadcast to its parent coop channel
        if community_id.is_some() && coop_id.is_some() {
            let coop_channel = build_channel_name(federation_id, coop_id, None);
            self.broadcast_to_channel(&coop_channel, event.clone());
        }

        // If we have coop_id, also broadcast to its parent federation channel
        if coop_id.is_some() && federation_id.is_some() {
            let fed_channel = build_channel_name(federation_id, None, None);
            self.broadcast_to_channel(&fed_channel, event.clone());
        }

        // Always broadcast to global channel
        self.broadcast_to_channel("global", event);
    }

    /// Start a background task that simulates events for testing
    pub fn start_simulation(self) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            
            loop {
                interval.tick().await;
                
                // Simulate different event types
                match rand::random::<u8>() % 4 {
                    0 => self.simulate_receipt_created(),
                    1 => self.simulate_token_minted(),
                    2 => self.simulate_token_transferred(),
                    _ => self.simulate_token_burned(),
                }
            }
        });
    }
    
    /// Simulate a receipt created event with random organization scope
    fn simulate_receipt_created(&self) {
        // Generate random organization scope
        let (federation_id, coop_id, community_id) = self.random_org_scope();
        
        // Create random receipt
        let receipt = ExecutionReceiptSummary {
            cid: format!("bafy2bzace{}", Uuid::new_v4()),
            executor: format!("did:icn:node{}", rand::random::<u8>() % 3 + 1),
            resource_usage: HashMap::from([
                ("CPU".to_string(), rand::random::<u16>() as u64),
                ("Memory".to_string(), (rand::random::<u16>() as u64) * 512),
            ]),
            timestamp: Utc::now(),
            coop_id: coop_id.clone(),
            community_id: community_id.clone(),
        };
        
        // Create and broadcast event
        let event = WebSocketEvent::ReceiptCreated(receipt);
        self.broadcast_event(
            federation_id.as_deref(), 
            coop_id.as_deref(), 
            community_id.as_deref(), 
            event
        );
        
        tracing::info!(
            "Simulated ReceiptCreated event for fed={:?}, coop={:?}, community={:?}", 
            federation_id, coop_id, community_id
        );
    }
    
    /// Simulate a token minted event with random organization scope
    fn simulate_token_minted(&self) {
        // Generate random organization scope
        let (federation_id, coop_id, community_id) = self.random_org_scope();
        
        // Create random token transaction
        let tx = TokenTransaction {
            id: format!("tx-{}", Uuid::new_v4()),
            from_did: "did:icn:treasury".to_string(),
            to_did: format!("did:icn:user{}", rand::random::<u8>() % 5 + 1),
            amount: (rand::random::<u16>() as u64) * 100,
            operation: "mint".to_string(),
            timestamp: Utc::now(),
            from_coop_id: None,
            from_community_id: None,
            to_coop_id: coop_id.clone(),
            to_community_id: community_id.clone(),
        };
        
        // Create and broadcast event
        let event = WebSocketEvent::TokenMinted(tx);
        self.broadcast_event(
            federation_id.as_deref(), 
            coop_id.as_deref(), 
            community_id.as_deref(), 
            event
        );
        
        tracing::info!(
            "Simulated TokenMinted event for fed={:?}, coop={:?}, community={:?}", 
            federation_id, coop_id, community_id
        );
    }
    
    /// Simulate a token transferred event with random organization scope
    fn simulate_token_transferred(&self) {
        // Generate random organization scope
        let (federation_id, coop_id, community_id) = self.random_org_scope();
        
        // Create random token transaction
        let from_user = format!("did:icn:user{}", rand::random::<u8>() % 5 + 1);
        let to_user = format!("did:icn:user{}", rand::random::<u8>() % 5 + 1);
        
        let tx = TokenTransaction {
            id: format!("tx-{}", Uuid::new_v4()),
            from_did: from_user,
            to_did: to_user,
            amount: (rand::random::<u16>() as u64) * 50,
            operation: "transfer".to_string(),
            timestamp: Utc::now(),
            from_coop_id: coop_id.clone(),
            from_community_id: community_id.clone(),
            to_coop_id: coop_id.clone(),
            to_community_id: community_id.clone(),
        };
        
        // Create and broadcast event
        let event = WebSocketEvent::TokenTransferred(tx);
        self.broadcast_event(
            federation_id.as_deref(), 
            coop_id.as_deref(), 
            community_id.as_deref(), 
            event
        );
        
        tracing::info!(
            "Simulated TokenTransferred event for fed={:?}, coop={:?}, community={:?}", 
            federation_id, coop_id, community_id
        );
    }
    
    /// Simulate a token burned event with random organization scope
    fn simulate_token_burned(&self) {
        // Generate random organization scope
        let (federation_id, coop_id, community_id) = self.random_org_scope();
        
        // Create random token transaction
        let tx = TokenTransaction {
            id: format!("tx-{}", Uuid::new_v4()),
            from_did: format!("did:icn:user{}", rand::random::<u8>() % 5 + 1),
            to_did: "did:icn:treasury".to_string(),
            amount: (rand::random::<u16>() as u64) * 25,
            operation: "burn".to_string(),
            timestamp: Utc::now(),
            from_coop_id: coop_id.clone(),
            from_community_id: community_id.clone(),
            to_coop_id: None,
            to_community_id: None,
        };
        
        // Create and broadcast event
        let event = WebSocketEvent::TokenBurned(tx);
        self.broadcast_event(
            federation_id.as_deref(), 
            coop_id.as_deref(), 
            community_id.as_deref(), 
            event
        );
        
        tracing::info!(
            "Simulated TokenBurned event for fed={:?}, coop={:?}, community={:?}", 
            federation_id, coop_id, community_id
        );
    }
    
    /// Generate a random organization scope
    fn random_org_scope(&self) -> (Option<String>, Option<String>, Option<String>) {
        let scope_type = rand::random::<u8>() % 4;
        
        match scope_type {
            0 => (None, None, None), // Global scope
            1 => (Some(format!("fed{}", rand::random::<u8>() % 3 + 1)), None, None), // Federation scope
            2 => {
                let fed_id = format!("fed{}", rand::random::<u8>() % 3 + 1);
                let coop_id = format!("coop{}", rand::random::<u8>() % 3 + 1);
                (Some(fed_id), Some(coop_id), None) // Cooperative scope
            },
            _ => {
                let fed_id = format!("fed{}", rand::random::<u8>() % 3 + 1);
                let coop_id = format!("coop{}", rand::random::<u8>() % 3 + 1);
                let comm_id = format!("comm{}", rand::random::<u8>() % 3 + 1);
                (Some(fed_id), Some(coop_id), Some(comm_id)) // Community scope
            }
        }
    }

    /// Broadcast a message to all connected clients on a channel
    pub fn broadcast(&self, channel: &str, message: serde_json::Value) {
        // Convert the message to a string
        if let Ok(message_string) = serde_json::to_string(&message) {
            // Get or create a channel for the given name
            let tx = self.get_or_create_channel(channel);
            
            // Create a WebSocket Message from the string
            let ws_message = Message::Text(message_string);
            
            // Send the message (WebSocketEvent::Custom would be ideal here, but we'll use the existing system)
            let custom_event = WebSocketEvent::Custom(message);
            let _ = tx.send(custom_event); // Ignore errors if no subscribers
            
            tracing::debug!("Broadcast message to channel: {}", channel);
        } else {
            tracing::error!("Failed to serialize message for broadcast");
        }
    }
    
    /// Send an event with a specific type directly to a named channel
    pub fn send_event_to_channel(&self, channel: &str, event_type: &str, data: &serde_json::Value) {
        let message = serde_json::json!({
            "event": event_type,
            "data": data
        });
        self.broadcast(channel, message);
    }
}

/// WebSocket handler for real-time updates
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WebSocketParams>,
    State((db, ws_state, jwt_config)): State<(Db, WebSocketState, Arc<JwtConfig>)>,
) -> impl IntoResponse {
    // Validate organization scope hierarchy
    if let Some(err) = validate_org_scope_hierarchy(&params) {
        tracing::warn!("Invalid organization scope: {}", err);
        return (
            axum::http::StatusCode::BAD_REQUEST,
            format!("Invalid organization scope: {}", err),
        ).into_response();
    }
    
    // Build the channel name based on organization scope
    let channel_name = build_channel_name(
        params.federation_id.as_deref(),
        params.coop_id.as_deref(),
        params.community_id.as_deref(),
    );
    
    // Log the connection
    tracing::info!(
        "WebSocket connection requested for channel: {}", 
        channel_name
    );
    
    // Perform JWT verification if token is provided
    let scope_claims = if let Some(token) = params.token {
        match validate_token(&token, &jwt_config) {
            Ok(claims) => {
                let scope_claims: ScopeClaims = claims.into();
                
                // Check if the user has access to the requested organization scope
                if !scope_claims.has_org_scope_access(
                    params.federation_id.as_deref(),
                    params.coop_id.as_deref(),
                    params.community_id.as_deref(),
                ) {
                    tracing::warn!("Unauthorized organization access attempt");
                    return (
                        axum::http::StatusCode::FORBIDDEN,
                        "Unauthorized: You do not have access to this organization scope",
                    ).into_response();
                }
                
                Some(scope_claims)
            },
            Err(err) => {
                tracing::warn!("JWT validation failed: {}", err);
                return (
                    axum::http::StatusCode::UNAUTHORIZED,
                    format!("Unauthorized: {}", err),
                ).into_response();
            }
        }
    } else {
        // For now, allow connections without tokens for development/testing purposes
        // In production, you should enforce token authentication
        tracing::warn!("WebSocket connection without token - this should be disabled in production");
        None
    };
    
    // Upgrade to WebSocket
    ws.on_upgrade(move |socket| websocket_connection(socket, channel_name, ws_state, scope_claims))
}

/// Validate organization scope hierarchy
/// This ensures that we don't have invalid combinations like a community without a coop
fn validate_org_scope_hierarchy(params: &WebSocketParams) -> Option<String> {
    // Community ID requires Cooperative ID
    if params.community_id.is_some() && params.coop_id.is_none() {
        return Some("Cannot specify a community without a cooperative".into());
    }
    
    // Cooperative ID requires Federation ID
    if params.coop_id.is_some() && params.federation_id.is_none() {
        return Some("Cannot specify a cooperative without a federation".into());
    }
    
    None
}

/// Handle WebSocket connection for a specific channel
async fn websocket_connection(
    socket: WebSocket, 
    channel_name: String, 
    ws_state: WebSocketState,
    scope_claims: Option<ScopeClaims>,
) {
    // Split the socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();
    
    // Get the broadcast channel
    let tx = ws_state.get_or_create_channel(&channel_name);
    let mut rx = tx.subscribe();
    
    // Generate client ID
    let client_id = Uuid::new_v4().to_string();
    tracing::info!("Client connected: {} to channel {}", client_id, channel_name);
    
    // Clone client_id for use in tasks
    let client_id_for_task = client_id.clone();
    
    // Task for sending messages to the WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            // Serialize the event to JSON
            if let Ok(json) = serde_json::to_string(&event) {
                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });
    
    // Task for receiving messages from the WebSocket (for ping/pong or commands)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    tracing::debug!("Received text message: {}", text);
                    // Handle commands if needed
                },
                Message::Ping(ping) => {
                    tracing::debug!("Received ping, pong will be sent automatically by axum");
                    // Axum automatically responds to pings with pongs, no need to do it manually
                },
                Message::Close(_) => {
                    tracing::info!("Client requested close: {}", client_id_for_task);
                    break;
                },
                _ => { /* Ignore other message types */ }
            }
        }
    });
    
    // Wait for either task to complete
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
    
    tracing::info!("Client disconnected: {} from channel {}", client_id, channel_name);
}

/// Helper function to create a WebSocket router
pub fn websocket_routes() -> Router<(Db, WebSocketState, Arc<JwtConfig>)> {
    Router::new()
        .route("/ws", get(websocket_handler))
        .route("/ws/:federation_id", get(federation_websocket_handler))
        .route("/ws/:federation_id/:coop_id", get(coop_websocket_handler))
        .route("/ws/:federation_id/:coop_id/:community_id", get(community_websocket_handler))
}

/// WebSocket handler for federation-specific channels
async fn federation_websocket_handler(
    ws: WebSocketUpgrade,
    Path(federation_id): Path<String>,
    Query(params): Query<WebSocketParams>,
    State((db, ws_state, jwt_config)): State<(Db, WebSocketState, Arc<JwtConfig>)>,
) -> impl IntoResponse {
    // Combine path and query parameters
    let combined_params = WebSocketParams {
        federation_id: Some(federation_id),
        coop_id: params.coop_id,
        community_id: params.community_id,
        token: params.token,
    };
    
    websocket_handler(ws, Query(combined_params), State((db, ws_state, jwt_config))).await
}

/// WebSocket handler for cooperative-specific channels
async fn coop_websocket_handler(
    ws: WebSocketUpgrade,
    Path((federation_id, coop_id)): Path<(String, String)>,
    Query(params): Query<WebSocketParams>,
    State((db, ws_state, jwt_config)): State<(Db, WebSocketState, Arc<JwtConfig>)>,
) -> impl IntoResponse {
    // Combine path and query parameters
    let combined_params = WebSocketParams {
        federation_id: Some(federation_id),
        coop_id: Some(coop_id),
        community_id: params.community_id,
        token: params.token,
    };
    
    websocket_handler(ws, Query(combined_params), State((db, ws_state, jwt_config))).await
}

/// WebSocket handler for community-specific channels
async fn community_websocket_handler(
    ws: WebSocketUpgrade,
    Path((federation_id, coop_id, community_id)): Path<(String, String, String)>,
    Query(params): Query<WebSocketParams>,
    State((db, ws_state, jwt_config)): State<(Db, WebSocketState, Arc<JwtConfig>)>,
) -> impl IntoResponse {
    // Combine path and query parameters
    let combined_params = WebSocketParams {
        federation_id: Some(federation_id),
        coop_id: Some(coop_id),
        community_id: Some(community_id),
        token: params.token,
    };
    
    websocket_handler(ws, Query(combined_params), State((db, ws_state, jwt_config))).await
} 