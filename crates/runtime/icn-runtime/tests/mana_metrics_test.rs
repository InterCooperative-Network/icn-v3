use icn_economics::mana::{ManaManager, ManaPool}; // Adjusted import for ManaManager
use icn_economics::ScopeKey; // Added ScopeKey import
use icn_runtime::metrics::PrometheusManaMetrics;
use prometheus::{Encoder, TextEncoder, gather};
use std::sync::Arc;

#[test]
fn test_mana_metrics_balance_update() {
    // Assume PrometheusManaMetrics::new() registers globally or uses a default registry
    let metrics_hook = PrometheusManaMetrics::new(); 
    // Pass the Arc<PrometheusManaMetrics> which implements ManaMetricsHook
    let mut manager = ManaManager::with_metrics_hook(metrics_hook.clone()); // Pass the hook Arc

    let coop_id = "coopX".to_string();
    let indiv_did = "did:example:actor1".to_string();

    let coop = ScopeKey::Cooperative(coop_id.clone());
    let indiv = ScopeKey::Individual(indiv_did.clone());

    // Provide explicit max and regen values instead of ManaPoolConfig
    let initial_max_mana = 1000;
    let regen_per_sec = 1;
    manager.ensure_pool(&coop, initial_max_mana, regen_per_sec);
    manager.ensure_pool(&indiv, initial_max_mana, regen_per_sec);

    // Credit coop (initial balance is 1000, credit 500 -> still 1000 due to max)
    // Note: pool.credit now takes hook, but manager.credit should handle it internally
    // The ManaManager::credit method doesn't exist; need to use pool_mut and pool.credit directly or manager.transfer
    // Let's use transfer from a dummy source to credit
    let dummy_source = ScopeKey::Individual("dummy_source".to_string());
    manager.ensure_pool(&dummy_source, 1000, 1);
    manager.transfer(&dummy_source, &coop, 500).expect("Transfer to coop failed"); 
    // Coop balance should be max (1000), dummy source is 500

    // Spend from indiv (initial balance 1000, spend 250 -> 750)
    manager.spend(&indiv, 250).expect("Spend from indiv failed"); // Use expect for Result

    // Gather metrics
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();
    // gather() collects from the default registry
    let metric_families = gather(); 
    encoder.encode(&metric_families, &mut buffer).expect("Failed to encode metrics");
    let output = String::from_utf8(buffer).expect("Metrics output not valid UTF-8");

    println!("Metrics Output:\n{}", output);

    // Assertions - Check for the presence of the metric lines
    // Note: Prometheus output format can vary slightly (label order)
    let coop_expected_balance = initial_max_mana; // Crediting 500 shouldn't exceed max
    let indiv_expected_balance = initial_max_mana - 250;

    let coop_pattern_1 = format!("icn_mana_pool_balance{{scope_id=\"{}\",scope_type=\"cooperative\"}} {}", coop_id, coop_expected_balance as f64);
    let coop_pattern_2 = format!("icn_mana_pool_balance{{scope_type=\"cooperative\",scope_id=\"{}\"}} {}", coop_id, coop_expected_balance as f64);
    assert!(output.contains(&coop_pattern_1) || output.contains(&coop_pattern_2),
            "Coop balance metric not found or incorrect. Expected: {}", coop_expected_balance);

    let indiv_pattern_1 = format!("icn_mana_pool_balance{{scope_id=\"{}\",scope_type=\"individual\"}} {}", indiv_did, indiv_expected_balance as f64);
    let indiv_pattern_2 = format!("icn_mana_pool_balance{{scope_type=\"individual\",scope_id=\"{}\"}} {}", indiv_did, indiv_expected_balance as f64);
    assert!(output.contains(&indiv_pattern_1) || output.contains(&indiv_pattern_2),
            "Individual balance metric not found or incorrect. Expected: {}", indiv_expected_balance);
} 