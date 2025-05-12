use anyhow::Result;
use crate::types::{Bid, JobRequest};
use crate::models::BidEvaluatorConfig;
use crate::reputation_client::ReputationClient;

// Constants for scoring
const DEFAULT_REPUTATION_SCORE_NORMALIZED: f64 = 0.5; // Default normalized reputation (0-1 scale)
const MAX_PRICE_FOR_NORMALIZATION: f64 = 1_000_000.0; // Arbitrary cap for price normalization, adjust as needed

// Default weights, can be made configurable later
pub const DEFAULT_REP_WEIGHT: f64 = 0.7;
pub const DEFAULT_PRICE_WEIGHT: f64 = 0.3;

/**
 * Calculates a selection score for a given bid based on reputation and price.
 *
 * - Bids that don't meet job requirements are disqualified (score = -1.0).
 * - Reputation is normalized (assuming input `bid.reputation_score` is 0-10, normalized to 0-1).
 * - Price is normalized (higher price leads to a score penalty).
 * - Final score combines weighted reputation and price.
 */
pub fn calculate_bid_selection_score(
    bid: &Bid,
    job_req: &JobRequest, // Used to check if bid meets resource requirements
    rep_weight: f64,
    price_weight: f64,
) -> f64 {
    // 1. Basic check if bid meets resource requirements
    // This is a simple check; more complex matching could exist.
    if bid.resources.cpu < job_req.requirements.cpu ||
       bid.resources.memory_mb < job_req.requirements.memory_mb ||
       bid.resources.storage_mb < job_req.requirements.storage_mb {
        tracing::debug!(
            "Bidder {} for job {} disqualified due to unmet resource requirements. Estimate: {:?}, Required: {:?}",
            bid.bidder_did, bid.job_id, bid.resources, job_req.requirements
        );
        return -1.0; // Disqualify bid that doesn't meet core requirements
    }

    // 2. Normalize Reputation Score
    // Assumes bid.reputation_score is on a 0-10 scale from icn-reputation::compute_score
    // If None, use a default neutral value.
    let normalized_reputation = bid.reputation_score
        .map(|r_score| (r_score / 10.0).clamp(0.0, 1.0)) // Normalize 0-10 to 0-1 and clamp
        .unwrap_or(DEFAULT_REPUTATION_SCORE_NORMALIZED);

    // 3. Normalize Price Factor (0 means free, 1 means at or above MAX_PRICE_FOR_NORMALIZATION)
    // Higher factor means more expensive, leading to a score penalty.
    let normalized_price_factor = (bid.price as f64 / MAX_PRICE_FOR_NORMALIZATION).min(1.0);

    // 4. Calculate final score
    // Higher reputation increases score, higher price decreases score.
    let score = (normalized_reputation * rep_weight) - (normalized_price_factor * price_weight);
    
    tracing::debug!(
        "Bidder {} for job {}: norm_rep={}, norm_price_factor={}, rep_w={}, price_w={}, final_score={}",
        bid.bidder_did, bid.job_id, normalized_reputation, normalized_price_factor, rep_weight, price_weight, score
    );

    score
}

pub fn validate_bid(bid: &Bid, job_req: &JobRequest) -> Result<()> {
    // Check if bid meets minimum resource requirements
    if bid.resources.cpu_cores < job_req.requirements.cpu_cores ||
       bid.resources.memory_mb < job_req.requirements.memory_mb ||
       bid.resources.storage_gb < job_req.requirements.storage_gb {
        return Err(anyhow::anyhow!("Bid does not meet minimum resource requirements"));
    }

    // Check if bid price is within acceptable range
    if bid.price > job_req.requirements.max_price {
        return Err(anyhow::anyhow!("Bid price exceeds maximum allowed price"));
    }

    Ok(())
}

pub fn calculate_bid_score(
    config: &BidEvaluatorConfig,
    bid: &Bid,
    reputation_score: f64,
) -> f64 {
    // Normalize the price component (lower is better)
    let normalized_price = 1.0 - (bid.price as f64 / bid.resources.max_price as f64);

    // Calculate resource utilization score
    let resource_score = calculate_resource_score(&bid.resources);

    // Combine components with weights
    let price_component = normalized_price * config.price_weight;
    let resource_component = resource_score * config.resource_match_weight;
    let reputation_component = reputation_score * config.reputation_weight;

    price_component + resource_component + reputation_component
}

fn calculate_resource_score(resources: &JobRequirements) -> f64 {
    // Calculate how well the resources match the requirements
    let cpu_utilization = resources.cpu_cores as f64 / resources.max_cpu_cores as f64;
    let memory_utilization = resources.memory_mb as f64 / resources.max_memory_mb as f64;
    let storage_utilization = resources.storage_gb as f64 / resources.max_storage_gb as f64;

    // Average the utilization scores
    (cpu_utilization + memory_utilization + storage_utilization) / 3.0
} 