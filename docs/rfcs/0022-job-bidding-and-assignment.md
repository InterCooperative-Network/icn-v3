# RFC 0022: Job Bidding and Assignment Messaging

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Networking + Runtime Teams
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0016 (Mesh Pipeline), RFC 0020 (Mesh Protocol), RFC 0021 (Topic Design)

---

## 0. Abstract

This RFC defines the messaging protocol and flow for job bidding and assignment in ICN’s decentralized mesh compute layer. It specifies the structure and routing of `JobBidV1` and `AssignJobV1` messages, their usage semantics, and how they interact with topic subscriptions and execution workflows.

---

## 1. Introduction

In ICN’s compute mesh, jobs are submitted by originators and executed by eligible nodes. The selection of an executor is mediated by a bidding process:

* Interested nodes **bid** to execute a job
* Originators **assign** a selected executor
* All communication is scoped via the mesh’s topic model

This system supports:

* Competitive selection based on price, latency, or reputation
* Privacy-preserving execution (only one node receives the full payload)
* Auditability of job selection and status

---

## 2. Message Types

### 2.1 JobBidV1

```rust
pub struct JobBidV1 {
    pub job_cid: Cid,
    pub bidder: Did,
    pub bid_metadata: BidMetadata,
    pub timestamp: Timestamp,
    pub signature: Signature,
}
```

Fields include:

* Reputation score (optional)
* Expected latency or compute class
* Proposed bid price (if economic model applies)

### 2.2 AssignJobV1

```rust
pub struct AssignJobV1 {
    pub job_cid: Cid,
    pub originator: Did,
    pub assigned_executor: Did,
    pub timestamp: Timestamp,
    pub signature: Signature,
}
```

---

## 3. Message Flow

1. Originator announces job on `/jobs/{federation_id}/announce`
2. Executors subscribe to `/jobs/{job_cid}/bids` and publish `JobBidV1`
3. Originator evaluates bids off-chain
4. Originator publishes `AssignJobV1` to `/jobs/{job_cid}/assignment`
5. Assigned node begins execution

---

## 4. Bid Evaluation

Evaluation may consider:

* Peer reputation (from `icn-reputation`)
* Mana balance or execution capacity
* Stated latency/cost in `BidMetadata`
* Federated or cooperative policy filters

The evaluation logic is external to the protocol and may be embedded in:

* Runtime node policy
* Governance rules (e.g., CCL proposal)
* Automated originator-side selector

---

## 5. Security and Integrity

* All bids and assignments are signed by their issuers
* Originator must verify signatures before acting
* Executors may reject assignments they didn’t bid on or validate
* Duplicate or stale assignments should be ignored or penalized

---

## 6. Failure and Resilience

* Executors may timeout or reject execution
* Originators may reassign if no receipt is returned in time
* Retried assignments must reference the same `job_cid` and may increment a version field (future extension)

---

## 7. Observability

All messages are observable via:

* Prometheus metrics (`job_bids_total`, `assignments_total`, `execution_start_total`)
* Tracing spans (`mesh::job::bid`, `mesh::job::assign`)
* DAG-anchored receipts record final executor and success/failure

---

## 8. Rationale and Alternatives

This model enables decentralized, non-exclusive bidding while keeping payload size small and execution private. It avoids flooding job content and supports programmable assignment.

Alternative: flooding job payload to all executors was rejected for privacy and bandwidth concerns.

---

## 9. Backward Compatibility

Job bidding is implemented in `icn-mesh-jobs` and `planetary-mesh`. This RFC documents the current protocol structure.

---

## 10. Open Questions and Future Work

* Should assignment include job payload or CID only?
* Multi-bid aggregation or bundle jobs?
* Slashing or penalization for failed assignments?

---

## 11. Acknowledgements

Thanks to runtime and networking contributors who built the job negotiation flow into the mesh layer.

---

## 12. References

* \[RFC 0016: Mesh Execution Pipeline]
* \[RFC 0020: Planetary Mesh Protocol]
* \[RFC 0021: Topic Design and Subscription Model]

---

**Filename:** `0022-job-bidding-and-assignment.md`
