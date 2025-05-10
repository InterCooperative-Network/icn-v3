use std::net::SocketAddr;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use metrics_util::MetricKindMask;
use tokio::task;

/// Metrics prefix for ICN Agoranet metrics
pub const METRICS_PREFIX: &str = "icn_agoranet";

/// Labels used for metrics dimensions
pub mod labels {
    pub const FEDERATION: &str = "federation";
    pub const ENTITY_TYPE: &str = "entity_type";
    pub const OPERATION: &str = "operation";
    pub const STATUS: &str = "status";
    pub const ERROR_TYPE: &str = "error_type";
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