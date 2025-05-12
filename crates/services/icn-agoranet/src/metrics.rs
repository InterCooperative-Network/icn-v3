use std::net::SocketAddr;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use metrics_util::MetricKindMask;
use tokio::task;
use std::time::Duration;

/// Metrics prefix for ICN Agoranet metrics
pub const METRICS_PREFIX: &str = "icn_agoranet";

/// Labels used for metrics dimensions
pub mod labels {
    pub const FEDERATION: &str = "federation_id";
    pub const ENTITY_TYPE: &str = "entity_type";
    pub const OPERATION: &str = "operation";
    pub const STATUS: &str = "status";
    pub const ERROR_TYPE: &str = "error_type";
    pub const CURRENCY: &str = "currency";
    pub const ENTITY_ID: &str = "entity_id";
    pub const HTTP_REQUESTS_TOTAL: &str = "http_requests_total";
    pub const HTTP_REQUEST_DURATION_SECONDS: &str = "http_request_duration_seconds";
    pub const WEBSOCKET_CONNECTIONS_TOTAL: &str = "websocket_connections_total";
    pub const WEBSOCKET_MESSAGES_TOTAL: &str = "websocket_messages_total";
    pub const SERVICE_INFO: &str = "service_info";
    pub const METHOD: &str = "method";
    pub const PATH: &str = "path";
    pub const DIRECTION: &str = "direction";
}

/// Ledger operation types for metrics labeling
pub mod operations {
    pub const TRANSFER: &str = "transfer";
    pub const BATCH_TRANSFER: &str = "batch_transfer";
    pub const QUERY: &str = "query";
    pub const BALANCE: &str = "balance";
    pub const ENSURE_ENTITY: &str = "ensure_entity";
}

/// Status values for metrics labeling
pub mod status {
    pub const SUCCESS: &str = "success";
    pub const ERROR: &str = "error";
}

/// Sets up the Prometheus metrics registry with sensible defaults
pub fn setup_metrics_recorder() -> PrometheusHandle {
    let builder = PrometheusBuilder::new();
    
    // Create a recorder that buckets histogram values
    let builder = builder
        .set_buckets_for_metric(
            Matcher::Full("icn_agoranet_transfer_latency_seconds".to_string()),
            &[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0],
        )
        .unwrap();
    
    // Install global recorder
    builder.install_recorder().unwrap()
}

/// Spawn a metrics exporter server that serves Prometheus metrics
pub fn spawn_metrics_exporter(handle: PrometheusHandle, addr: SocketAddr) -> task::JoinHandle<()> {
    // Spawn a separate web server for metrics
    task::spawn(async move {
        let app = axum::Router::new()
            .route(
                "/metrics",
                axum::routing::get(move || std::future::ready(handle.render())),
            );
        
        tracing::info!("Starting metrics exporter on {}", addr);
        axum::serve(
            tokio::net::TcpListener::bind(addr).await.unwrap(),
            app,
        )
        .await
        .unwrap();
    })
}

/// Helper macro to record ledger operation timing
#[macro_export]
macro_rules! time_ledger_op {
    ($operation:expr, $federation_id:expr, $entity_type:expr, $code:block) => {{
        let timer = metrics::histogram!(
            concat!($crate::metrics::METRICS_PREFIX, "_transfer_latency_seconds"),
            $crate::metrics::labels::OPERATION => $operation,
            $crate::metrics::labels::FEDERATION => $federation_id,
            $crate::metrics::labels::ENTITY_TYPE => $entity_type
        )
        .start();
        
        let result = $code;
        
        // Use drop to explicitly end the timer and record the result
        drop(timer);
        
        // Count success or failure based on the result
        match &result {
            Ok(_) => {
                metrics::counter!(
                    concat!($crate::metrics::METRICS_PREFIX, "_operations_total"),
                    $crate::metrics::labels::OPERATION => $operation,
                    $crate::metrics::labels::FEDERATION => $federation_id,
                    $crate::metrics::labels::ENTITY_TYPE => $entity_type,
                    $crate::metrics::labels::STATUS => $crate::metrics::status::SUCCESS
                )
                .increment(1);
            }
            Err(e) => {
                // Extract error type for more granular metrics
                let error_type = format!("{:?}", e);
                metrics::counter!(
                    concat!($crate::metrics::METRICS_PREFIX, "_operations_total"),
                    $crate::metrics::labels::OPERATION => $operation,
                    $crate::metrics::labels::FEDERATION => $federation_id,
                    $crate::metrics::labels::ENTITY_TYPE => $entity_type,
                    $crate::metrics::labels::STATUS => $crate::metrics::status::ERROR,
                    $crate::metrics::labels::ERROR_TYPE => error_type
                )
                .increment(1);
            }
        }
        
        result
    }};
}

/// Helper function to track resource gauges (like entity counts)
pub fn update_resource_gauge(
    metric_name: &str, 
    value: u64, 
    federation_id: &str,
    labels: &[(&str, &str)]
) {
    // Create a metric name with the prefix
    let full_metric_name = format!("{}_{}", METRICS_PREFIX, metric_name);
    
    // For simplicity, we'll implement specific common patterns
    // This avoids issues with the metrics! macro which needs static labels
    
    // Just federation ID
    if labels.is_empty() {
        metrics::gauge!(&full_metric_name, labels::FEDERATION => federation_id).set(value as f64);
        return;
    }
    
    // Federation ID + entity type (most common case)
    if labels.len() == 1 && labels[0].0 == labels::ENTITY_TYPE {
        metrics::gauge!(
            &full_metric_name, 
            labels::FEDERATION => federation_id,
            labels::ENTITY_TYPE => labels[0].1
        ).set(value as f64);
        return;
    }
    
    // Federation ID + operation type
    if labels.len() == 1 && labels[0].0 == labels::OPERATION {
        metrics::gauge!(
            &full_metric_name, 
            labels::FEDERATION => federation_id,
            labels::OPERATION => labels[0].1
        ).set(value as f64);
        return;
    }
    
    // Federation ID + status
    if labels.len() == 1 && labels[0].0 == labels::STATUS {
        metrics::gauge!(
            &full_metric_name, 
            labels::FEDERATION => federation_id,
            labels::STATUS => labels[0].1
        ).set(value as f64);
        return;
    }
    
    // Federation ID + entity type + operation (common for operation counts)
    if labels.len() == 2 && 
       labels[0].0 == labels::ENTITY_TYPE && 
       labels[1].0 == labels::OPERATION {
        metrics::gauge!(
            &full_metric_name, 
            labels::FEDERATION => federation_id,
            labels::ENTITY_TYPE => labels[0].1,
            labels::OPERATION => labels[1].1
        ).set(value as f64);
        return;
    }
    
    // If we get here, we have an unsupported label combination
    // Log a warning and use just the federation ID
    tracing::warn!(
        "Unsupported label combination for gauge metric {}: {:?}. Using only federation ID.",
        full_metric_name, 
        labels
    );
    metrics::gauge!(&full_metric_name, labels::FEDERATION => federation_id).set(value as f64);
}

pub fn set_ledger_fee_revenue(federation_id: &str, currency: &str, amount: u64) {
    let full_metric_name = format!("agoranet_ledger_fee_revenue_total_{}", currency);
    let labels = [
        (labels::CURRENCY, currency.to_string()),
        (labels::FEDERATION, federation_id.to_string())
    ];
    metrics::counter!(&full_metric_name, &labels).increment(amount);
}

pub fn set_ledger_balance(federation_id: &str, currency: &str, entity_type: &str, entity_id: &str, value: u64) {
    let full_metric_name = format!("agoranet_ledger_balance_{}", currency);
    let labels = [
        (labels::CURRENCY, currency.to_string()),
        (labels::FEDERATION, federation_id.to_string()),
        (labels::ENTITY_TYPE, entity_type.to_string()),
        (labels::ENTITY_ID, entity_id.to_string())
    ];
    metrics::gauge!(&full_metric_name, &labels).set(value as f64);
}

pub fn set_ledger_total_supply(federation_id: &str, currency: &str, value: u64) {
    let full_metric_name = format!("agoranet_ledger_total_supply_{}", currency);
    metrics::gauge!(&full_metric_name, labels::CURRENCY = currency.to_string(), labels::FEDERATION = federation_id.to_string()).set(value as f64);
}

pub fn set_federation_stat(federation_id: &str, stat_name: &str, value: u64) {
    let full_metric_name = format!("agoranet_federation_{}", stat_name);
    metrics::gauge!(&full_metric_name, labels::FEDERATION = federation_id.to_string()).set(value as f64);
}

pub fn increment_http_request_count(method: &str, path: &str) {
    metrics::counter!(labels::HTTP_REQUESTS_TOTAL, labels::METHOD = method.to_string(), labels::PATH = path.to_string()).increment(1);
}

pub fn record_http_request_duration(method: &str, path: &str, duration: Duration) {
    metrics::histogram!(labels::HTTP_REQUEST_DURATION_SECONDS, labels::METHOD = method.to_string(), labels::PATH = path.to_string()).record(duration.as_secs_f64());
}

pub fn increment_websocket_connection_count(federation_id: &str) {
    metrics::counter!(labels::WEBSOCKET_CONNECTIONS_TOTAL, labels::FEDERATION = federation_id.to_string()).increment(1);
}

pub fn decrement_websocket_connection_count(federation_id: &str) {
    metrics::counter!(labels::WEBSOCKET_CONNECTIONS_TOTAL, labels::FEDERATION = federation_id.to_string()).decrement(1);
}

pub fn increment_websocket_message_count(federation_id: &str, direction: &str) {
    metrics::counter!(labels::WEBSOCKET_MESSAGES_TOTAL, labels::FEDERATION = federation_id.to_string(), labels::DIRECTION = direction.to_string()).increment(1);
}

pub fn set_service_info(version: &str) {
    metrics::gauge!(labels::SERVICE_INFO, "version" = version.to_string()).set(1.0);
}

pub fn describe_metrics() {
    // Counters
    metrics::describe_counter!(labels::HTTP_REQUESTS_TOTAL, "Total number of HTTP requests received.");
    metrics::describe_counter!("agoranet_ledger_transfers_total", "Total number of ledger transfers processed.");
    metrics::describe_counter!("agoranet_ledger_fee_revenue_total_icn", "Total fee revenue collected in ICN."); 
    metrics::describe_counter!(labels::WEBSOCKET_CONNECTIONS_TOTAL, "Total number of WebSocket connections established.");
    metrics::describe_counter!(labels::WEBSOCKET_MESSAGES_TOTAL, "Total number of WebSocket messages processed.");

    // Gauges
    metrics::describe_gauge!("agoranet_ledger_balance_icn", "Current balance of an entity in ICN.");
    metrics::describe_gauge!("agoranet_ledger_total_supply_icn", "Total supply of ICN tokens in a federation.");
    metrics::describe_gauge!("agoranet_federation_active_entities", "Number of active entities in a federation.");
    metrics::describe_gauge!("agoranet_federation_total_entities", "Total number of entities in a federation.");
    metrics::describe_gauge!(labels::SERVICE_INFO, "Basic service information.");
    metrics::describe_gauge!("agoranet_websocket_active_connections", "Current number of active WebSocket connections.");

    // Histograms
    metrics::describe_histogram!(labels::HTTP_REQUEST_DURATION_SECONDS, metrics::Unit::Seconds, "Duration of HTTP requests.");
}

pub fn increment_ledger_transfer_count(federation_id: &str, currency: &str) {
    let full_metric_name = format!("agoranet_ledger_transfers_total_{}", currency);
    let labels = [
        (labels::CURRENCY, currency.to_string()),
        (labels::FEDERATION, federation_id.to_string())
    ];
    metrics::counter!(&full_metric_name, &labels).increment(1);
}

pub fn increment_ledger_fee_revenue(federation_id: &str, currency: &str, amount: u64) {
    let full_metric_name = format!("agoranet_ledger_fee_revenue_total_{}", currency);
    let labels = [
        (labels::CURRENCY, currency.to_string()),
        (labels::FEDERATION, federation_id.to_string())
    ];
    metrics::counter!(&full_metric_name, &labels).increment(amount);
}