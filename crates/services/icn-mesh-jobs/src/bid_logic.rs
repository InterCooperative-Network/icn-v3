use crate::types::{Bid, JobRequest};

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