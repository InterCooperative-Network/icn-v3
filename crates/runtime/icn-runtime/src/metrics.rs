use prometheus::{IntCounter, register_int_counter, opts};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref REPUTATION_UPDATES_TOTAL: IntCounter = 
        register_int_counter!(
            opts!("icn_runtime_reputation_updates_total", "Total number of reputation updates triggered")
        ).unwrap();
        
    pub static ref REPUTATION_UPDATES_SUCCESS: IntCounter = 
        register_int_counter!(
            opts!("icn_runtime_reputation_updates_success", "Number of successful reputation updates")
        ).unwrap();
        
    pub static ref REPUTATION_UPDATES_FAILURE: IntCounter = 
        register_int_counter!(
            opts!("icn_runtime_reputation_updates_failure", "Number of failed reputation updates")
        ).unwrap();
}

/// Increment the total count of reputation updates
pub fn record_reputation_update_attempt() {
    REPUTATION_UPDATES_TOTAL.inc();
}

/// Increment the count of successful reputation updates
pub fn record_reputation_update_success() {
    REPUTATION_UPDATES_SUCCESS.inc();
}

/// Increment the count of failed reputation updates
pub fn record_reputation_update_failure() {
    REPUTATION_UPDATES_FAILURE.inc();
} 