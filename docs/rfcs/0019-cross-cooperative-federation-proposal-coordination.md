---

RFC: 0019
Title: Cross-Cooperative Federation Proposal Coordination
Author: Matt Faherty
Date: 2025-05-12
Status: Draft

# Summary

Defines the mechanisms by which multiple ICN cooperatives coordinate, propose, and vote on federation-wide initiatives across autonomous Agoranet instances. It outlines message flows, trust bundling, quorum aggregation, and result dissemination.

# Motivation

Cooperatives often need to collaborate on shared initiatives (e.g., shared infrastructure, joint budgets, policy alignment). A standardized cross-cooperative proposal flow ensures:

* Secure, verifiable voting across multiple trust domains.
* Consistent quorum and threshold calculations.
* Reliable propagation of results back to originator and participating cooperatives.

# Goals

1. Define cross-cooperative proposal announcement, voting, and result phases.
2. Specify trust bundle composition and signature verification across domains.
3. Formalize quorum aggregation and threshold semantics for federated votes.
4. Outline recovery and fallback if some cooperatives are unreachable.

# Non-Goals

* Low-level economic settlement of federated budgets (handled in economic RFCs).
* Detailed UI/UX of cross-coop voting interfaces.

# Protocol Phases

## 1. Proposal Broadcast

* **Originator Cooperative** constructs `FederationProposalV1` with:

  * `proposal_id`, `title`, `body`, `scope: federation:<federation_id>`, `timestamp`, `required_quorum`
* Broadcast via federation-wide P2P topic or REST API fan-out to each Agoranet.

## 2. Local Validation

Each participant Agoranet instance:

1. Verifies originator DID signature against trust bundle.
2. Ensures `scope` matches its federation membership.
3. Registers proposal in local queue and opens voting window.

## 3. Vote Submission

Participants submit `FederationVoteV1`:

```rust
pub struct FederationVoteV1 {
    pub proposal_id: Cid,
    pub voter_did: String,
    pub choice: VoteChoice, // Yes/No/Abstain
    pub timestamp: u64,
}
```

* Signed by local member DID.
* Sent to local Agoranet, which anchors vote receipt and forwards a signed aggregate to originator.

## 4. Quorum Aggregation

Originator collects `FederationVoteAggregateV1` from each coop:

```rust
pub struct FederationVoteAggregateV1 {
    pub proposal_id: Cid,
    pub coop_id: String,
    pub votes_for: u64,
    pub votes_against: u64,
    pub votes_abstain: u64,
    pub signature: String, // Coop-level JWS
    pub timestamp: u64,
}
```

* Originator validates each coop’s signature.
* Totals votes across cooperatives.
* Compares against `required_quorum` (e.g., 60% of total cooperatives weighted equally or by membership size).

## 5. Result Dissemination

* Originator publishes `FederationProposalResultV1` with outcome and aggregate tallies.
* Each Agoranet instance verifies and updates local state (e.g., enact policy, notify members).

# Trust Bundles & Signature Verification

* Trust bundle for federation comprised of each coop’s DID signer.
* Bundles refreshed on rotation or on membership changes.
* All messages signed with JWS; signature header includes `kid` referencing key in bundle.

# Failure & Recovery

* **Timeouts**: If coop fails to send aggregate within window *T₃*, originator marks as abstain.
* **Network Partitions**: Later reparations allow late joins; originator may re-aggregate and publish updated results.
* **Revocation**: Coop-level key revocations remove participant from quorum and adjust required thresholds.

# Test Cases

* Multi-coop happy path with unanimous approval.
* One coop offline: aggregated as abstain, still passes quorum.
* Coop key rotation mid-vote: test revocation handling.

# Future Work

* Weighted voting by membership size or reputational metrics.
* Support for multi-round deliberation and amendment flows.
* UI specification for cross-coop dashboards.

---
