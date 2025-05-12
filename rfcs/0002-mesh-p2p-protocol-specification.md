# RFC: Mesh P2P Protocol Specification

**Status:** Draft
**Version:** 0.1.0
**Authors:** ICN System-Aware Assistant, ICN Development Team
**Date:** (Current Date)

## 1. Introduction

This RFC provides a detailed specification for the peer-to-peer (P2P) communication protocol used within the ICN Planetary Mesh. The "Planetary Mesh Architecture" RFC (RFC-0001) outlines the conceptual framework, core components like the `MeshNode`, and the overall operational flows of the mesh. This document builds directly upon that foundation by formally defining the wire-level details of the messages exchanged between `MeshNode`s.

The primary motivation for this specification is to ensure unambiguous communication, facilitate interoperable implementations of `MeshNode`s, and establish a clear versioning strategy for protocol evolution. By formalizing the schema, validation rules, expected interaction patterns, and security considerations for each message variant within the `MeshProtocolMessage` enum, this RFC aims to:

* Serve as a definitive guide for developers building or integrating with the Planetary Mesh.
* Enable third-party auditing and verification of protocol compliance.
* Provide a stable base for future extensions and upgrades to the P2P layer.
* Clarify the precise usage of underlying libp2p transport mechanisms (Gossipsub, Kademlia DHT, direct messaging) in the context of specific mesh operations.

This document will cover transport mechanisms, protocol versioning, a detailed breakdown of each `MeshProtocolMessage` variant, topic structures, security rules, and considerations for future compatibility. 

## 2. Transport Mechanisms

The Planetary Mesh P2P protocol utilizes the robust and flexible networking capabilities provided by libp2p. Specific `MeshProtocolMessage` variants are typically transmitted using one or a combination of the following libp2p transport mechanisms, chosen based on the message's purpose (e.g., broadcast, targeted delivery, content discovery).

All P2P messages are encapsulated within the `MeshBehaviour` of a `MeshNode`'s libp2p `Swarm`. The serialization format for all `MeshProtocolMessage` variants is **CBOR (Concise Binary Object Representation)**, as per ICN standards (refer to ADR-0002-dag-codec for related decisions on `dag-cbor`).

### 2.1. Gossipsub

Libp2p's Gossipsub protocol is the primary mechanism for scalable, topic-based publish/subscribe messaging. It is used for messages that require broad dissemination to many potentially interested peers without prior direct connections.

*   **Typical Use Cases:**
    *   `CapabilityAdvertisementV1`: Broadcasting node capabilities to the network.
    *   `JobAnnouncementV1`: Announcing new jobs to potential executors.
    *   `ExecutionReceiptAvailableV1`: Announcing the availability of a new execution receipt (typically its CID and key metadata).
*   **Characteristics:**
    *   **Resilience:** Messages propagate through the network even with node churn.
    *   **Scalability:** Efficiently disseminates messages to large numbers of subscribers.
    *   **Topic-Based:** Nodes subscribe to specific topics relevant to their interests (e.g., a global job announcement topic, a capability topic).
*   **Considerations:**
    *   **Message Duplication:** Gossipsub handles message deduplication.
    *   **Not Strictly Ordered:** Message delivery order is not guaranteed across the network.
    *   **Spam Mitigation:** Relies on Gossipsub's peer scoring and other mechanisms to mitigate spam. Specific topic structures and validation rules (detailed later) also play a role.

### 2.2. Direct Messaging (Request-Response & Unicast Streams)

For targeted communication between two specific peers, the protocol relies on libp2p's direct messaging capabilities, which can be implemented using request-response patterns or unicast streams.

*   **Typical Use Cases:**
    *   `JobBidV1`: An executor sending a specific bid to a job originator.
    *   `AssignJobV1`: An originator assigning a job to a chosen executor.
    *   `JobStatusUpdateV1`: An executor sending status updates directly to the job originator.
    *   `JobInteractiveInputV1` / `JobInteractiveOutputV1`: Exchanging data streams for interactive jobs between the originator and executor.
    *   Directly requesting a full `ExecutionReceipt` from a peer known to have it.
*   **Characteristics:**
    *   **Targeted:** Messages are sent to a known `PeerId`.
    *   **Potentially Reliable:** Can be layered over reliable transport protocols provided by libp2p.
    *   **Lower Latency (Potentially):** Avoids the propagation delays inherent in Gossipsub for direct peer-to-peer interactions.
*   **Considerations:**
    *   **Peer Discovery:** Requires the sender to know the `PeerId` and address(es) of the recipient. This is often facilitated by Kademlia DHT or prior interaction.
    *   **Connection Management:** Libp2p handles underlying connection establishment and maintenance.

### 2.3. Kademlia (Kad-DHT)

Libp2p's Kademlia-based Distributed Hash Table (Kad-DHT) is employed for decentralized peer discovery and content addressing/retrieval.

*   **Typical Use Cases:**
    *   **Peer Discovery:** Finding other `MeshNode`s on the network, discovering their addresses, and bootstrapping connections for Gossipsub and direct messaging.
    *   **Content Discovery & Retrieval:**
        *   Storing and retrieving full `ExecutionReceipt` objects using their CIDs as keys. Executor nodes `put` their receipts into the DHT, and interested parties (originators, auditors) `get` them.
        *   Potentially storing and retrieving other content-addressable data like WASM modules or large job input data, although this might also be handled via direct transfer or other ICN data availability layers.
    *   **Provider Records:** Nodes advertise to the DHT that they are "providers" for specific CIDs (e.g., they hold a copy of a particular `ExecutionReceipt` or WASM module), allowing other nodes to discover and connect to them to retrieve the data.
*   **Characteristics:**
    *   **Decentralized:** No central point of failure for discovery or storage.
    *   **Content-Addressable:** Data is typically identified and retrieved by its CID.
    *   **Resilient:** DHT records are replicated across multiple nodes.
*   **Considerations:**
    *   **Churn:** The DHT must be robust to nodes joining and leaving the network.
    *   **Storage Overhead:** Nodes participating in the DHT contribute to storing routing information and (for providers) the data itself.
    *   **Lookup Times:** DHT lookups involve iterative requests and can have higher latency than direct messaging once a peer is known.

The choice of transport mechanism for each message variant is crucial for network efficiency, scalability, and reliability. Subsequent sections will specify the intended transport for each message type.

## 3. Protocol Versioning and Compatibility

To accommodate future enhancements, bug fixes, and evolving requirements, the Planetary Mesh P2P protocol incorporates a versioning strategy. This strategy aims to ensure clarity, enable interoperability between nodes running different compatible protocol versions, and provide a managed path for introducing breaking changes.

### 3.1. Message Variant Versioning

Each distinct message type within the `MeshProtocolMessage` enum is explicitly versioned as part of its variant name. This is indicated by a `V` followed by a version number suffix.

*   **Examples:**
    *   `CapabilityAdvertisementV1`
    *   `JobAnnouncementV1`
    *   `JobBidV1`

When a change is made to the schema or semantics of a message variant that is **not backward compatible**, a new variant with an incremented version number MUST be introduced (e.g., `JobAnnouncementV2`). Older versions may be supported concurrently for a deprecation period.

Minor, backward-compatible changes (e.g., adding new optional fields to a CBOR map) MAY be introduced to an existing message variant version without incrementing the version number, provided that nodes parsing the message can safely ignore unknown fields. However, for clarity and explicit contract definition, even backward-compatible additions often benefit from a new version if they introduce significantly new functionality.

### 3.2. Global Protocol Identifier (Optional)

While individual messages are versioned, a global protocol identifier string for the entire suite of `MeshProtocolMessage`s (e.g., `/icn/mesh/protocol/0.1.0`) MAY be used in libp2p stream negotiation or capability announcements. This can help nodes quickly identify if they share a baseline understanding of the mesh protocol suite. However, the primary mechanism for handling changes is at the individual message variant level.

### 3.3. Compatibility Strategy

*   **Backward Compatibility:**
    *   Nodes SHOULD be tolerant of receiving older versions of message variants they understand, if the older version can still be processed meaningfully according to the current node's logic.
    *   When introducing new *optional* fields to an existing message variant version, these fields MUST be structured (e.g., in CBOR maps) such that older clients can ignore them without error.
*   **Forward Compatibility:**
    *   Nodes SHOULD be designed to gracefully handle (e.g., ignore or log a warning for) unknown message variants or newer versions of known message variants they do not yet understand. This prevents older nodes from crashing or misbehaving when encountering messages from newer nodes.
*   **Deprecation:**
    *   When a new, non-backward-compatible version of a message variant is introduced (e.g., `JobAnnouncementV2` superseding `JobAnnouncementV1`), there SHOULD be a clearly communicated deprecation period for the older version.
    *   During this period, nodes MAY support both sending and receiving both versions to ensure smooth network upgrades.
    *   After the deprecation period, support for the older version MAY be removed.
*   **Negotiation (Future):**
    *   For complex interactions or critical messages, explicit version negotiation mechanisms MAY be introduced in the future, where peers can advertise the range of message versions they support for a particular interaction. For now, versioning is primarily on the sender to emit understandable messages and the receiver to be tolerant.

### 3.4. Serialization and Deserialization

*   Nodes MUST strictly adhere to the CBOR schemas defined for each message variant version.
*   Parsers MUST be robust to encountering additional, unspecified fields in CBOR maps if the message versioning strategy allows for optional field additions. They should ignore these extra fields rather than failing to parse.
*   If a required field is missing according to the schema for a given message variant version, the message MUST be considered malformed and SHOULD be rejected or handled as an error.

This versioning approach allows for incremental evolution of the protocol, ensuring that `MeshNode`s can continue to interoperate effectively even as new features and improvements are introduced.

## 4. MeshProtocolMessage Overview

The `MeshProtocolMessage` enum, typically defined in `planetary-mesh/src/protocol.rs`, encapsulates all P2P messages exchanged between `MeshNode`s in the ICN Planetary Mesh. Each variant of this enum represents a distinct type of message with a specific purpose in the job lifecycle, capability advertisement, or other mesh operations.

All messages are serialized using **CBOR (Concise Binary Object Representation)**. The following is a list of the primary message variants. Detailed specifications for each, including their fields, schema, intended transport, and security considerations, will be provided in Section 5.

*   **`CapabilityAdvertisementV1`**:
    *   **Purpose:** Allows executor nodes to broadcast their capabilities (e.g., resources, supported WASM versions, region) to the network.
*   **`JobAnnouncementV1`**:
    *   **Purpose:** Used by job originators to announce new jobs, including their `MeshJobParams` and `ExecutionPolicy`, to make them discoverable by potential executors.
*   **`JobBidV1`**:
    *   **Purpose:** Carries an executor's formal bid for a job, including price and other relevant metadata like `executor_did` and `region`.
*   **`AssignJobV1`**:
    *   **Purpose:** Sent by a job originator to a selected executor to formally assign them the job.
*   **`JobStatusUpdateV1`**:
    *   **Purpose:** Sent by an executor to the job originator to provide updates on the current status of an assigned job.
*   **`ExecutionReceiptAvailableV1`**:
    *   **Purpose:** Sent by an executor after job completion to announce that the signed `ExecutionReceipt` (identified by its CID) is available.
*   **`JobInteractiveInputV1`**:
    *   **Purpose:** Used in interactive jobs to send data from the originator to the executor during active job execution.
*   **`JobInteractiveOutputV1`**:
    *   **Purpose:** Used in interactive jobs to send data from the executor back to the originator during active job execution.

The design of these messages aims for clarity and efficiency, providing the necessary information for each stage of interaction within the Planetary Mesh while adhering to the versioning and compatibility strategies outlined in Section 3. 