use anyhow::Result;
use async_trait::async_trait;
use cid::Cid;
use icn_identity::Did;
use crate::types::{Bid, JobRequest, JobRequirements};
use std::sync::Arc;
use crate::bid_logic;
use crate::models::BidEvaluatorConfig;
use crate::reputation_client::{ReputationClient, ReputationProfile};
use crate::metrics;
use tracing;
use icn_types::reputation::ReputationProfile as ICNReputationProfile;

/// Defines the selection strategy to use for assigning jobs to executors
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionStrategy {
    /// Select the bid with the lowest price
    LowestPrice,
    /// Select the bid based on reputation score
    Reputation,
    /// Select the bid using a hybrid approach
    Hybrid,
}

/// Defines the policy parameters for the GovernedExecutorSelector.
/// This might be sourced from job metadata, governance proposals, or runtime configuration.
#[derive(Debug, Clone)]
pub struct ExecutionPolicy {
    pub rep_weight: f64,
    pub price_weight: f64,
    pub region_filter: Option<String>,
    pub min_reputation: Option<f64>,
    pub selection_strategy: SelectionStrategy,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self {
            rep_weight: 0.7,
            price_weight: 0.3,
            region_filter: None,
            min_reputation: None,
            selection_strategy: SelectionStrategy::Reputation,
        }
    }
}

/// Trait for selecting the best executor (winning bid) for a given job request.
#[async_trait]
pub trait ExecutorSelector: Send + Sync {
    /// Given a job request and a list of bids, returns the winning bid, its score, and the reason,
    /// or `None` if no bid is acceptable.
    async fn select(&self, request: &JobRequest, bids: &[Bid], job_id: Cid) -> Result<Option<(Bid, f64, String)>>;
}

/// Selector that chooses the executor with the lowest price.
pub struct LowestPriceExecutorSelector {}

#[async_trait]
impl ExecutorSelector for LowestPriceExecutorSelector {
    async fn select(&self, _request: &JobRequest, bids: &[Bid], _job_id: Cid) -> Result<Option<(Bid, f64, String)>> {
        if bids.is_empty() {
            return Ok(None);
        }
        
        // Find the bid with the lowest price
        let mut best_bid = &bids[0];
        let mut lowest_price = best_bid.price;
        
        for bid in bids {
            if bid.price < lowest_price {
                best_bid = bid;
                lowest_price = bid.price;
            }
        }
        
        let reason = format!("lowest_price_{}", lowest_price);
        metrics::record_bid_evaluation(&reason);
        
        Ok(Some((best_bid.clone(), 1.0, reason)))
    }
}

/// Selector that uses reputation scores to choose the best executor.
pub struct ReputationExecutorSelector {
    pub config: BidEvaluatorConfig,
    pub reputation_client: Arc<dyn ReputationClient>,
}

#[async_trait]
impl ExecutorSelector for ReputationExecutorSelector {
    async fn select(&self, request: &JobRequest, bids: &[Bid], _job_id: Cid) -> Result<Option<(Bid, f64, String)>> {
        if bids.is_empty() {
            return Ok(None);
        }
        
        let max_price = bids.iter().map(|b| b.price).max().unwrap_or(1);
        let mut best: Option<(Bid, f64, String)> = None;

        for bid in bids {
            // Fetch reputation profile
            let profile = match self.reputation_client.fetch_profile(&bid.bidder_did).await {
                Ok(Some(p)) => p,
                Ok(None) => {
                    tracing::debug!("No reputation profile found for bidder {}. Constructing default profile.", bid.bidder_did);
                    // Construct a default profile if none is found.
                    // Ensure mana_state is None so it fails checks if mana is required.
                    ICNReputationProfile {
                        node_id: bid.bidder_did.clone(),
                        mana_state: None, // Explicitly None for default profile
                        // Initialize other fields to sensible defaults
                        last_updated: chrono::Utc::now(), // Placeholder, might need specific default
                        total_jobs: 0,
                        successful_jobs: 0,
                        failed_jobs: 0,
                        jobs_on_time: 0,
                        jobs_late: 0,
                        average_execution_ms: None,
                        average_bid_accuracy: None,
                        dishonesty_events: 0,
                        endorsements: vec![],
                        current_stake: None,
                        computed_score: 50.0, // Neutral score, adjust as per existing defaults
                        latest_anchor_cid: None,
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch reputation for {}: {}. Skipping bid.", bid.bidder_did, e);
                    continue; // Skip bid if profile fetch fails
                }
            };

            // Mana Check
            if let Some(required_mana_amount) = request.requirements.required_mana {
                let has_sufficient_mana = profile.mana_state.as_ref().map_or(false, |mana_details| {
                    mana_details.state.current_mana >= required_mana_amount
                });

                if !has_sufficient_mana {
                    tracing::info!(
                        "Bidder {} for job {} disqualified due to insufficient mana. Required: {}, Available: {:?}.",
                        bid.bidder_did,
                        bid.job_id,
                        required_mana_amount,
                        profile.mana_state.as_ref().map(|ms| ms.state.current_mana)
                    );
                    metrics::increment_bids_disqualified_insufficient_mana();
                    continue; // Disqualify bid
                }
            }

            // Calculate normalized price (0-1 where 0 is best)
            let normalized_price = if max_price > 0 { bid.price as f64 / max_price as f64 } else { 0.0 };
            
            // Calculate resource match (0-1 where 1 is best)
            let resource_match = self.calculate_resource_match(&bid.resources, &request.requirements);
            
            // Calculate score using the client's logic
            let score = self.reputation_client.calculate_bid_score(
                &self.config,
                &profile,
                normalized_price,
                resource_match
            );
            
            if score < 0.0 {
                tracing::debug!(
                    "Bidder {} for job {} has a negative score ({}) and is disqualified.",
                    bid.bidder_did, bid.job_id, score
                );
                continue;
            }

            if best.is_none() || score > best.as_ref().unwrap().1 {
                let reason = if profile.computed_score > 75.0 {
                    format!("high_reputation_{}", bid.bidder_did)
                } else if normalized_price < 0.3 {
                    format!("low_price_{}", bid.bidder_did)
                } else if resource_match > 0.8 {
                    format!("good_resource_match_{}", bid.bidder_did)
                } else {
                    format!("balanced_score_{}", bid.bidder_did)
                };
                best = Some((bid.clone(), score, reason));
            }
        }
        
        Ok(best)
    }
}

impl ReputationExecutorSelector {
    fn calculate_resource_match(
        &self, 
        bid_resources: &JobRequirements, // Changed from &icn_types::jobs::ResourceEstimate to local JobRequirements
        job_requirements: &JobRequirements  // Changed from &icn_types::jobs::ResourceRequirements to local JobRequirements
    ) -> f64 {
        // Implementation using fields from local JobRequirements
        // (cpu_cores, memory_mb, storage_gb)
        // Ensure these fields exist and are comparable.
        // The fields in JobRequirements are: cpu_cores, memory_mb, storage_gb.
        // The original icn_types::jobs::ResourceEstimate had: cpu, memory_mb, storage_mb.
        // Assuming a direct mapping for now. If fields differ significantly, logic needs adjustment.

        let cpu_match = if bid_resources.cpu_cores >= job_requirements.cpu_cores {
            1.0
        } else if job_requirements.cpu_cores == 0 { // Avoid division by zero if requirement is 0
            1.0 // or 0.0, depending on desired behavior for 0 requirement
        } else {
            bid_resources.cpu_cores as f64 / job_requirements.cpu_cores as f64
        };
        
        let memory_match = if bid_resources.memory_mb >= job_requirements.memory_mb {
            1.0
        } else if job_requirements.memory_mb == 0 {
            1.0
        } else {
            bid_resources.memory_mb as f64 / job_requirements.memory_mb as f64
        };
        
        // Assuming storage_gb on both. Original had storage_mb for estimate.
        // If JobRequirements has storage_gb for both, this is fine.
        let storage_match = if bid_resources.storage_gb >= job_requirements.storage_gb {
            1.0
        } else if job_requirements.storage_gb == 0 {
            1.0
        } else {
            bid_resources.storage_gb as f64 / job_requirements.storage_gb as f64
        };
        
        // Average the match scores, ensure it's clamped 0.0 to 1.0
        ((cpu_match + memory_match + storage_match) / 3.0).clamp(0.0, 1.0)
    }
}

/// Hybrid selector that combines aspects of ExecutionPolicy with reputation-based scoring.
pub struct HybridExecutorSelector {
    pub policy: ExecutionPolicy,
    pub reputation_client: Arc<dyn ReputationClient>,
}

#[async_trait]
impl ExecutorSelector for HybridExecutorSelector {
    async fn select(&self, request: &JobRequest, bids: &[Bid], job_id: Cid) -> Result<Option<(Bid, f64, String)>> {
        if bids.is_empty() {
            return Ok(None);
        }
        
        // Filter by minimum reputation if specified
        let filtered_bids = if let Some(min_rep) = self.policy.min_reputation {
            let mut valid_bids = Vec::new();
            
            for bid in bids {
                let profile = match self.reputation_client.fetch_profile(&bid.bidder_did).await {
                    Ok(profile) => profile,
                    Err(_) => continue, // Skip bids where we can't fetch reputation
                };
                
                // Check if reputation meets minimum
                if profile.computed_score >= min_rep {
                    valid_bids.push(bid.clone());
                }
            }
            
            valid_bids
        } else {
            bids.to_vec()
        };
        
        if filtered_bids.is_empty() {
            return Ok(None);
        }
        
        // Create a config from the policy
        let config = BidEvaluatorConfig {
            weight_price: self.policy.price_weight,
            weight_reputation: self.policy.rep_weight,
            weight_resources: 0.1, // Default
            weight_timeliness: 0.1, // Default
        };
        
        // Use the ReputationExecutorSelector for actual scoring
        let reputation_selector = ReputationExecutorSelector {
            config,
            reputation_client: self.reputation_client.clone(),
        };
        
        reputation_selector.select(request, &filtered_bids, job_id).await
    }
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

#[async_trait]
impl ExecutorSelector for DefaultExecutorSelector {
    async fn select(&self, request: &JobRequest, bids: &[Bid], _job_id: Cid) -> Result<Option<(Bid, f64, String)>> {
        let mut best: Option<(Bid, f64, String)> = None;
        for bid in bids {
            let score = bid_logic::calculate_bid_selection_score(
                bid,
                request,
                self.rep_weight,
                self.price_weight,
            );
            if score > 0.0 { // Ensure score is positive
                if best.is_none() || score > best.as_ref().unwrap().1 {
                    best = Some((bid.clone(), score, "default_scoring".to_string()));
                }
            }
        }
        Ok(best)
    }
}

/// Legacy selector, to be phased out
pub struct GovernedExecutorSelector {
    pub policy: ExecutionPolicy,
}

impl GovernedExecutorSelector {
    /// Create a new governed selector with the given policy.
    pub fn new(policy: ExecutionPolicy) -> Self {
        Self { policy }
    }
}

pub struct JobAssignmentService {
    reputation_client: Arc<dyn ReputationClient>,
    config: BidEvaluatorConfig,
}

impl JobAssignmentService {
    pub fn new(reputation_client: Arc<dyn ReputationClient>, config: BidEvaluatorConfig) -> Self {
        Self {
            reputation_client,
            config,
        }
    }

    pub async fn evaluate_bids(&self, request: &JobRequest, bids: &[Bid]) -> Result<Vec<(Bid, f64)>> {
        let mut scored_bids = Vec::new();

        for bid in bids {
            let resource_match = self.calculate_resource_match(&bid.resources, &request.requirements);
            let normalized_price = self.normalize_price(bid.price, request.requirements.max_price);

            let profile = match self.reputation_client.fetch_profile(&bid.bidder_did).await {
                Ok(Some(profile)) => profile,
                Ok(None) => {
                    tracing::warn!("No reputation profile found for {}", bid.bidder_did.to_string());
                    continue;
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch reputation for {}: {}", bid.bidder_did.to_string(), e);
                    continue;
                }
            };

            let score = self.calculate_bid_score(
                &self.config,
                &profile,
                normalized_price,
                resource_match,
            );

            scored_bids.push((bid.clone(), score));
        }

        // Sort by score in descending order
        scored_bids.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored_bids)
    }

    fn calculate_resource_match(&self, estimate: &JobRequirements, requirements: &JobRequirements) -> f64 {
        let cpu_match = (estimate.cpu_cores as f64 / requirements.cpu_cores as f64).min(1.0);
        let memory_match = (estimate.memory_mb as f64 / requirements.memory_mb as f64).min(1.0);
        let storage_match = (estimate.storage_gb as f64 / requirements.storage_gb as f64).min(1.0);

        (cpu_match + memory_match + storage_match) / 3.0
    }

    fn normalize_price(&self, bid_price: u64, max_price: u64) -> f64 {
        if max_price == 0 {
            return 0.0;
        }
        1.0 - (bid_price as f64 / max_price as f64)
    }

    fn calculate_bid_score(
        &self,
        config: &BidEvaluatorConfig,
        profile: &ICNReputationProfile,
        normalized_price: f64,
        resource_match: f64,
    ) -> f64 {
        let reputation_component = profile.computed_score * config.reputation_weight;
        let price_component = normalized_price * config.price_weight;
        let resource_component = resource_match * config.resource_match_weight;

        // Log the scoring components for debugging
        tracing::debug!(
            "Bid score components for {}: reputation={}, price={}, resources={}",
            profile.node_id,
            reputation_component,
            price_component,
            resource_component
        );

        reputation_component + price_component + resource_component
    }
}

#[cfg(test)]
mod job_assignment_tests; 