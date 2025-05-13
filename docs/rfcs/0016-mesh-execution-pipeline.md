---

RFC: 0016
Title: Mesh Job Execution Pipeline Specification
Author: Matt Faherty
Date: 2025-05-12
Status: Draft

# Summary

This RFC defines the end-to-end *Mesh Job Execution Pipeline* within the InterCooperative Network (ICN). It formalizes the sequence of protocol messages, state transitions, and responsibilities from job announcement through bid evaluation, assignment, execution, and receipt anchoring.

# Motivation

To ensure interoperability, observability, and verifiability of mesh-based compute tasks, we need a canonical specification that:

1. Defines each protocol message and its schema.
2. Clarifies state transitions in originator and executor nodes.
3. Specifies timing and retry semantics.
4. Outlines error handling and fallback behaviors.

# Goals

* Provide a clear reference for implementers of `p2p/planetary-mesh`.
* Ensure consistency across versions of mesh clients and mesh services.
* Enhance test coverage by codifying expected behaviors in integration tests.

# Non-Goals

* Introducing new message variants (reserved for future RFCs).
* Detailing economic policy weights or reputation algorithms (covered elsewhere).

# Protocol Overview

The pipeline consists of the following high-level stages:

1. **Job Announcement**: Originator broadcasts `JobAnnouncementV1`.
2. **Bid Submission**: Executors submit `JobBidV1` messages.
3. **Bid Evaluation**: Originator collects bids, applies configured policy, and selects a winning bid.
4. **Job Assignment**: Originator sends `AssignJobV1` to the selected executor.
5. **Job Execution**: Executor runs the WASM job in `icn-runtime`.
6. **Execution Receipt**: Executor anchors and announces `ExecutionReceiptAvailableV1`.

# Schema Definitions

## 1. JobAnnouncementV1

```rust
pub struct JobAnnouncementV1 {
    pub job_id: Cid,
    pub originator_did: String,
    pub params: MeshJobParams,
    pub timestamp: u64,
}
```

## 2. JobBidV1

```rust
pub struct JobBidV1 {
    pub job_id: Cid,
    pub executor_did: String,
    pub price: u64,
    pub resources: ResourceRequirements,
    pub timestamp: u64,
}
```

## 3. AssignJobV1

```rust
pub struct AssignJobV1 {
    pub job_id: Cid,
    pub executor_did: String,
    pub assignment_id: Uuid,
    pub timestamp: u64,
}
```

## 4. ExecutionReceiptAvailableV1

```rust
pub struct ExecutionReceiptAvailableV1 {
    pub job_id: Cid,
    pub executor_did: String,
    pub receipt_cid: Cid,
    pub timestamp: u64,
}
```

# State Transitions

| Originator Node          | Executor Node               |
| ------------------------ | --------------------------- |
| Idle                     | Idle                        |
| ➜ Broadcast Announcement | ◀ Idle                      |
| Awaiting Bids            | ➜ Receive Announcement      |
| ➜ Select Bid             | ◀ Submit Bid                |
| ➜ Send AssignJob         | ➜ Receive AssignJob         |
| Awaiting Receipt         | ➜ Execute Job               |
| ◀ Receive Receipt        | ➜ Anchor & Announce Receipt |
| Completed                | Completed                   |

# Timing & Retries

* Announcements: broadcast every *T₁* seconds until at least one bid received or *N₁* retries.
* Bids: executors may retry up to *N₂* times on failure.
* Assignment: originator retries *N₃* times if no ack from executor.

# Error Handling

* If no bids arrive within timeout *T₂*, originator may rebroadcast announcement or abort.
* Executors should validate receipts against `MeshJobParams` and report errors via a `JobErrorV1` (reserved for future RFC).

# Test Vectors

* Integration tests MUST cover:

  * Single executor happy path.
  * Multiple competing bids with policy-driven selection.
  * Executor failure during execution and retry semantics.

# Future Work

* Formalize `JobErrorV1` and recovery flows.
* Integrate reputation-based bid weighting into announcement TTL adjustments.

---
