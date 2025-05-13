use prometheus::{IntCounter, register_int_counter, opts};
use prometheus::{Histogram, register_histogram};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref REPUTATION_SUBMISSION_ATTEMPTS_TOTAL: IntCounter = 
        register_int_counter!(
            opts!("icn_runtime_reputation_submission_attempts_total", "Total attempts to submit reputation records")
        ).unwrap();
        
    pub static ref REPUTATION_SUBMISSION_SUCCESS_TOTAL: IntCounter = 
        register_int_counter!(
            opts!("icn_runtime_reputation_submission_success_total", "Number of successful reputation submissions")
        ).unwrap();
        
    pub static ref REPUTATION_SUBMISSION_FAILURE_TOTAL: IntCounter = 
        register_int_counter!(
            opts!("icn_runtime_reputation_submission_failure_total", "Number of failed reputation submissions")
        ).unwrap();

    pub static ref RECEIPT_VERIFICATION_SUCCESS_TOTAL: IntCounter = 
        register_int_counter!(
            opts!("icn_runtime_receipt_verification_success_total", "Total receipts passing signature verification")
        ).unwrap();
        
    pub static ref RECEIPT_VERIFICATION_FAILURE_TOTAL: IntCounter = 
        register_int_counter!(
            opts!("icn_runtime_receipt_verification_failure_total", "Total receipts failing signature verification")
        ).unwrap();
        
    pub static ref RECEIPT_MANA_COST_TOTAL: IntCounter = 
        register_int_counter!(
            opts!("icn_runtime_receipt_mana_cost_total", "Total mana cost recorded from executed receipts")
        ).unwrap();
        
    pub static ref ANCHOR_RECEIPT_DURATION_SECONDS: Histogram = 
        register_histogram!(
            "icn_runtime_anchor_receipt_duration_seconds",
            "Histogram of the time taken to anchor a receipt (including verification, storage, reputation submission)",
            vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
        ).unwrap();
}

/// Increment the total count of reputation updates
pub fn record_reputation_update_attempt() {
    REPUTATION_SUBMISSION_ATTEMPTS_TOTAL.inc();
}

/// Increment the count of successful reputation updates
pub fn record_reputation_update_success() {
    REPUTATION_SUBMISSION_SUCCESS_TOTAL.inc();
}

/// Increment the count of failed reputation updates
pub fn record_reputation_update_failure() {
    REPUTATION_SUBMISSION_FAILURE_TOTAL.inc();
}

/// Increment the count of successful receipt verifications
pub fn record_receipt_verification_success() {
    RECEIPT_VERIFICATION_SUCCESS_TOTAL.inc();
}

/// Increment the count of failed receipt verifications
pub fn record_receipt_verification_failure() {
    RECEIPT_VERIFICATION_FAILURE_TOTAL.inc();
}

/// Add the mana cost from a receipt to the total
pub fn record_receipt_mana_cost(cost: u64) {
    RECEIPT_MANA_COST_TOTAL.inc_by(cost);
}

/// Observe the duration of the anchor_receipt operation
pub fn observe_anchor_receipt_duration(duration_secs: f64) {
    ANCHOR_RECEIPT_DURATION_SECONDS.observe(duration_secs);
} 