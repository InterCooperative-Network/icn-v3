use anyhow::Result;
// Use the updated types directly from icn_types
use icn_types::jobs::{Bid, JobRequest}; // NodeMetadata is part of Bid
use crate::bid_logic; // Still need this for calculate_bid_selection_score

/// Trait for selecting the best executor (winning bid) for a given job request.
pub trait ExecutorSelector: Send + Sync {
    /// Given a job request and a list of bids, returns the winning bid and its score,
    /// or `None` if no bid is acceptable.
    fn select(&self, request: &JobRequest, bids: &[Bid]) -> Result<Option<(Bid, f64)>>;
}

/// Default executor selector, using reputation and price weights.
pub struct DefaultExecutorSelector {
    pub rep_weight: f64,
    pub price_weight: f64,
}

impl DefaultExecutorSelector {
    /// Create a selector with the provided weights.
    pub fn new(rep_weight: f64, price_weight: f64) -> Self {
        Self { rep_weight, price_weight }
    }
}

impl ExecutorSelector for DefaultExecutorSelector {
    fn select(&self, request: &JobRequest, bids: &[Bid]) -> Result<Option<(Bid, f64)>> {
        let mut best: Option<(Bid, f64)> = None;
        for bid in bids {
            let score = bid_logic::calculate_bid_selection_score(
                bid,
                request,
                self.rep_weight,
                self.price_weight,
            );
            if score > 0.0 { // Ensure score is positive
                if best.is_none() || score > best.as_ref().unwrap().1 {
                    best = Some((bid.clone(), score));
                }
            }
        }
        Ok(best)
    }
}

/// Defines the policy parameters for the GovernedExecutorSelector.
/// This might be sourced from job metadata, governance proposals, or runtime configuration.
#[derive(Debug, Clone)]
pub struct ExecutionPolicy {
    pub rep_weight: f64,
    pub price_weight: f64,
    pub region_filter: Option<String>,
    pub min_reputation: Option<f64>,
}

/// Governed executor selector, using an ExecutionPolicy.
pub struct GovernedExecutorSelector {
    pub policy: ExecutionPolicy,
}

impl GovernedExecutorSelector {
    pub fn new(policy: ExecutionPolicy) -> Self {
        Self { policy }
    }
}

impl ExecutorSelector for GovernedExecutorSelector {
    fn select(&self, request: &JobRequest, bids: &[Bid]) -> Result<Option<(Bid, f64)>> {
        let mut best_bid: Option<(Bid, f64)> = None;

        for bid in bids {
            // Filter based on policy using bid.node_metadata
            if let Some(ref required_region) = self.policy.region_filter {
                let bid_region = bid.node_metadata.as_ref().and_then(|meta| meta.region.as_ref());
                if bid_region.map(|s| s.as_str()) != Some(required_region.as_str()) {
                    tracing::debug!(bidder = %bid.bidder, job_id = %request.wasm_cid, "Bidder filtered out by region policy. Required: {:?}, Bidder has: {:?}", required_region, bid_region);
                    continue;
                }
            }

            if let Some(min_rep) = self.policy.min_reputation {
                let bid_reputation = bid.node_metadata.as_ref().and_then(|meta| meta.reputation);
                if bid_reputation.unwrap_or(0.0) < min_rep {
                     tracing::debug!(bidder = %bid.bidder, job_id = %request.wasm_cid, "Bidder filtered out by reputation policy. Min required: {}, Bidder has: {:?}", min_rep, bid_reputation);
                    continue;
                }
            }

            let score = bid_logic::calculate_bid_selection_score(
                bid,
                request,
                self.policy.rep_weight,
                self.policy.price_weight,
            );

            if score > 0.0 { // Ensure score is positive
                 if best_bid.is_none() || score > best_bid.as_ref().unwrap().1 {
                    best_bid = Some((bid.clone(), score));
                }
            }
        }
        Ok(best_bid)
    }
} 