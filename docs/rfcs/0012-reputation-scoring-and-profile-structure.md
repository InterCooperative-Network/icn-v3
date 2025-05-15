# RFC 0012: Reputation Scoring and Profile Structure

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0010 (Mana Accounting), RFC 0011 (Runtime ABI), RFC 0016 (Mesh Execution Pipeline), RFC 0042 (ExecutionReceipts)

---

## 0. Abstract

This RFC defines the structure, scoring model, and service behavior for reputation within the InterCooperative Network (ICN). Reputation quantifies the historical execution performance of DIDs (identities), supporting trust evaluation, mana influence, and job scheduling. The system is transparent, scoped, and tunable.

---

## 1. Introduction

The ICN is a trust-sensitive compute and governance network. Reputation acts as a decentralized accountability mechanism. It is earned through verifiable execution, affects execution privileges (e.g., mana regeneration), and influences economic and governance processes.

This document defines:

* The reputation profile structure
* The scoring algorithm and input weighting
* How execution receipts feed into profile updates
* The APIs for querying and using reputation data

---

## 2. Terminology

* **DID** – Decentralized Identifier representing an agent
* **ExecutionReceipt** – Verifiable signed record of job outcome
* **ReputationModifier** – Additive or multiplicative factor affecting scores
* **Sigmoid Parameters** – Tunable scoring function defining steepness and asymptotes
* **ScopeKey** – Reputation may be aggregated at individual, coop, or federation levels

---

## 3. Profile Structure

Each DID is associated with a reputation profile:

```rust
pub struct ReputationProfile {
    pub did: Did,
    pub total_jobs: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub accumulated_score: f64,
    pub score_history: Vec<ScoreChange>,
    pub last_updated: Timestamp,
}
```

Profiles are stored and versioned in `icn-reputation`, indexed by DID.

---

## 4. Scoring Algorithm

Each job execution triggers a reputation update based on the `ExecutionReceipt` outcome. The process:

1. Extract:

   * Success/failure
   * Mana cost
   * Latency or execution time
2. Apply modifiers:

   * Penalties for failures (proportional to cost)
   * Bonuses for high-value, timely completions
3. Compute delta:

```rust
let delta = sigmoid(mana_cost, slope, midpoint);
if failure { delta *= -penalty_factor; }
```

4. Update profile score with rolling accumulation

### Sigmoid Function

```rust
fn sigmoid(x: f64, slope: f64, midpoint: f64) -> f64 {
    1.0 / (1.0 + (-slope * (x - midpoint)).exp())
}
```

This ensures diminishing returns on very large jobs and zero contribution from trivial executions.

---

## 5. Reputation Service API

The `icn-reputation` service exposes:

### Ingestion

```http
POST /reputation/receipts
Body: ExecutionReceipt
```

### Mana Adjustments

```http
POST /reputation/mana-adjustments
Body: { did, amount }
```

### Querying

```http
GET /reputation/profiles/{did}
GET /reputation/profiles/{did}/history
GET /reputation/leaderboard?scope=coop&limit=10
```

These endpoints enable dashboards, validators, and schedulers to act on reputation scores.

---

## 6. Runtime Integration

* `Runtime::anchor_receipt()` calls the HTTP reputation client
* Prometheus metrics track submission latency and errors
* Optional caching or batching supported for performance

---

## 7. Rationale and Alternatives

This system balances fairness, simplicity, and tunability:

* Sigmoid scoring reflects diminishing marginal trust
* Penalties discourage abuse without excessive harshness
* Temporal evolution is visible via `score_history`

Alternatives considered:

* Linear scoring: insufficient nuance
* Machine learning: excessive complexity

---

## 8. Security Considerations

* All scores are derived from signed, anchored receipts
* The reputation service must validate receipt authenticity and origin
* Anti-spam thresholds may be required for extreme job volume

---

## 9. Privacy Considerations

* All DIDs are pseudonymous by design
* Profiles are public and auditable
* No sensitive personal data is stored or required

---

## 10. Economic Impact

* Reputation influences mana regeneration indirectly
* Executors with higher scores may be prioritized in job assignment
* Governance roles may require minimum scores

---

## 11. Open Questions and Future Work

* Time-decay or sliding window scores?
* Reputation fusion across multiple scopes (e.g., coop + individual)?
* Integration with staking or bonding?

---

## 12. Acknowledgements

Thanks to the contributors to `icn-reputation`, `icn-runtime`, and the design of `ExecutionReceipt` for foundational work that made this scoring system feasible.

---

## 13. References

* \[RFC 0010: Mana Accounting and Regeneration]
* \[RFC 0016: Mesh Execution Pipeline (planned)]
* \[RFC 0042: Credential Types and ExecutionReceipts]

---

**Filename:** `0012-reputation-scoring-and-profile-structure.md`
