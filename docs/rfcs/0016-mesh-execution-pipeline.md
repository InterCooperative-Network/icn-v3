# RFC 0016: Mesh Execution Pipeline (Job → Receipt → Reputation)

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0010 (Mana), RFC 0012 (Reputation), RFC 0011 (Host ABI), RFC 0042 (ExecutionReceipts)

---

## 0. Abstract

This RFC defines the end-to-end pipeline for executing mesh jobs in ICN—from submission and P2P distribution through runtime execution, anchoring of verifiable receipts, and final propagation to the reputation service. It formalizes the interfaces and message flows across services and runtimes, ensuring trust, traceability, and accountability.

---

## 1. Introduction

In the ICN mesh, jobs are distributed and executed by participating nodes in a decentralized fashion. The trust model relies on:

* Verifiable job assignment
* Cryptographically signed execution receipts
* DAG-based receipt anchoring
* Transparent submission of results to a scoring engine

This RFC documents that full pipeline and its primary actors.

---

## 2. Terminology

* **JobRequest** – The structured request containing parameters for a mesh job
* **MeshJob** – The internal representation of a job in transit
* **ExecutionReceipt** – A signed result of WASM execution (with mana cost, output hash, etc.)
* **DAG Anchor** – A cryptographic insertion of receipt into the shared runtime DAG
* **Reputation Update** – Submission of a validated receipt to the scoring service

---

## 3. Execution Lifecycle

### 3.1 Stages

```
Job Creation → Bidding → Assignment → Execution → Receipt Anchoring → Reputation Submission
```

Each stage is implemented by different actors:

* **Originator node**: Creates job, awaits receipt
* **Mesh node**: Bids, accepts, and executes
* **Runtime**: Executes WASM and anchors receipt
* **Reputation service**: Scores outcome

---

## 4. Protocol Events

### 4.1 Job Lifecycle

* `JobAnnouncementV1`
* `JobBidV1`
* `AssignJobV1`
* `JobStatusUpdateV1`
* `ExecutionReceiptAvailableV1`

### 4.2 Internal Interfaces

* `host_submit_mesh_job(params)` (runtime → mesh layer)
* `Runtime::execute_mesh_job()` (mesh layer → runtime)
* `ReputationUpdater::submit(receipt)` (runtime → reputation)

---

## 5. ExecutionReceipt

Defined as:

```rust
pub struct RuntimeExecutionReceipt {
    pub job_cid: Cid,
    pub output_cid: Option<Cid>,
    pub executor: Did,
    pub originator: Did,
    pub scope_key: ScopeKey,
    pub metrics: RuntimeExecutionMetrics,
    pub signature: Option<Signature>,
}
```

Receipts must:

* Be signed by the executor
* Include mana cost and execution metadata
* Be anchored in a DAG

---

## 6. DAG Anchoring

Implemented via `DagStore::insert(receipt.cid())`. This ensures:

* Content-addressed deduplication
* Tamper-evident structure
* Auditable history of mesh participation

Anchored receipts are broadcast with `ExecutionReceiptAvailableV1` to interested peers.

---

## 7. Reputation Submission

Anchored receipts are asynchronously submitted to the reputation service by the runtime. On success:

* The reputation profile of the executor is updated
* Mana cost is applied to influence scoring
* Prometheus metrics record the event

---

## 8. Error Handling and Resilience

* Retriable errors during DAG or reputation submission are logged and retried with exponential backoff
* Failed executions are still anchored as receipts with failure status
* Originators may reject receipts with invalid signatures or failed DAG inserts

---

## 9. Observability

Each stage is instrumented with:

* Prometheus metrics (latency, failure counts, receipt counts)
* Tracing spans (`mesh::submit_job`, `runtime::execute_job`, `reputation::submit`)

---

## 10. Rationale and Alternatives

The mesh pipeline prioritizes verifiability, modularity, and auditability. Each step is observable and content-addressed. This avoids black-box trust and enables reputation-aware decision-making.

Alternative: central verification or opaque scheduling layers were rejected to preserve decentralization and auditability.

---

## 11. Backward Compatibility

This pipeline is already implemented in ICN v3. This RFC documents existing behavior across `icn-runtime`, `icn-mesh-jobs`, `planetary-mesh`, and `icn-reputation`.

---

## 12. Open Questions and Future Work

* Multi-node redundancy (same job executed by >1 node)?
* Receipt compression or proof systems for DAG storage?
* Conditional execution or expiration windows?

---

## 13. Acknowledgements

Thanks to contributors to the planetary mesh protocol, runtime anchoring, and receipt signature flows.

---

## 14. References

* \[RFC 0010: Mana Accounting]
* \[RFC 0012: Reputation Scoring]
* \[RFC 0042: ExecutionReceipts and Credentialing (planned)]

---

**Filename:** `0016-mesh-execution-pipeline.md`
