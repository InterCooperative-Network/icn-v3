# RFC 0023: Receipt Broadcasting and DAG Sync

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Runtime & Networking Teams
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0016 (Mesh Pipeline), RFC 0020 (Planetary Mesh), RFC 0024 (Peer Discovery)

---

## 0. Abstract

This RFC defines the protocol for broadcasting execution receipts and synchronizing their content across the InterCooperative Network’s decentralized DAG. Anchored `ExecutionReceipt`s are announced using signed mesh messages and made available for pull-based sync. This ensures verifiability, redundancy, and federation-level observability.

---

## 1. Introduction

After a job is executed by a mesh node, the outcome is encapsulated in a signed `ExecutionReceipt`. To ensure transparency and auditable trust, these receipts are:

* **Anchored** in a shared, content-addressed DAG
* **Announced** to peers via the mesh
* **Replicated** across the network using CID-based requests

This RFC defines the messaging and sync protocol supporting these operations.

---

## 2. Terminology

* **ExecutionReceipt** – A verifiable record of job outcome and execution metrics
* **CID** – Content Identifier (via multihash) for DAG-anchored receipt
* **Receipt DAG** – Shared graph structure for receipts and anchors
* **DAGSyncRequest / Response** – Protocol for requesting or serving content by CID

---

## 3. Receipt Broadcast

When a runtime node completes a job and anchors the receipt:

1. The receipt is inserted into the local DAG
2. The node emits an `ExecutionReceiptAvailableV1` message:

```rust
pub struct ExecutionReceiptAvailableV1 {
    pub receipt_cid: Cid,
    pub executor: Did,
    pub federation_scope: ScopeKey,
    pub timestamp: Timestamp,
    pub signature: Signature,
}
```

3. Message is published on:

```
/receipts/{federation_id}/available
```

---

## 4. DAG Synchronization

Interested peers (e.g. verifiers, reputation services) may:

1. Subscribe to `/receipts/{federation_id}/available`
2. On new receipt CID, issue `DAGSyncRequest`:

```rust
pub struct DAGSyncRequest {
    pub root_cid: Cid,
    pub requester: Did,
    pub max_depth: Option<u32>,
}
```

3. Node responds with `DAGSyncResponse`, transmitting receipt and links

---

## 5. DAGSync Protocol

* DAG sync uses IPLD-compatible encoding (e.g., DAG-CBOR)
* Requests are authenticated and rate-limited
* Partial DAG syncs supported (e.g., only receipt metadata)
* Gossip is not used for full DAG content

---

## 6. Content Verification

Every receipt must:

* Be signed by the executor DID
* Match its hash (CID)
* Pass deserialization checks

Nodes receiving a receipt via DAGSync should:

* Verify signature and job linkage
* Validate scope and reputation relevance

---

## 7. Storage Guarantees

* Nodes are not required to retain all receipts
* High-trust federation actors (e.g. verifiers) should pin recent receipts
* Archived DAG segments may be distributed via external storage (e.g., IPFS pinning clusters)

---

## 8. Observability

Metrics:

* `receipt_announcements_total`
* `dag_sync_requests_total`
* `dag_sync_success_total`

Logging:

* DAG inserts
* Receipt signature validation failures
* Sync timeout or refusal

---

## 9. Rationale and Alternatives

This architecture separates announcement (push) from content retrieval (pull), improving bandwidth efficiency and minimizing trust assumptions. All receipt content is verifiable and decentralized.

Alternative: gossiping full receipts was rejected due to size, redundancy, and unverifiability.

---

## 10. Backward Compatibility

This protocol is implemented in `planetary-mesh` and `icn-runtime`. The receipt CID and DAG format are stable across v3.

---

## 11. Open Questions and Future Work

* Federated DAG compaction or pruning?
* Persistent receipt indexes for verifiers?
* Time-indexed sync requests (e.g., "all receipts after T")?

---

## 12. Acknowledgements

Thanks to contributors to the receipt DAG, mesh message layer, and verifiable compute flow.

---

## 13. References

* \[RFC 0016: Mesh Execution Pipeline]
* \[RFC 0020: Planetary Mesh Protocol]
* \[RFC 0042: Credential Types and ExecutionReceipts (planned)]

---

**Filename:** `0023-receipt-broadcasting-and-dag-sync.md`
