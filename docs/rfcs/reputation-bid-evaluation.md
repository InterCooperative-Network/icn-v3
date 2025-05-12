# RFC: Reputation-Driven Bid Evaluation for Mesh Compute

## Summary

This RFC proposes a comprehensive integration of node reputation into the bid evaluation process for mesh compute job allocation. This integration creates a direct economic incentive for maintaining high reputation, strengthening the connection between reputation and economic outcomes in the ICN ecosystem.

## Motivation

Currently, the bid selection process primarily considers price, with no formal mechanism for incorporating node reputation. This creates several issues:

1. **Reliability vs. Cost Tradeoff**: Originators have no way to balance reliability with cost considerations when selecting executors.
2. **Missing Economic Incentive**: Nodes with proven track records receive no direct economic benefit from their reputation.
3. **No Feedback Loop**: The system does not use past behavior to influence future job allocation.

By integrating reputation directly into the bid evaluation process, we create a system that:

- Rewards reliable execution with increased job allocation
- Creates market-driven incentives for maintaining high reputation
- Allows tuning the reputation influence based on job criticality
- Provides a balanced approach that still respects resource pricing

## Detailed Design

### 1. Reputation Client Interface

Create a `ReputationClient` trait and implementation in `planetary-mesh/src/reputation_integration.rs`:

```rust
#[async_trait]
pub trait ReputationClient {
    async fn fetch_profile(&self, did: &str) -> Result<ReputationProfile>;
    fn verify_reported_score(&self, profile: &ReputationProfile, reported: u32) -> bool;
    fn calculate_bid_score(&self, config: &BidEvaluatorConfig, profile: &ReputationProfile, 
                          normalized_price: f64, resource_match: f64) -> f64;
}
```

### 2. Bid Evaluator Configuration

Define a configuration struct with the weights for each component:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BidEvaluatorConfig {
    pub weight_price: f64,
    pub weight_resources: f64,
    pub weight_reputation: f64,
    pub weight_timeliness: f64,
    pub reputation_api_endpoint: String,
    pub reputation_api_timeout_secs: u64,
    pub score_verification_tolerance: f64,
}
```

### 3. Bid Scoring Formula

Bids are scored using a weighted formula:

```
total_score = 
  w_price·(1 - norm_price) +
  w_resources·resource_match +
  w_reputation·(reputation_score/100) +
  w_timeliness·(on_time_jobs/total_successful_jobs)
```

Where:
- Each component is normalized to a 0-1 range
- Weights are configurable
- Reputation score is taken from the node's profile
- Timeliness is calculated as the ratio of on-time jobs to total successful jobs

### 4. CCL Policy Integration

Define a CCL contract type for configuring the weights:

```
policy_def ReputationBidWeights {
    price_weight: 0.4,
    resources_weight: 0.2,
    reputation_weight: 0.3,
    timeliness_weight: 0.1,
    min_reputation_for_critical_jobs: 70,
    require_verified_reputation: true,
    reputation_score_verification_tolerance: 0.05
}
```

### 5. Bid Selection Process

Modify the bid selection process in `MeshNode` to:
1. Fetch reputation profiles for all bidders
2. Calculate a score for each bid using the weighted formula
3. Select the bid with the highest score
4. Log detailed scoring information for transparency

## Implementation Steps

1. ✅ Create the `reputation_integration.rs` module with the `ReputationClient` trait and implementation
2. ✅ Define the `BidEvaluatorConfig` struct 
3. ✅ Implement the bid scoring formula
4. ✅ Update the bid selection logic in `MeshNode`
5. ✅ Develop CCL contract examples for reputation weights
6. 🔲 Add reputation to job-specific requirements
7. 🔲 Implement resource matching logic

## Backward Compatibility

This change is backward compatible as it enhances the existing bid selection process without changing any public APIs. Default weight values ensure that price remains a significant factor in bid selection.

## Testing Considerations

1. **Unit Tests**:
   - Test bid scoring with different reputation profiles
   - Test score normalization edge cases
   - Verify verification tolerance logic

2. **Integration Tests**:
   - Test end-to-end bid selection with mock reputation profiles
   - Test handling of reputation service failures
   - Test different weight configurations

## Alternatives Considered

1. **Binary Reputation Threshold**: Simply filtering out nodes below a certain reputation score was considered but rejected as it doesn't provide a smooth gradient of incentives.

2. **Originator-Defined Weights**: Allowing job originators to define their own weights was considered, but this adds complexity to the job specification. This could be a future enhancement.

3. **Direct Price Adjustment**: Directly adjusting bid prices based on reputation was considered, but using a separate scoring formula provides more flexibility and transparency.

## References

1. `icn-types/reputation.rs` - Reputation profile and scoring definitions
2. `planetary-mesh/src/lib.rs` - Bid struct and node capabilities
3. `planetary-mesh/src/node.rs` - Bid selection logic

## Implementation Example

See `crates/p2p/planetary-mesh/src/reputation_integration.rs` for the implementation of the reputation client and bid evaluation logic.

See `examples/reputation_bid_weights.ccl` for an example CCL contract defining bid weights. 