// src/metrics_server.rs

use axum::{
    routing::get,
    response::Html, // Use Html for plain text response
    Router,
};
use prometheus::{Encoder, TextEncoder, gather};
use std::net::SocketAddr;

/// Handler for the /metrics endpoint
async fn metrics_handler() -> Html<String> {
    let encoder = TextEncoder::new();
    let metric_families = gather();
    let mut buffer = vec![];
    match encoder.encode(&metric_families, &mut buffer) {
        Ok(_) => {
            match String::from_utf8(buffer) {
                Ok(metrics_text) => Html(metrics_text),
                Err(e) => {
                    tracing::error!("Failed to convert Prometheus buffer to UTF-8: {}", e);
                    Html("# ERROR: Failed to convert buffer to UTF-8\n".to_string())
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to encode Prometheus metrics: {}", e);
            Html(format!("# ERROR: Failed to encode metrics: {}\n", e))
        }
    }
}

/// Starts the Prometheus metrics server on the given address.
/// This function runs indefinitely.
pub async fn run_metrics_server(addr: SocketAddr) {
    let app = Router::new().route("/metrics", get(metrics_handler));

    info!("Metrics server listening on {}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("Failed to bind metrics server to {}: {}", addr, e);
            return;
        }
    };
    
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("Metrics server failed: {}", e);
    }
} 