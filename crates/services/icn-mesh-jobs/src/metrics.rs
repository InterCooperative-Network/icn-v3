use prometheus::{
    Histogram, HistogramOpts, IntCounterVec, IntCounter, IntGauge, 
    Opts, Registry, register
};
use once_cell::sync::Lazy;
use std::sync::Mutex;

// Registry holds all our metrics
static REGISTRY: Lazy<Mutex<Registry>> = Lazy::new(|| {
    Mutex::new(Registry::new())
});

// Metrics for bid evaluation
static BID_EVALUATION_COUNT: Lazy<IntCounterVec> = Lazy::new(|| {
    let bid_eval_count = IntCounterVec::new(
        Opts::new(
            "mesh_bid_evaluation_count",
            "Number of bid evaluations performed, by reason",
        ),
        &["reason"], // "high_reputation", "low_price", "resource_match", etc.
    ).expect("Failed to create bid_evaluation_count metric");
    
    register_metric(&bid_eval_count);
    bid_eval_count
});

static BID_SCORE_HISTOGRAM: Lazy<Histogram> = Lazy::new(|| {
    let bid_score_histogram = Histogram::with_opts(
        HistogramOpts::new(
            "mesh_bid_score_distribution",
            "Distribution of bid scores",
        )
        .buckets(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0])
    ).expect("Failed to create bid_score_histogram metric");
    
    register_metric(&bid_score_histogram);
    bid_score_histogram
});

static BID_SCORE_COMPONENT: Lazy<IntCounterVec> = Lazy::new(|| {
    let bid_score_component = IntCounterVec::new(
        Opts::new(
            "mesh_bid_evaluation_component_score",
            "Contribution of each component to winning bid scores",
        ),
        &["component", "node_id"], // component: "price", "reputation", "timeliness", "resources"
    ).expect("Failed to create bid_evaluation_component_score metric");
    
    register_metric(&bid_score_component);
    bid_score_component
});

static REPUTATION_QUERY_COUNT: Lazy<IntCounter> = Lazy::new(|| {
    let reputation_query_count = IntCounter::new(
        "mesh_reputation_query_count",
        "Number of reputation service queries made",
    ).expect("Failed to create reputation_query_count metric");
    
    register_metric(&reputation_query_count);
    reputation_query_count
});

static REPUTATION_CACHE_HITS: Lazy<IntCounter> = Lazy::new(|| {
    let reputation_cache_hits = IntCounter::new(
        "mesh_reputation_cache_hits",
        "Number of reputation profile cache hits",
    ).expect("Failed to create reputation_cache_hits metric");
    
    register_metric(&reputation_cache_hits);
    reputation_cache_hits
});

static REPUTATION_CACHE_MISSES: Lazy<IntCounter> = Lazy::new(|| {
    let reputation_cache_misses = IntCounter::new(
        "mesh_reputation_cache_misses",
        "Number of reputation profile cache misses",
    ).expect("Failed to create reputation_cache_misses metric");
    
    register_metric(&reputation_cache_misses);
    reputation_cache_misses
});

static REPUTATION_CACHE_SIZE: Lazy<IntGauge> = Lazy::new(|| {
    let reputation_cache_size = IntGauge::new(
        "mesh_reputation_cache_size",
        "Number of entries in the reputation profile cache",
    ).expect("Failed to create reputation_cache_size metric");
    
    register_metric(&reputation_cache_size);
    reputation_cache_size
});

/// Helper function to register a metric with the registry
fn register_metric<M: prometheus::core::Collector>(metric: &M) {
    let mut registry = REGISTRY.lock().unwrap();
    registry.register(Box::new(metric.clone())).expect("Failed to register metric");
}

/// Record a bid evaluation, tracking the reason for the final selection
pub fn record_bid_evaluation(reason: &str) {
    BID_EVALUATION_COUNT.with_label_values(&[reason]).inc();
}

/// Record a bid score value to track the distribution
pub fn record_bid_score(score: f64) {
    BID_SCORE_HISTOGRAM.observe(score);
}

/// Record a component's contribution to a winning bid
pub fn record_bid_component_score(component: &str, node_id: &str, score: f64) {
    // We store scores as integers (multiplied by 100) to preserve precision 
    // since IntCounterVec doesn't support floating point
    let score_int = (score * 100.0) as i64;
    BID_SCORE_COMPONENT.with_label_values(&[component, node_id]).inc_by(score_int as u64);
}

/// Record a reputation service query
pub fn record_reputation_query() {
    REPUTATION_QUERY_COUNT.inc();
}

/// Record a reputation cache hit
pub fn record_reputation_cache_hit() {
    REPUTATION_CACHE_HITS.inc();
}

/// Record a reputation cache miss
pub fn record_reputation_cache_miss() {
    REPUTATION_CACHE_MISSES.inc();
}

/// Update the reputation cache size
pub fn update_reputation_cache_size(size: usize) {
    REPUTATION_CACHE_SIZE.set(size as i64);
}

/// Get the registry of all metrics
pub fn get_registry() -> Registry {
    REGISTRY.lock().unwrap().clone()
} 