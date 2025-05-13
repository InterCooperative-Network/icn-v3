use prometheus::{IntCounter, register_int_counter, opts};
use prometheus::{Histogram, register_histogram};
use prometheus::{IntCounterVec, register_int_counter_vec};
use prometheus::{HistogramVec, register_histogram_vec};
use lazy_static::lazy_static;
use std::sync::Arc;
use icn_economics::{ScopeKey, ManaMetricsHook};
use prometheus::{GaugeVec, register_gauge_vec, Registry};

// Define standard label names
const LABEL_COOP_ID: &str = "coop_id";
const LABEL_COMMUNITY_ID: &str = "community_id";
const LABEL_ISSUER_DID: &str = "issuer_did";
const LABEL_EXECUTOR_DID: &str = "executor_did";
const LABEL_RESULT: &str = "result";
const LABEL_SUCCESS: &str = "success";

// Example buckets for score deltas, adjust as needed
const SCORE_DELTA_BUCKETS: &[f64] = &[-100.0, -50.0, -25.0, -10.0, 0.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0];

lazy_static! {
    // --- Reputation System Metrics ---
    pub static ref REPUTATION_SUBMISSIONS_BY_RESULT: IntCounterVec =
        register_int_counter_vec!(
            opts!("icn_runtime_reputation_submissions_by_result", "Reputation submissions categorized by result (success/failure) and tagged with federation and executor identifiers."),
            &[LABEL_SUCCESS, LABEL_COOP_ID, LABEL_COMMUNITY_ID, LABEL_EXECUTOR_DID]
        ).unwrap();

    pub static ref REPUTATION_SCORE_DELTA_HISTOGRAM: HistogramVec =
        register_histogram_vec!(
            "icn_runtime_reputation_score_delta_histogram",
            "Distribution of reputation score deltas, tagged with federation and executor identifiers.",
            &[LABEL_COOP_ID, LABEL_COMMUNITY_ID, LABEL_EXECUTOR_DID],
            SCORE_DELTA_BUCKETS.to_vec()
        ).unwrap();

    // --- Receipt Processing Metrics ---
    pub static ref RECEIPT_VERIFICATIONS_TOTAL: IntCounterVec =
        register_int_counter_vec!(
            opts!("icn_runtime_receipt_verifications_total", "Total receipts processed for verification, tagged by result and federation/issuer identifiers."),
            &[LABEL_RESULT, LABEL_COOP_ID, LABEL_COMMUNITY_ID, LABEL_ISSUER_DID]
        ).unwrap();
        
    pub static ref RECEIPT_MANA_COST_TOTAL: IntCounterVec =
        register_int_counter_vec!(
            opts!("icn_runtime_receipt_mana_cost_total", "Total mana cost recorded from executed receipts, tagged by federation/issuer identifiers."),
            &[LABEL_COOP_ID, LABEL_COMMUNITY_ID, LABEL_ISSUER_DID]
        ).unwrap();
        
    pub static ref ANCHOR_RECEIPT_DURATION_SECONDS: HistogramVec =
        register_histogram_vec!(
            "icn_runtime_anchor_receipt_duration_seconds",
            "Histogram of the time taken to anchor a receipt (including verification, storage, reputation submission), tagged by federation/issuer identifiers.",
            &[LABEL_COOP_ID, LABEL_COMMUNITY_ID, LABEL_ISSUER_DID],
            vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
        ).unwrap();

    // --- Mana Metrics ---
    pub static ref MANA_COST_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "icn_mana_cost_distribution",
        "Distribution of mana costs by executor",
        &["executor_did"], // Label is executor_did
        // Buckets suitable for typical mana costs (adjust if needed)
        vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 5000.0]
    ).unwrap();
}

// --- Helper Functions for Reputation Metrics ---

/// Records a reputation submission attempt and its outcome (success/failure).
///
/// # Arguments
/// * `success` - Boolean indicating if the submission was successful.
/// * `coop_id` - Identifier for the cooperative.
/// * `community_id` - Identifier for the community.
/// * `executor_did` - DID of the executor node whose reputation is being updated.
pub fn increment_reputation_submission(success: bool, coop_id: &str, community_id: &str, executor_did: &str) {
    REPUTATION_SUBMISSIONS_BY_RESULT.with_label_values(&[
        if success { "true" } else { "false" },
        coop_id,
        community_id,
        executor_did
    ]).inc();
}

/// Observes the delta of a reputation score change.
///
/// # Arguments
/// * `delta` - The change in reputation score.
/// * `coop_id` - Identifier for the cooperative.
/// * `community_id` - Identifier for the community.
/// * `executor_did` - DID of the executor node whose reputation score changed.
pub fn observe_reputation_score_delta(delta: f64, coop_id: &str, community_id: &str, executor_did: &str) {
    REPUTATION_SCORE_DELTA_HISTOGRAM.with_label_values(&[
        coop_id,
        community_id,
        executor_did
    ]).observe(delta);
}


// --- Helper Functions for Receipt Processing Metrics ---

/// Records the outcome of a receipt verification attempt.
///
/// # Arguments
/// * `is_successful` - Boolean indicating if the verification passed.
/// * `coop_id` - Identifier for the cooperative.
/// * `community_id` - Identifier for the community.
/// * `issuer_did` - DID of the receipt issuer.
pub fn record_receipt_verification_outcome(is_successful: bool, coop_id: &str, community_id: &str, issuer_did: &str) {
    RECEIPT_VERIFICATIONS_TOTAL.with_label_values(&[
        if is_successful { "success" } else { "failure" },
        coop_id,
        community_id,
        issuer_did
    ]).inc();
}

/// Adds the mana cost from a receipt to the total, tagged with identifiers.
///
/// # Arguments
/// * `cost` - The mana cost from the receipt.
/// * `coop_id` - Identifier for the cooperative.
/// * `community_id` - Identifier for the community.
/// * `issuer_did` - DID of the receipt issuer.
pub fn record_receipt_mana_cost(cost: u64, coop_id: &str, community_id: &str, issuer_did: &str) {
    RECEIPT_MANA_COST_TOTAL.with_label_values(&[
        coop_id,
        community_id,
        issuer_did
    ]).inc_by(cost);
}

/// Observes the duration of the anchor_receipt operation, tagged with identifiers.
///
/// # Arguments
/// * `duration_secs` - The duration of the operation in seconds.
/// * `coop_id` - Identifier for the cooperative.
/// * `community_id` - Identifier for the community.
/// * `issuer_did` - DID of the receipt issuer.
pub fn observe_anchor_receipt_duration(duration_secs: f64, coop_id: &str, community_id: &str, issuer_did: &str) {
    ANCHOR_RECEIPT_DURATION_SECONDS.with_label_values(&[
        coop_id,
        community_id,
        issuer_did
    ]).observe(duration_secs);
}

// PrometheusManaMetrics and its implementations as per user's latest request
#[derive(Debug)]
pub struct PrometheusManaMetrics {
    gauge: GaugeVec,
}

impl PrometheusManaMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            gauge: register_gauge_vec!(
                "icn_mana_pool_balance",
                "Current mana balance per scope",
                &["scope_type", "scope_id"]
            )
            .expect("Failed to register mana pool balance gauge"),
        })
    }
}

impl ManaMetricsHook for PrometheusManaMetrics {
    fn update_balance(&self, scope: &ScopeKey, balance: u64) {
        let (scope_type, scope_id_val) = match scope {
            ScopeKey::Individual(did) => ("individual".to_string(), did.to_string()),
            ScopeKey::Cooperative(id) => ("cooperative".to_string(), id.clone()),
            ScopeKey::Community(id) => ("community".to_string(), id.clone()),
            ScopeKey::Federation(id) => ("federation".to_string(), id.clone()),
        };
        self.gauge
            .with_label_values(&[&scope_type, &scope_id_val])
            .set(balance as f64);
    }
} 