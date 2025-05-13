use prometheus::{
    IntCounterVec, IntGauge, register_int_counter_vec, register_int_gauge,
};
use lazy_static::lazy_static;
use crate::mana::RegenerationPolicy;

lazy_static! {
    // Ticks
    pub static ref MANA_REGENERATION_TICKS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "mana_regeneration_ticks_total",
        "Total number of regeneration ticks",
        &["policy_type"]
    ).unwrap();

    pub static ref MANA_REGENERATED_DIDS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "mana_regenerated_dids_total",
        "Total number of DIDs with increased mana per tick",
        &["policy_type"]
    ).unwrap();

    pub static ref MANA_PROCESSED_DIDS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "mana_processed_dids_total",
        "Total number of DIDs considered for regeneration per tick",
        &["policy_type"]
    ).unwrap();

    pub static ref MANA_REGENERATION_ERRORS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "mana_regeneration_errors_total",
        "Total number of errors encountered during regeneration ticks",
        &["policy_type", "error_scope"]
    ).unwrap();

    // Active state
    pub static ref MANA_ACTIVE_DIDS_GAUGE: IntGauge = register_int_gauge!(
        "mana_active_dids",
        "Number of DIDs currently tracked by the mana ledger"
    ).unwrap();

    // Ledger operations
    pub static ref MANA_LEDGER_OPERATIONS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "mana_ledger_operations_total",
        "Total number of operations against the mana ledger",
        &["ledger_type", "operation", "status"]
    ).unwrap();

    pub static ref MANA_LEDGER_ERRORS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "mana_ledger_errors_total",
        "Total number of errors during mana ledger operations",
        &["ledger_type", "operation", "error_type"]
    ).unwrap();
}

/// Helper to extract label string from a policy
pub fn policy_to_label(policy: &RegenerationPolicy) -> &'static str {
    match policy {
        RegenerationPolicy::FixedRatePerTick(_) => "fixed_rate_per_tick",
    }
} 