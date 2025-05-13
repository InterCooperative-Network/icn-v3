use lazy_static::lazy_static;
use prometheus::{
    opts, register_counter, register_counter_vec, register_histogram, register_histogram_vec, Counter,
    CounterVec, Histogram, HistogramVec,
};

// --- Metric Label Definitions ---
const LABEL_RESULT: &str = "result"; // "success" or "failure"
const LABEL_PROCESSING_STAGE: &str = "stage"; // e.g., "receipt_cid_generation", "receipt_anchoring_initiation"

// --- General Job Lifecycle Metrics ---
lazy_static! {
    pub static ref MESH_JOBS_RECEIVED_TOTAL: Counter = register_counter!(
        opts!("icn_mesh_jobs_received_total", "Total number of mesh jobs received by this node for potential execution.")
    ).unwrap();

    pub static ref MESH_JOBS_ATTEMPTED_TOTAL: Counter = register_counter!(
        opts!("icn_mesh_jobs_execution_attempted_total", "Total number of mesh jobs this node attempted to execute.")
    ).unwrap();

    pub static ref MESH_JOBS_EXECUTED_TOTAL: CounterVec = register_counter_vec!(
        opts!("icn_mesh_jobs_executed_total", "Total number of mesh jobs executed by this node, labeled by result."),
        &[LABEL_RESULT]
    ).unwrap();

    pub static ref MESH_JOB_EXECUTION_DURATION_SECONDS: Histogram = register_histogram!(
        "icn_mesh_job_execution_duration_seconds",
        "Histogram of mesh job execution durations on this node.",
        // Buckets in seconds: 100ms, 500ms, 1s, 2.5s, 5s, 10s, 30s, 1m, 2m, 5m, 10m, 30m, 1hr
        vec![0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0]
    ).unwrap();
}

// --- Execution Receipt Specific Metrics (Local to Mesh Node) ---
lazy_static! {
    pub static ref MESH_RECEIPTS_CREATED_TOTAL: Counter = register_counter!(
        opts!("icn_mesh_receipts_created_total", "Total number of mesh execution receipts created locally by this node.")
    ).unwrap();

    pub static ref MESH_RECEIPTS_SIGNED_TOTAL: CounterVec = register_counter_vec!(
        opts!("icn_mesh_receipts_signed_total", "Total number of mesh execution receipts signed locally, labeled by result."),
        &[LABEL_RESULT]
    ).unwrap();

    pub static ref MESH_RECEIPT_SIGNING_DURATION_SECONDS: Histogram = register_histogram!(
        "icn_mesh_receipt_signing_duration_seconds",
        "Histogram of mesh execution receipt signing durations locally on this node.",
        // Buckets in seconds: 1ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5]
    ).unwrap();

    // Metric for errors during local processing stages of a receipt before it's fully handed off.
    pub static ref MESH_RECEIPT_LOCAL_PROCESSING_ERRORS_TOTAL: CounterVec = register_counter_vec!(
        opts!("icn_mesh_receipt_local_processing_errors_total", "Total errors during local mesh receipt processing stages (e.g., CID generation, anchor call prep). Labelled by stage."),
        &[LABEL_PROCESSING_STAGE]
    ).unwrap();
}


// --- Helper Functions to Record Metrics ---

// Job Lifecycle
#[inline]
pub fn jobs_received_inc() {
    MESH_JOBS_RECEIVED_TOTAL.inc();
}

#[inline]
pub fn jobs_execution_attempted_inc() {
    MESH_JOBS_ATTEMPTED_TOTAL.inc();
}

#[inline]
pub fn job_execution_observe(duration_seconds: f64, success: bool) {
    let result_label = if success { "success" } else { "failure" };
    MESH_JOBS_EXECUTED_TOTAL.with_label_values(&[result_label]).inc();
    MESH_JOB_EXECUTION_DURATION_SECONDS.observe(duration_seconds);
}

// Receipt Handling (Local to Mesh Node)
#[inline]
pub fn receipts_created_inc() {
    MESH_RECEIPTS_CREATED_TOTAL.inc();
}

#[inline]
pub fn receipt_signing_observe(duration_seconds: f64, success: bool) {
    let result_label = if success { "success" } else { "failure" };
    MESH_RECEIPTS_SIGNED_TOTAL.with_label_values(&[result_label]).inc();
    // Only observe duration for successful signings for this specific histogram.
    // Failed signing durations could be a separate metric if valuable.
    if success {
        MESH_RECEIPT_SIGNING_DURATION_SECONDS.observe(duration_seconds);
    }
}

#[inline]
pub fn receipt_local_processing_error_inc(stage: &str) {
    MESH_RECEIPT_LOCAL_PROCESSING_ERRORS_TOTAL.with_label_values(&[stage]).inc();
} 