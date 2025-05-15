use anyhow::Result as AnyhowResult;
use async_trait::async_trait;
use cid::Cid;
use icn_identity::Did;
use crate::types::{Bid, JobRequest, JobRequirements};
use std::sync::Arc;
use crate::bid_logic;
use crate::models::BidEvaluatorConfig;
use crate::reputation_client::{ReputationClient, ReputationClientError, ReputationProfile};
use crate::metrics;
use tracing;
use icn_types::reputation::ReputationProfile as ICNReputationProfile;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SelectionError {
    #[error("Internal error during executor selection: {reason}")]
    Internal { reason: String },
}

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
    async fn select(&self, request: &JobRequest, bids: &[Bid], job_id: Cid) -> Result<Option<(Bid, f64, String)>, SelectionError>;
}

/// Selector that chooses the executor with the lowest price.
pub struct LowestPriceExecutorSelector {}

#[async_trait]
impl ExecutorSelector for LowestPriceExecutorSelector {
    async fn select(&self, _request: &JobRequest, bids: &[Bid], _job_id: Cid) -> Result<Option<(Bid, f64, String)>, SelectionError> {
        if bids.is_empty() {
            return Ok(None);
        }
        
        // Find the bid with the lowest price
        let mut best_bid = &bids[0];
        let mut lowest_price = best_bid.price_atto_icn;
        
        for bid_item in bids.iter().skip(1) {
            if bid_item.price_atto_icn < lowest_price {
                best_bid = bid_item;
                lowest_price = bid_item.price_atto_icn;
            }
        }
        
        let reason = format!("lowest_price_{}", lowest_price);
        
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
    async fn select(&self, request: &JobRequest, bids: &[Bid], _job_id: Cid) -> Result<Option<(Bid, f64, String)>, SelectionError> {
        if bids.is_empty() {
            return Ok(None);
        }
        
        // Determine max_price safely
        let max_price = bids.iter().map(|b| b.price_atto_icn).max().unwrap_or(1); // if bids is empty, unwrap_or(1) is fine.
                                                                              // if not empty, max() returns Some, so unwrap is fine.

        let mut best: Option<(Bid, f64, String)> = None;

        for bid_item in bids {
            // Fetch reputation profile
            let profile = match self.reputation_client.fetch_profile(&bid_item.bidder).await { // Corrected: bid_item.bidder
                Ok(Some(p)) => p,
                Ok(None) => {
                    tracing::debug!("No reputation profile found for bidder {}. Constructing default profile.", bid_item.bidder);
                    ICNReputationProfile {
                        node_id: bid_item.bidder.clone(),
                        mana_state: None, 
                        last_updated: chrono::Utc::now(),
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
                        computed_score: 50.0, 
                        latest_anchor_cid: None,
                    }
                }
                Err(e) => {
                    // This is ReputationClientError. Log and skip bid.
                    // Does not cause SelectionError unless all bids are skipped this way leading to Ok(None).
                    tracing::warn!("Failed to fetch reputation for {}: {}. Skipping bid.", bid_item.bidder, e);
                    continue; 
                }
            };

            // Mana Check (assuming JobRequest.params.required_mana)
            if let Some(required_mana_amount) = request.params.required_mana { // Changed from request.requirements
                let has_sufficient_mana = profile.mana_state.as_ref().map_or(false, |mana_details| {
                    mana_details.current_mana >= required_mana_amount // Assuming mana_state has current_mana directly
                });

                if !has_sufficient_mana {
                    tracing::info!(
                        "Bidder {} for job {} disqualified due to insufficient mana. Required: {}, Available: {:?}.",
                        bid_item.bidder,
                        bid_item.job_id, // Assuming Bid has job_id
                        required_mana_amount,
                        profile.mana_state.as_ref().map(|ms| ms.current_mana)
                    );
                    // metrics call removed for brevity, needs to be handled if metrics can fail
                    continue; 
                }
            }

            let normalized_price = if max_price > 0 { bid_item.price_atto_icn as f64 / max_price as f64 } else { 0.0 };
            
            // Assuming bid_item.data is of type ResourceEstimate matching calculate_resource_match
            // and request.params has the requirements.
            let resource_match = self.calculate_resource_match(&bid_item.data, &request.params.requirements_v1); // Changed to params.requirements_v1 based on typical structure
            
            let score = self.reputation_client.calculate_bid_score(
                &self.config,
                &profile,
                normalized_price,
                resource_match
            );
            
            if score < 0.0 {
                tracing::debug!(
                    "Bidder {} for job {} has a negative score ({}) and is disqualified.",
                    bid_item.bidder, bid_item.job_id, score
                );
                continue;
            }

            if best.is_none() || score > best.as_ref().unwrap().1 {
                let reason = format!("selected_by_reputation_score_{}", score);
                best = Some((bid_item.clone(), score, reason));
            }
        }
        
        Ok(best)
    }
}

impl ReputationExecutorSelector {
    // Assuming bid_resources is ResourceEstimate, job_requirements is ResourceRequirementsV1
    fn calculate_resource_match(
        &self, 
        bid_resources: &icn_types::jobs::ResourceEstimate, 
        job_requirements: &icn_types::jobs::ResourceRequirementsV1
    ) -> f64 {
        let cpu_match = if bid_resources.cpu >= job_requirements.cpu_cores {
            1.0
        } else if job_requirements.cpu_cores == 0 {
            1.0 
        } else {
            bid_resources.cpu as f64 / job_requirements.cpu_cores as f64
        };
        
        let memory_match = if bid_resources.memory_mb >= job_requirements.memory_mb {
            1.0
        } else if job_requirements.memory_mb == 0 {
            1.0
        } else {
            bid_resources.memory_mb as f64 / job_requirements.memory_mb as f64
        };
        
        let storage_match = if bid_resources.storage_mb / 1024 >= job_requirements.storage_gb {
            1.0
        } else if job_requirements.storage_gb == 0 {
            1.0
        } else {
            (bid_resources.storage_mb / 1024) as f64 / job_requirements.storage_gb as f64
        };
        
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
    async fn select(&self, request: &JobRequest, bids: &[Bid], job_id: Cid) -> Result<Option<(Bid, f64, String)>, SelectionError> {
        if bids.is_empty() {
            return Ok(None);
        }

        // 1. Filter by min_reputation if specified in policy
        let mut filtered_bids: Vec<Bid> = Vec::new();
        if let Some(min_rep) = self.policy.min_reputation {
            for bid_item in bids {
                match self.reputation_client.fetch_profile(&bid_item.bidder).await {
                    Ok(Some(profile)) => {
                        if profile.computed_score >= min_rep {
                            filtered_bids.push(bid_item.clone());
                        } else {
                            tracing::debug!("Bidder {} filtered out by min_reputation ({} < {})", bid_item.bidder, profile.computed_score, min_rep);
                        }
                    }
                    Ok(None) => {
                        tracing::debug!("Bidder {} has no reputation profile, filtered out by min_reputation policy ({})", bid_item.bidder, min_rep);
                    }
                    Err(e) => {
                        // If fetching reputation for a bid fails, this specific bid is skipped for min_reputation filter.
                        // It could alternatively cause a SelectionError::ReputationServiceFailure(e) if critical.
                        tracing::warn!("Failed to fetch reputation for {} during min_reputation filter: {}. Skipping bid for this filter.", bid_item.bidder, e);
                    }
                }
            }
            if filtered_bids.is_empty() && !bids.is_empty() {
                tracing::warn!("All bids filtered out by min_reputation policy. No bids left for selection.");
                return Ok(None); // No bids meet min_reputation
            }
        } else {
            filtered_bids = bids.to_vec(); // No min_reputation filter, use all bids
        }

        if filtered_bids.is_empty() { // Check again if filtering resulted in empty list
            return Ok(None);
        }

        // 2. Apply selection strategy on filtered bids
        let selector_to_use: Box<dyn ExecutorSelector> = match self.policy.selection_strategy {
            SelectionStrategy::LowestPrice => Box::new(LowestPriceExecutorSelector {}),
            SelectionStrategy::Reputation | SelectionStrategy::Hybrid => {
                 // For Hybrid, the reputation selector part uses the policy's weights
                Box::new(ReputationExecutorSelector {
                    config: BidEvaluatorConfig { // Use weights from policy
                        weight_price: self.policy.price_weight, 
                        weight_reputation: self.policy.rep_weight,
                        // Assume other weights are 0 or derived if not in ExecutionPolicy
                        weight_resources: (1.0 - self.policy.price_weight - self.policy.rep_weight).max(0.0) / 2.0, // Example derivation
                        weight_timeliness: (1.0 - self.policy.price_weight - self.policy.rep_weight).max(0.0) / 2.0, // Example derivation
                    },
                    reputation_client: self.reputation_client.clone(),
                })
            }
        };

        // The select call now returns Result<..., SelectionError>, propagate with ?
        selector_to_use.select(request, &filtered_bids, job_id).await?
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
    async fn select(&self, request: &JobRequest, bids: &[Bid], _job_id: Cid) -> Result<Option<(Bid, f64, String)>, SelectionError> {
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