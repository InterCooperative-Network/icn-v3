# RFC 0024: Gossip Strategy and Peer Discovery

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Networking Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0020 (Planetary Mesh), RFC 0021 (Topic Design), RFC 0023 (Receipt Broadcasting)

---

## 0. Abstract

This RFC defines the peer discovery and gossip strategies used in ICN's planetary mesh. It outlines how mesh nodes join, maintain, and interact with the peer-to-peer network, focusing on libp2p protocols, scope-aware filtering, and resilience mechanisms. It ensures that ICN’s decentralized compute fabric remains robust, scalable, and privacy-conscious.

---

## 1. Introduction

The ICN relies on a decentralized mesh to coordinate compute, broadcast receipts, and propagate cooperative state. To support this, nodes must:

* Discover compatible peers
* Join appropriate topic overlays
* Gossip relevant messages without overwhelming the network

This RFC specifies how libp2p features are used to implement these goals, including mDNS, Kademlia, and GossipSub configurations.

---

## 2. Peer Discovery Mechanisms

### 2.1 Multicast DNS (mDNS)

* Used for local peer discovery
* Activated in development/test deployments
* Fast bootstrap with minimal config

### 2.2 Kademlia DHT

* Used for public and federated deployments
* Supports:

  * Peer address resolution
  * Federation identity mapping (future extension)
* Nodes advertise under scope-prefixed peer records

### 2.3 Static Peering (Optional)

* Nodes may be configured with static bootstrap peers
* Used for cooperative or validator clusters

---

## 3. Peer Metadata Exchange

Upon connection, nodes exchange:

* DID
* Federation and cooperative scope keys
* Peer roles (executor, verifier, observer)
* Reputation score (optional preview)

This metadata is used to:

* Filter topic subscriptions
* Prefer high-reputation peers in message relaying
* Inform peer scoring heuristics

---

## 4. Gossip Strategy (GossipSub v1.1)

GossipSub is configured with:

* Mesh overlay for each topic
* Randomized peer fanout per topic (default: 6)
* Heartbeat intervals for mesh health checks
* Message validation: signature + CID + deserialization

Nodes maintain:

* Topic-level peer scoring
* History windows for message deduplication

---

## 5. Peer Scoring Heuristics

Peers are scored on:

* Message delivery rate and timeliness
* Validity of recent messages (CID integrity, signature)
* Scope overlap and federation match
* DAG sync responsiveness

Scores influence:

* Message propagation preference
* Mesh retention during rebalance
* Drop/ban decisions on misbehavior

---

## 6. Privacy Considerations

* DIDs and role metadata may be redacted unless needed for scoped topics
* Nodes may operate in "observer mode" without announcing intent to execute
* No location or IP metadata is shared beyond libp2p connection layer

---

## 7. Fault Tolerance

* Nodes periodically re-bootstrap via DHT
* Mesh overlays are rebuilt on peer churn
* Sync requests may retry alternate peers on failure

---

## 8. Observability

Metrics:

* `peer_connections_total`
* `gossipsub_mesh_peers{topic=...}`
* `peer_scores{peer=..., score=...}`

Tracing:

* Peer join/leave events
* Bootstrap and rebalance intervals

---

## 9. Rationale and Alternatives

libp2p was selected for:

* Modular, battle-tested P2P protocols
* Support for scoped pubsub overlays
* Peer scoring and metadata extensions

Alternative: custom P2P stack or central relays were rejected due to limited scalability and trust assumptions.

---

## 10. Backward Compatibility

This behavior is implemented in `planetary-mesh` and actively used in ICN v3 deployments. This RFC documents the current configuration and practices.

---

## 11. Open Questions and Future Work

* Geo-aware peer clustering or latency heuristics?
* Encrypted topic overlays for private federations?
* Peer reputation inclusion from `icn-reputation`?

---

## 12. Acknowledgements

Thanks to libp2p contributors and all developers building `planetary-mesh`, especially around peer management and adaptive gossiping.

---

## 13. References

* \[RFC 0020: Planetary Mesh Protocol]
* \[RFC 0023: Receipt Broadcasting and DAG Sync]
* [libp2p GossipSub Scoring Guide](https://docs.libp2p.io/concepts/publish-subscribe/gossipsub/#scoring)

---

**Filename:** `0024-gossip-strategy-and-peer-discovery.md`
