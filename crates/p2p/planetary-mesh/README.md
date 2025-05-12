# Planetary Mesh with Reputation-Driven Bid Evaluation

The Planetary Mesh component of ICN provides decentralized job execution across the network. This implementation includes a reputation-driven bid evaluation system that integrates reputation scores into the job allocation process.

## Key Features

- **Reputation Integration**: Job allocators consider node reputation when selecting executors, favoring nodes with proven track records.
- **Multi-factor Bid Evaluation**: Bids are evaluated based on a weighted formula that includes:
  - Price (lower is better)
  - Resource match (how well node resources match job requirements)
  - Reputation score (higher is better)
  - Timeliness (percentage of jobs completed on time)
- **Configurable Weights**: All evaluation factors have adjustable weights that can be tuned via CCL governance policies.
- **Verified Reputation**: Self-reported reputation scores can be verified against the reputation service.

## Architecture

The reputation integration consists of the following components:

1. **ReputationClient**: Interface to fetch reputation profiles from the reputation service.
2. **BidEvaluatorConfig**: Configuration for the weights used in bid evaluation.
3. **Bid Evaluation Logic**: Implemented in `MeshNode`, evaluates bids using the weighted formula.

## How Bid Scoring Works

Bids are scored using the following formula:

```
total_score = 
  w_price·(1 - norm_price) +
  w_resources·resource_match +
  w_reputation·(reputation_score/100) +
  w_timeliness·(on_time_jobs/total_successful_jobs)
```

Where:
- `w_price`, `w_resources`, `w_reputation`, `w_timeliness` are configurable weights
- `norm_price` is the normalized price (0-1 range)
- `resource_match` indicates how well node resources match job requirements (0-1 range)
- `reputation_score` is the node's reputation score (0-100 range)
- `on_time_jobs` and `total_successful_jobs` are from the node's reputation profile

## Configuration via CCL

The weights and other parameters can be configured via a CCL contract:

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

See `examples/reputation_bid_weights.ccl` for a complete example.

## Usage

The reputation-based bid evaluation is automatically used by the `MeshNode` when selecting executors for jobs. No additional configuration is needed to enable it.

## Future Work

- Implement resource matching logic based on job requirements and node capabilities
- Add support for minimum reputation requirements for specific job types
- Introduce reputation boosting for specialized capabilities
- Develop a feedback loop where job execution outcomes directly affect bid evaluation 