# RFC 0021: Libp2p Topic Design and Subscription Model

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Networking Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0020 (Planetary Mesh), RFC 0022 (Bidding Protocol), RFC 0024 (Peer Discovery)

---

## 0. Abstract

This RFC defines the topic hierarchy and subscription model for ICN’s mesh protocol, implemented via libp2p’s GossipSub. Topics are scoped to federation identifiers, job contexts, and content identifiers (CIDs), enabling efficient and privacy-preserving dissemination of mesh messages.

---

## 1. Introduction

ICN’s planetary mesh uses topic-based pub/sub to route messages related to compute jobs, receipts, proposals, and DAG anchors. This document formalizes the canonical topic structure and node subscription behavior for `planetary-mesh`.

Goals:

* Limit message flooding to relevant scopes
* Enable scoped filtering and access control
* Reduce bandwidth and memory pressure on participants

---

## 2. Terminology

* **Topic** – A pub/sub channel in libp2p GossipSub
* **Scope** – A federation, community, cooperative, or individual-level identifier
* **CID** – Content identifier for job payloads or receipts
* **Interest Set** – The subset of topics a mesh node actively listens to

---

## 3. Topic Naming Convention

Topic names follow a hierarchical URI-like structure:

```
/jobs/{federation_id}/announce
/jobs/{job_cid}/bids
/jobs/{job_cid}/assignment
/jobs/{job_cid}/status
/receipts/{federation_id}/available
/dag/{cid}/sync
```

Each topic includes scope-specific data to limit propagation.

---

## 4. Job Topics

### Announcement

```
/jobs/{federation_id}/announce
```

* Originators publish new jobs
* Mesh nodes subscribe if accepting jobs from this federation

### Bidding

```
/jobs/{job_cid}/bids
```

* Bidders respond with `JobBidV1`
* Only relevant nodes subscribe

### Assignment

```
/jobs/{job_cid}/assignment
```

* Originator confirms assigned executor

### Status Updates

```
/jobs/{job_cid}/status
```

* Executor reports progress or errors
* Originator subscribes to their job topics

---

## 5. Receipt and DAG Topics

### Receipt Availability

```
/receipts/{federation_id}/available
```

* Executors announce anchored receipts
* Reputation services or verifiers subscribe

### DAG Synchronization

```
/dag/{cid}/sync
```

* On-demand pull-based sync for receipt or anchor data

---

## 6. Subscription Policy

Each node maintains an **interest set**:

* Derived from its federation, coop, and execution role
* Refreshed periodically or upon job context change
* Nodes may unsubscribe from stale job topics

**Examples**:

* A validator for `fed-a` subscribes to all `/jobs/fed-a/*`
* A job executor only subscribes to `/jobs/{job_cid}/assignment` once assigned
* Reputation node listens to `/receipts/*/available`

---

## 7. Topic Lifecycle and Expiration

* Job topics are ephemeral and may expire after:

  * DAG anchor confirmation
  * TTL of 24–72 hours
* Nodes should implement garbage collection for subscriptions and handlers

---

## 8. Observability

Prometheus metrics per topic:

* `mesh_messages_received_total{topic="/jobs/..."}`
* `mesh_subscriptions{active=true}`
* `mesh_peer_joins{topic=...}`

---

## 9. Rationale and Alternatives

This structure enables content-addressed, permissionless message routing with scoped filtering. It avoids flat global topics (which overload bandwidth) and overly deep topic trees (which fragment the network).

Alternatives considered:

* Static per-node topic routing → inflexible and non-dynamic
* Binary topic filters → not supported by GossipSub

---

## 10. Backward Compatibility

All listed topics are in use in ICN v3. This RFC formalizes naming and usage expectations.

---

## 11. Open Questions and Future Work

* Should topic lifecycles be policy-configurable?
* Federation-topic filters for bandwidth-constrained devices?
* DAG topic compression schemes?

---

## 12. Acknowledgements

Thanks to contributors who designed the job message format and mesh subscription logic.

---

## 13. References

* \[RFC 0020: Planetary Mesh Protocol Overview]
* \[RFC 0022: Bidding and Assignment (planned)]
* [libp2p GossipSub spec](https://github.com/libp2p/specs/tree/master/pubsub/gossipsub)

---

**Filename:** `0021-topic-design-and-subscription-model.md`
