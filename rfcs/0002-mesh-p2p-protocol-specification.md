# RFC: Mesh P2P Protocol Specification

**Status:** Draft
**Version:** 0.1.0
**Authors:** ICN System-Aware Assistant, ICN Development Team
**Date:** 2025-05-11

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

* **Typical Use Cases:**

  * `CapabilityAdvertisementV1`: Broadcasting node capabilities to the network.
  * `JobAnnouncementV1`: Announcing new jobs to potential executors.
  * `ExecutionReceiptAvailableV1`: Announcing the availability of a new execution receipt (typically its CID and key metadata).
* **Characteristics:**

  * **Resilience:** Messages propagate through the network even with node churn.
  * **Scalability:** Efficiently disseminates messages to large numbers of subscribers.
  * **Topic-Based:** Nodes subscribe to specific topics relevant to their interests (e.g., a global job announcement topic, a capability topic).
* **Considerations:**

  * **Message Duplication:** Gossipsub handles message deduplication.
  * **Not Strictly Ordered:** Message delivery order is not guaranteed across the network.
  * **Spam Mitigation:** Relies on Gossipsub's peer scoring and other mechanisms to mitigate spam. Specific topic structures and validation rules (detailed later) also play a role.

### 2.2. Direct Messaging (Request-Response & Unicast Streams)

For targeted communication between two specific peers, the protocol relies on libp2p's direct messaging capabilities, which can be implemented using request-response patterns or unicast streams.

* **Typical Use Cases:**

  * `JobBidV1`: An executor sending a specific bid to a job originator.
  * `AssignJobV1`: An originator assigning a job to a chosen executor.
  * `JobStatusUpdateV1`: An executor sending status updates directly to the job originator.
  * `JobInteractiveInputV1` / `JobInteractiveOutputV1`: Exchanging data streams for interactive jobs between the originator and executor.
  * Directly requesting a full `ExecutionReceipt` from a peer known to have it.
* **Characteristics:**

  * **Targeted:** Messages are sent to a known `PeerId`.
  * **Potentially Reliable:** Can be layered over reliable transport protocols provided by libp2p.
  * **Lower Latency (Potentially):** Avoids the propagation delays inherent in Gossipsub for direct peer-to-peer interactions.
* **Considerations:**

  * **Peer Discovery:** Requires the sender to know the `PeerId` and address(es) of the recipient. This is often facilitated by Kademlia DHT or prior interaction.
  * **Connection Management:** Libp2p handles underlying connection establishment and maintenance.

### 2.3. Kademlia (Kad-DHT)

Libp2p's Kademlia-based Distributed Hash Table (Kad-DHT) is employed for decentralized peer discovery and content addressing/retrieval.

* **Typical Use Cases:**

  * **Peer Discovery:** Finding other `MeshNode`s on the network, discovering their addresses, and bootstrapping connections for Gossipsub and direct messaging.
  * **Content Discovery & Retrieval:**

    * Storing and retrieving full `ExecutionReceipt` objects using their CIDs as keys. Executor nodes `put` their receipts into the DHT, and interested parties (originators, auditors) `get` them.
    * Potentially storing and retrieving other content-addressable data like WASM modules or large job input data, although this might also be handled via direct transfer or other ICN data availability layers.
  * **Provider Records:** Nodes advertise to the DHT that they are "providers" for specific CIDs (e.g., they hold a copy of a particular `ExecutionReceipt` or WASM module), allowing other nodes to discover and connect to them to retrieve the data.
* **Characteristics:**

  * **Decentralized:** No central point of failure for discovery or storage.
  * **Content-Addressable:** Data is typically identified and retrieved by its CID.
  * **Resilient:** DHT records are replicated across multiple nodes.
* **Considerations:**

  * **Churn:** The DHT must be robust to nodes joining and leaving the network.
  * **Storage Overhead:** Nodes participating in the DHT contribute to storing routing information and (for providers) the data itself.
  * **Lookup Times:** DHT lookups involve iterative requests and can have higher latency than direct messaging once a peer is known.

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

## 5. Message Variant Specifications

This section provides detailed specifications for each message variant within the `MeshProtocolMessage` enum. For each variant, we will define its purpose, the schema of its fields (using Rust struct-like definitions for clarity, with types corresponding to `icn-types` or standard Rust types where applicable), the recommended transport mechanism and topic (if any), and key security considerations.

All message payloads are serialized using CBOR.

### 5.1. `CapabilityAdvertisementV1`

*   **Purpose:**
    Allows a `MeshNode` (typically an executor node) to advertise its execution capabilities to the network. This enables job originators or discovery services to identify suitable nodes that can potentially execute their jobs based on resource availability, supported runtimes, geographical region, or other specific features. Advertisements are typically broadcast periodically and also when a node's capabilities change significantly.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::CapabilityAdvertisementV1
    pub struct NodeCapability {
        // DID of the node advertising its capabilities.
        // Type: icn_types::identity::Did (String)
        pub node_did: String,

        // PeerId of the node, for direct network addressing, serialized as a Base58-encoded string.
        // Type: String 
        pub peer_id: String, 

        // Human-readable alias or name for the node (optional).
        // Type: Option<String>
        pub alias: Option<String>,

        // Geographical region of the node (e.g., "us-east-1", "eu-central").
        // Aligns with icn_types::jobs::policy::ExecutionPolicy::region_filter.
        // Type: Option<String>
        pub region: Option<String>,

        // List of supported WASM runtime identifiers.
        // (e.g., "wasmtime-v18.0", "wasmedge-0.13")
        // This helps match jobs requiring specific runtime features or versions.
        // Type: Vec<String>
        pub supported_runtimes: Vec<String>,

        // Available resource types and their quantities.
        // Uses icn_types::mesh::ResourceType for keys.
        // Quantities could be simple numerical values or more complex structures.
        // Example: {"CPU": "8c", "RAM": "16GiB", "GPU_NVIDIA_A100": "1"}
        // Type: std::collections::HashMap<String, String> 
        // (Key: icn_types::mesh::ResourceType as String, Value: String representation of quantity/type)
        pub available_resources: std::collections::HashMap<String, String>,

        // List of specialized hardware features or services offered (optional).
        // (e.g., "TEE-SGX-FLC", "IPFS-Pinning-Service", "CustomAI-Accelerator-XYZ")
        // Type: Vec<String>
        pub specialized_features: Vec<String>,

        // Timestamp of when this capability set was generated or last updated (UTC, ISO 8601).
        // Type: String 
        pub timestamp: String, 

        // Optional CID pointing to a more detailed, potentially verifiable, capability attestation
        // document (e.g., a Verifiable Credential).
        // Type: Option<String> (icn_types::cid::Cid representation)
        pub attestation_cid: Option<String>,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the node_did's private key.
        // This ensures authenticity and integrity of the advertisement.
        // The exact signing mechanism (e.g., JWS, direct signature of CBOR bytes) needs
        // to be consistent across the network. For now, assume a direct signature
        // of the CBOR-encoded NodeCapability struct (excluding the signature field).
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Gossipsub.
    *   **Topic:** A well-known topic, e.g., `/icn/mesh/capabilities/v1`. (The exact topic string will be defined in Section 6: Topic Structure).

*   **Processing by Receiving Nodes:**
    *   Receiving nodes should validate the `timestamp` to ensure freshness and discard stale advertisements.
    *   The `signature` MUST be verified against the `node_did` and the rest of the payload. Invalid or unverifiable advertisements MUST be discarded and potentially penalized (e.g., Gossipsub peer scoring).
    *   Nodes can cache valid capability advertisements, keyed by `node_did` or `peer_id`, replacing older entries with newer ones (based on `timestamp`).
    *   This information is used to pre-filter nodes when a job originator is looking for executors or when an executor is deciding if it should express interest in broadly announced jobs.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` field is critical. It prevents spoofing of capabilities and ensures the advertisement hasn't been tampered with. The public key corresponding to `node_did` must be discoverable (e.g., via a DID resolver or Kad-DHT).
    *   **Replay Attacks:** The `timestamp` helps mitigate replay attacks of old capability advertisements. Nodes should define a reasonable window for accepting advertisements.
    *   **Denial of Service (DoS):** Malicious nodes could flood the capability topic with spurious advertisements. Gossipsub's peer scoring mechanisms, combined with signature verification costs and potentially rate limiting, help mitigate this.
    *   **Stale Information:** Nodes should re-advertise periodically and when capabilities change to ensure the network has reasonably up-to-date information. Consumers of this information must be aware that it's eventually consistent.
    *   **Misrepresentation:** A node might falsely advertise capabilities. While the signature confirms *who* sent it, it doesn't inherently prove the capabilities are real. This is where reputation systems (`icn-reputation`) and the outcomes of actual job executions (via `ExecutionReceipts`) become important for building trust. The optional `attestation_cid` can point to more robust, verifiable claims if needed.

### 5.2. `JobAnnouncementV1`

*   **Purpose:**
    Used by a `MeshNode` (job originator) to announce a new computational job to the network. This message makes the job discoverable by potential executor nodes. It contains the essential parameters of the job, including its definition (or a pointer to it) and the execution policy that executors must satisfy.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::JobAnnouncementV1
    pub struct JobAnnouncement {
        // Unique identifier for this job announcement instance.
        // Could be a UUID or a CID of the announcement content.
        // Type: String
        pub announcement_id: String,

        // DID of the node originating this job announcement.
        // Type: icn_types::identity::Did (String)
        pub originator_did: String,

        // The canonical MeshJobParams defining the job.
        // This struct is defined in `icn-types/src/mesh.rs`.
        // It includes wasm_cid, input_cids, execution_policy, etc.
        // For network efficiency, if MeshJobParams is large, this could
        // alternatively be a CID pointing to the full MeshJobParams stored
        // in a discoverable location (e.g., DHT, IPFS). For V1, we'll assume
        // it's embedded directly for simplicity unless it proves too large in practice.
        // Type: icn_types::mesh::MeshJobParams
        pub job_params: icn_types::mesh::MeshJobParams,

        // Timestamp of when this job announcement was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Optional: Duration for which bids will be accepted for this job (e.g., "PT30M" for 30 minutes).
        // If None, bidding duration might be determined by originator's local policy
        // or until explicitly closed.
        // Type: Option<String> (ISO 8601 duration format)
        pub bidding_duration: Option<String>,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the originator_did's private key.
        // Ensures authenticity and integrity of the job announcement.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Gossipsub.
    *   **Topic:** A well-known global job announcement topic, e.g., `/icn/mesh/jobs/announce/v1`.
        *   Alternatively, or in addition, scoped topics based on job characteristics (e.g., required `ResourceType` from `ExecutionPolicy`, region) could be used to reduce noise, e.g., `/icn/mesh/jobs/announce/region/us-east-1/v1`. This will be further detailed in Section 6.

*   **Processing by Receiving Nodes:**
    *   Verify the `signature` against the `originator_did`. Invalid announcements MUST be discarded.
    *   Validate the `timestamp` to prevent processing of excessively old announcements.
    *   Evaluate the embedded `job_params.execution_policy` against the node's own capabilities (as advertised in its `NodeCapability`) and local policies.
    *   If the node is capable and interested in bidding, it may store the `JobAnnouncement` details and prepare a `JobBidV1`.
    *   Nodes should be mindful of the `bidding_duration` if provided.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` is crucial to ensure the job announcement is from the claimed `originator_did` and hasn't been altered.
    *   **Replay Attacks:** The `timestamp` and `announcement_id` help differentiate announcements and can mitigate replay attacks if nodes track recently seen IDs.
    *   **Denial of Service (DoS) / Spam:** Malicious nodes could flood job announcement topics. Gossipsub peer scoring, signature verification costs, and potentially requiring a small stake or fee (future work via `icn-economics`) for announcements can help.
    *   **Job Validity:** This message announces a job; it doesn't guarantee the job itself (e.g., the WASM CID or input CIDs in `job_params`) is valid or non-malicious. Executors must perform their own due diligence before fetching and executing job code (see `AssignJobV1` and execution phase).
    *   **Policy Truthfulness:** The `ExecutionPolicy` within `job_params` is originator-defined. While it dictates requirements for bidders, it doesn't guarantee the originator will honor bids fairly. Reputation systems play a role here.

### 5.3. `JobBidV1`

*   **Purpose:**
    Carries an executor's formal bid for a job, including price and other relevant metadata like `executor_did` and `region`.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::JobBidV1
    pub struct JobBid {
        // Identifier of the JobAnnouncement this bid is for.
        // Type: String (Should match JobAnnouncement.announcement_id)
        pub announcement_id: String,

        // DID of the executor bidding for the job.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // PeerId of the executor node, for direct P2P communication, serialized as a Base58-encoded string.
        // Type: String
        pub executor_peer_id: String,

        // Proposed price for executing the job.
        // Type: Option<icn_types::jobs::TokenAmount>
        pub price: Option<icn_types::jobs::TokenAmount>,

        // The region this executor is operating from, if relevant to the bid.
        // Type: Option<String>
        pub region: Option<String>,

        // Timestamp of when this bid was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the executor_did's private key.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Direct messaging.
    *   **Topic:** Not applicable.

*   **Processing by Receiving Nodes:**
    *   Verify the `signature` against the `executor_did`. Invalid bids MUST be discarded.
    *   Validate the `timestamp` to prevent processing of excessively old bids.
    *   Evaluate the bid against the job's requirements (from the corresponding `JobAnnouncement`) and the executor's capabilities.
    *   If the bid is acceptable, the originator node may store it and consider it during executor selection.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` is crucial to ensure the bid is from the claimed `executor_did` and hasn't been altered.
    *   **Replay Attacks:** The `timestamp` and its relation to the `announcement_id` help differentiate bids and can mitigate replay attacks if originators track bids per announcement.
    *   **Bid Validity:** This message conveys a bid. The originator must verify that the `announcement_id` corresponds to an active job it announced and that the bid terms (`price`, `region`, implicit capabilities of `executor_did`) meet the `ExecutionPolicy`.
    *   **Unauthorized Bids:** Ensure the `executor_did` is a valid network participant (e.g., not on a blocklist, meets minimum reputation if such a pre-filter is applied before full bid evaluation).

### 5.4. `AssignJobV1`

*   **Purpose:**
    Sent by a job originator to a selected executor to formally assign them the job.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::AssignJobV1
    pub struct JobAssignment {
        // Identifier of the JobAnnouncement this assignment pertains to.
        // MUST match the announcement_id of the original JobAnnouncementV1.
        // Type: String
        pub announcement_id: String,

        // DID of the job originator sending this assignment.
        // Type: icn_types::identity::Did (String)
        pub originator_did: String,

        // DID of the selected executor being assigned the job.
        // MUST match the executor_did from the accepted JobBidV1.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // Timestamp of when this job assignment was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the originator_did's private key.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Direct messaging.
    *   **Topic:** Not applicable.

*   **Processing by Receiving Nodes (Executor):**
    *   Verify the `signature` against the `originator_did` (the public key for which should be discoverable).
    *   Validate the `timestamp` to ensure the assignment is recent and not a replay.
    *   Verify that the `announcement_id` corresponds to a job the executor bid on (and ideally, that the bid was accepted or is still considered active by the executor).
    *   Verify that the `executor_did` in the message matches the receiving node's own DID. This prevents processing an assignment intended for another executor.
    *   If all checks pass, the executor retrieves the original `JobParams` associated with the `announcement_id` (which it should have cached from the `JobAnnouncementV1`). It then prepares the execution environment and proceeds to execute the job as per these parameters.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` by `originator_did` is critical to ensure the assignment is legitimate and unaltered.
    *   **Replay Attacks:** The `timestamp` and the requirement for the `announcement_id` to correspond to an active/pending bid help mitigate replay attacks.
    *   **Mis-assignment/Targeting:** The executor MUST verify it is the intended recipient (`executor_did`) for the specified `announcement_id`. An attacker should not be able to trick an executor into running a job it didn't agree to or that was meant for someone else.
    *   **Consistency of Job Parameters:** The executor executes the job based on the `JobParams` associated with the `announcement_id` it originally processed. The `AssignJobV1` message confirms this link. If there were a discrepancy or if the `JobParams` could change post-announcement (not recommended for V1), this message might need to carry a hash or CID of the agreed-upon `JobParams` for re-verification.

### 5.5. `JobStatusUpdateV1`

*   **Purpose:**
    Sent by an executor to the job originator to provide updates on the current status of an assigned job.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::JobStatusUpdateV1
    pub struct JobStatusUpdate {
        // Identifier of the JobAnnouncement this status update pertains to.
        // MUST match the announcement_id of the original JobAnnouncementV1.
        // Type: String
        pub announcement_id: String,

        // DID of the executor providing the status update.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // Current P2P-level status of the job at the executor.
        // This string should correspond to a well-defined P2P job lifecycle state
        // (e.g., "PreparingExecution", "RunningWasm", "ExecutionFailed").
        // See RFC-0001 Section 4.2 for discussion on local vs. canonical job statuses.
        // Type: String
        pub p2p_job_status: String,

        // Optional additional details or context for the status (e.g., error message, progress info).
        // CBOR map or string.
        // Type: Option<String> // Or Option<std::collections::HashMap<String, String>> for structured details
        pub details: Option<String>,

        // Timestamp of when this status update was generated (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the executor_did's private key.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Direct messaging.
    *   **Topic:** Not applicable.

*   **Processing by Receiving Nodes (Originator):**
    *   Verify the `signature` against the `executor_did` (the public key for which should be discoverable).
    *   Validate the `timestamp` to ensure the update is recent.
    *   Confirm that the `announcement_id` corresponds to an active job that this originator assigned to the specified `executor_did`.
    *   Update its local state for the job based on the `p2p_job_status` and `details`. This might trigger UI updates or further originator-side logic.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` by `executor_did` is vital.
    *   **Replay Attacks:** `timestamp` and correlation with `announcement_id` help prevent replay of old statuses.
    *   **Unauthorized Updates:** The originator MUST verify that the status update is for a job it owns and that was assigned to this `executor_did`. An executor should not be able to send status updates for jobs it is not assigned to, or to the wrong originator.
    *   **Status Validity:** While the message itself is validated, the truthfulness of the `p2p_job_status` (e.g., an executor falsely claiming progress) is harder to verify at this P2P layer. This is managed by trust in the executor (via reputation) and ultimately confirmed by the `ExecutionReceipt`.

### 5.6. `ExecutionReceiptAvailableV1`

*   **Purpose:**
    Sent by the `executor_did` after successful job completion to announce that the signed `ExecutionReceipt` is available for retrieval. This message primarily targets the `originator_did` of the job, but can also be broadcast more widely (e.g., via Gossipsub) for general discoverability by auditors or other interested parties. It provides the necessary information (primarily the CID of the receipt) for any authorized party to fetch the full `ExecutionReceipt`.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::ExecutionReceiptAvailableV1
    pub struct ExecutionReceiptAvailable {
        // Identifier of the JobAnnouncement this receipt pertains to.
        // MUST match the announcement_id of the original JobAnnouncementV1.
        // Type: String
        pub announcement_id: String,

        // DID of the job originator for whom the job was executed.
        // Type: icn_types::identity::Did (String)
        pub originator_did: String,

        // DID of the executor who executed the job and generated the receipt.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // CID of the full, signed ExecutionReceipt.
        // The actual ExecutionReceipt object (likely stored on DHT or directly providable
        // by the executor) contains detailed execution results, proofs, and its own signature.
        // Type: icn_types::cid::Cid (String)
        pub receipt_cid: String,

        // Timestamp of when this availability announcement was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Cryptographic signature of the fields above in this ExecutionReceiptAvailable message
        // (i.e., announcement_id, originator_did, executor_did, receipt_cid, timestamp),
        // created by the executor_did's private key.
        // This signature authenticates this *announcement* message.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Primary Mechanism:** Direct messaging to the `originator_did` of the job.
    *   **Secondary Mechanism (Optional):** Gossipsub for broader dissemination.
        *   **Topic Example:** `/icn/mesh/receipts/available/v1`
        *   Nodes subscribing to this topic might include auditors, reputation services, or other components that track job completion and receipt availability across the network.
    *   The choice of transport may depend on originator preferences or network policy.

*   **Processing by Receiving Nodes:**
    *   **General Validation (Applicable to Originator and other Subscribers):**
        1.  Verify the `signature` of this `ExecutionReceiptAvailableV1` message against the `executor_did`'s public key. If invalid, discard.
        2.  Validate the `timestamp` to ensure freshness and prevent replay attacks of stale announcements.
    *   **Processing by Job Originator (`originator_did`):**
        1.  Confirm that its own DID matches the `originator_did` field in the message.
        2.  Verify that the `announcement_id` corresponds to a job it originated and that was assigned to the specified `executor_did`.
        3.  If all checks pass, the originator notes that the `ExecutionReceipt` (identified by `receipt_cid`) is available for the job.
        4.  The originator SHOULD then attempt to retrieve the full `ExecutionReceipt` from the network (e.g., via Kad-DHT `get` on the `receipt_cid`, or by directly requesting it from the `executor_did` if its address is known).
        5.  Upon successful retrieval, the originator MUST rigorously validate the `ExecutionReceipt` itself (e.g., its internal signature, consistency with job parameters, results, proofs).
    *   **Processing by Other Subscribing Nodes (e.g., Auditors):**
        1.  These nodes may use the information (`receipt_cid`, `executor_did`, `originator_did`, `announcement_id`) to log the availability of the receipt.
        2.  They MAY choose to fetch and validate the `ExecutionReceipt` based on their own criteria and policies (e.g., if auditing jobs for a specific originator or involving a particular executor).

*   **Security Considerations:**
    *   **Authenticity of Announcement:** The `signature` on this message, verified against `executor_did`, ensures that the announcement itself is authentic and has not been tampered with. It confirms that the specified `executor_did` claims a receipt with `receipt_cid` is available for the job `announcement_id`.
    *   **Integrity of Announcement:** The signature also ensures the integrity of the announced `receipt_cid` and other fields.
    *   **Replay Attacks (Announcement):** The `timestamp` and tracking of seen `announcement_id`/`receipt_cid` pairs help mitigate replaying old availability announcements.
    *   **False/Malicious Announcements:** An executor could maliciously announce a `receipt_cid` that:
        *   Does not exist.
        *   Points to a malformed or invalid `ExecutionReceipt`.
        *   Points to an `ExecutionReceipt` for a different job or signed by an unexpected party.
        Receiving nodes (especially the originator) mitigate this by *always* fetching and thoroughly validating the actual `ExecutionReceipt` referenced by `receipt_cid`. Failure to provide a valid, corresponding receipt would severely damage the executor's reputation.
    *   **Receipt Validity is Separate:** This message only *announces* availability. The actual `ExecutionReceipt` obtained via `receipt_cid` MUST be independently and rigorously validated (its own internal signature, contents, proofs, linkage to the job, etc.) according to ICN standards for `ExecutionReceipts`. This message does not provide any guarantees about the validity of the receipt itself, only about its claimed existence and location.
    *   **Spamming Announcements:** If Gossipsub is used, malicious nodes could spam the topic. Standard Gossipsub defenses (peer scoring, message validation costs) apply.

### 5.7. `JobInteractiveInputV1`

*   **Purpose:**
    Used during an active interactive job session, allowing the `originator_did` to stream input data to the `executor_did` where the job is running. This facilitates real-time interaction with the WASM module. Each piece of input is sent as a distinct message.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::JobInteractiveInputV1
    pub struct JobInteractiveInput {
        // Identifier of the JobAnnouncement this interactive input pertains to.
        // MUST match the announcement_id of the original JobAnnouncementV1.
        // Type: String
        pub announcement_id: String,

        // DID of the job originator sending the input.
        // Type: icn_types::identity::Did (String)
        pub originator_did: String,

        // DID of the executor for whom this input is intended.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // A unique identifier for this specific input message (e.g., a UUID).
        // Helps in tracking individual input chunks if needed for logging or debugging.
        // Type: String
        pub input_id: String,

        // A strictly increasing sequence number for inputs related to a specific announcement_id.
        // Starts at 0 or 1. Used to ensure ordered delivery and detect missing inputs.
        // Type: u64
        pub sequence_number: u64,

        // The actual input data payload for the job.
        // Type: Vec<u8> (Bytes of the input data)
        pub data: Vec<u8>,

        // Timestamp of when this input message was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Cryptographic signature of all fields above in this JobInteractiveInput message
        // (announcement_id, originator_did, executor_did, input_id, sequence_number, data, timestamp),
        // created by the originator_did's private key.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Direct messaging (likely over a persistent or quickly re-established stream for the duration of the interactive session).
    *   **Topic:** Not applicable.

*   **Processing by Receiving Node (Executor):**
    1.  Verify the `signature` against the `originator_did`'s public key. If invalid, discard and potentially log a security event.
    2.  Validate the `timestamp` to ensure reasonable freshness.
    3.  Verify that its own DID matches the `executor_did` field.
    4.  Confirm that the `announcement_id` corresponds to an active, interactive job that this executor is currently running for the specified `originator_did`.
    5.  Use the `sequence_number` to ensure inputs are processed in the correct order.
        *   If an input arrives out of order, the executor MAY buffer it for a short period, waiting for missing inputs.
        *   If a duplicate `sequence_number` is received, it SHOULD be discarded.
    6.  If all checks pass, the `data` payload is delivered to the appropriate WASM instance or execution environment handling the interactive job.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` by `originator_did` is crucial to ensure the input is from the legitimate job originator and the data has not been tampered with in transit.
    *   **Replay Attacks:** The combination of `timestamp` and `sequence_number` helps prevent replay attacks of old input segments. The executor must track the last valid `sequence_number`.
    *   **Out-of-Order or Missing Inputs:** The `sequence_number` allows the executor to detect and potentially handle (or report) missing or out-of-order inputs. The exact handling strategy (e.g., error, wait, request retransmission) might depend on the job's requirements.
    *   **Data Validation/Sanitization:** The executor's host environment or the WASM module itself MAY need to perform validation or sanitization on the received `data` before use, to protect against malformed or malicious inputs that could crash the job or exploit vulnerabilities.
    *   **Denial of Service (DoS):**
        *   An attacker (or faulty originator) could flood the executor with `JobInteractiveInputV1` messages. The executor SHOULD implement rate limiting per job session or per originator.
        *   Excessively large `data` payloads could also be a DoS vector. The protocol or executor policy MAY define a maximum size for `data` per message.
    *   **Unauthorized Inputs:** The executor MUST ensure that inputs are only accepted from the `originator_did` that initiated and was assigned the job specified by `announcement_id`.
    *   **Session Management:** Ensuring that inputs are only processed for currently active and correctly authenticated interactive sessions is critical.

### 5.8. `JobInteractiveOutputV1`

*   **Purpose:**
    Used during an active interactive job session, allowing the `executor_did` to stream output data back to the `originator_did` from the running job. This facilitates real-time interaction with the WASM module. Each piece of output is sent as a distinct message.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::JobInteractiveOutputV1
    pub struct JobInteractiveOutput {
        // Identifier of the JobAnnouncement this interactive output pertains to.
        // MUST match the announcement_id of the original JobAnnouncementV1.
        // Type: String
        pub announcement_id: String,

        // DID of the executor sending the output.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // DID of the job originator for whom this output is intended.
        // Type: icn_types::identity::Did (String)
        pub originator_did: String,

        // A unique identifier for this specific output message (e.g., a UUID).
        // Helps in tracking individual output chunks.
        // Type: String
        pub output_id: String,

        // A strictly increasing sequence number for outputs related to a specific announcement_id.
        // Starts at 0 or 1. Used to ensure ordered delivery and detect missing outputs.
        // Type: u64
        pub sequence_number: u64,

        // The actual output data payload from the job.
        // Type: Vec<u8> (Bytes of the output data)
        pub data: Vec<u8>,

        // Timestamp of when this output message was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Cryptographic signature of all fields above in this JobInteractiveOutput message
        // (announcement_id, executor_did, originator_did, output_id, sequence_number, data, timestamp),
        // created by the executor_did's private key.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Direct messaging (likely over a persistent or quickly re-established stream for the duration of the interactive session).
    *   **Topic:** Not applicable.

*   **Processing by Receiving Node (Originator):**
    1.  Verify the `signature` against the `executor_did`'s public key. If invalid, discard and potentially log a security event.
    2.  Validate the `timestamp` to ensure reasonable freshness.
    3.  Verify that its own DID matches the `originator_did` field.
    4.  Confirm that the `announcement_id` corresponds to an active, interactive job that this originator initiated and is currently running on the specified `executor_did`.
    5.  Use the `sequence_number` to ensure outputs are processed/displayed in the correct order.
        *   If an output arrives out of order, the originator MAY buffer it for a short period.
        *   If a duplicate `sequence_number` is received, it SHOULD be discarded.
    6.  If all checks pass, the `data` payload is made available to the end-user or application that initiated the interactive job.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` by `executor_did` is crucial to ensure the output is from the legitimate job executor and the data has not been tampered with.
    *   **Replay Attacks:** The combination of `timestamp` and `sequence_number` helps prevent replay of old output segments. The originator must track the last valid `sequence_number` for this job from this executor.
    *   **Out-of-Order or Missing Outputs:** The `sequence_number` allows the originator to detect missing or out-of-order outputs.
    *   **Data Validation/Interpretation:** The originator (or the end-user application) is responsible for interpreting the `data`. While the source is authenticated, the data itself might be malformed if the WASM module has bugs, or it could be unexpectedly large.
    *   **Denial of Service (DoS):**
        *   A malicious or faulty executor could flood the originator with `JobInteractiveOutputV1` messages. The originator SHOULD implement rate limiting per job session or per executor.
        *   Excessively large `data` payloads could also be a DoS vector. The protocol or originator policy MAY define a maximum size for `data`.
    *   **Unauthorized Outputs:** The originator MUST ensure that outputs are only accepted from the `executor_did` to whom the job specified by `announcement_id` was assigned.
    *   **Session Management:** Ensuring that outputs are only processed for currently active and correctly authenticated interactive sessions is critical.

## 6. Topic Structure

Gossipsub topics are used for broadcasting messages like capability advertisements, job announcements, and notifications of receipt availability. A consistent topic structure is essential for network organization, message filtering, and versioning.

### 6.1. General Topic Pattern

Planetary Mesh Gossipsub topics SHOULD follow this general pattern:

`/icn/mesh/<message-type>/<version>[/<scope-specific-identifiers>]`

Where:

*   **`/icn/mesh/`**: A common prefix for all ICN Planetary Mesh P2P topics, preventing collisions with other libp2p applications.
*   **`<message-type>`**: A short, descriptive name for the type of message being published on the topic. Examples:
    *   `capabilities` (for `CapabilityAdvertisementV1`)
    *   `jobs/announce` (for `JobAnnouncementV1`)
    *   `receipts/available` (for `ExecutionReceiptAvailableV1`)
*   **`<version>`**: The version of the message *schema* or *topic semantics* being used (e.g., `v1`, `v2`). This aligns with the message variant versioning (e.g., `JobAnnouncementV1` would typically use a `v1` topic version).
*   **`[/<scope-specific-identifiers>]`** (Optional): Further path segments can be appended to create more specific, scoped topics. This allows nodes to subscribe only to messages relevant to them, reducing bandwidth and processing overhead. Examples:
    *   `/region/<region-name>`: For messages scoped to a particular geographical or logical region (e.g., `/icn/mesh/jobs/announce/v1/region/us-east-1`).
    *   `/type/<job-type-identifier>`: For jobs of a specific type or requiring specific resources.

All topic segments SHOULD use lowercase alphanumeric characters and hyphens (`-`) for separators if needed (e.g., `us-east-1`).

### 6.2. Defined Topics

The following Gossipsub topics are defined for the V1 messages specified in this document:

1.  **Capability Advertisements:**
    *   **Message:** `CapabilityAdvertisementV1`
    *   **Global Topic:** `/icn/mesh/capabilities/v1`
    *   **Purpose:** For nodes to broadcast their capabilities.
    *   **Scoped Variants (Optional):**
        *   `/icn/mesh/capabilities/v1/region/<region-name>`: Nodes can subscribe to capabilities in specific regions.
        *   (Other scopes like specific `ResourceType` could be considered in future revisions if proven beneficial).

2.  **Job Announcements:**
    *   **Message:** `JobAnnouncementV1`
    *   **Global Topic:** `/icn/mesh/jobs/announce/v1`
    *   **Purpose:** For originators to announce new jobs to all potential executors.
    *   **Scoped Variants (Recommended for Executors):**
        *   `/icn/mesh/jobs/announce/v1/region/<region-name>`: Executors can subscribe to jobs matching their operating region if the `job_params.execution_policy` specifies `region_filter`.
        *   `/icn/mesh/jobs/announce/v1/type/<job-type-hash>`: If jobs can be categorized by a hash of their required `ResourceType`s or other core parameters, executors could subscribe to specific types.
        *   `/icn/mesh/jobs/announce/v1/runtime/<runtime-id>`: Executors can subscribe to jobs requiring specific WASM runtimes they support.
        *   **Note:** The exact set of recommended/supported scoped job announcement topics may evolve. For V1, the global topic and region-scoped topics are primary.

3.  **Execution Receipt Availability (Optional Broadcast):**
    *   **Message:** `ExecutionReceiptAvailableV1`
    *   **Global Topic:** `/icn/mesh/receipts/available/v1`
    *   **Purpose:** For executors to optionally announce the availability of new execution receipts to a wider audience (e.g., auditors, reputation systems) beyond just direct notification to the originator.
    *   **Scoped Variants (Optional):**
        *   `/icn/mesh/receipts/available/v1/originator/<originator-did-hash>`: If auditors want to track receipts for specific originators.
        *   `/icn/mesh/receipts/available/v1/executor/<executor-did-hash>`: If auditors want to track receipts from specific executors.

### 6.3. Topic Subscription and Publishing Strategy

*   **Publishers:**
    *   Nodes publishing `CapabilityAdvertisementV1` SHOULD publish to the global capabilities topic and MAY publish to relevant scoped topics (e.g., their specific region).
    *   Nodes publishing `JobAnnouncementV1` SHOULD publish to the global job announcement topic. They MAY also publish to relevant scoped topics if the job has specific targeting requirements (e.g., a specific region in its `ExecutionPolicy`).
    *   Nodes publishing `ExecutionReceiptAvailableV1` to Gossipsub (if not only sending directly) SHOULD publish to the global receipts topic.
*   **Subscribers:**
    *   Job originators seeking executors MAY subscribe to the global capabilities topic or filter based on cached capabilities.
    *   Executors SHOULD subscribe to the global job announcement topic and/or more specific scoped job announcement topics that match their capabilities and policies (e.g., their region, supported runtimes).
    *   Auditors or other monitoring services MAY subscribe to the global receipts available topic or more specific scoped receipt topics.

Nodes MUST be prepared to handle messages on the global topics even if they primarily focus on scoped ones, especially for critical announcements. The use of scoped topics is an optimization to reduce irrelevant message flow.

## 7. Security and Validation Summary

Security and robust validation are paramount for the integrity and reliability of the Planetary Mesh P2P protocol. This section summarizes the key security considerations and validation steps that nodes MUST implement. Detailed security points for each message type are provided in Section 5.

### 7.1. Core Security Principles

1.  **Message Authenticity and Integrity:**
    *   All `MeshProtocolMessage` variants that assert an identity or convey critical instructions (e.g., `JobAnnouncementV1`, `JobBidV1`, `AssignJobV1`, `JobStatusUpdateV1`, `CapabilityAdvertisementV1`, `ExecutionReceiptAvailableV1`, `JobInteractiveInputV1`, `JobInteractiveOutputV1`) MUST include a cryptographic `signature`.
    *   This signature is created by the private key associated with the sender's DID (e.g., `originator_did` or `executor_did`).
    *   Receiving nodes MUST verify this signature against the claimed sender's public key (retrieved via a trusted DID resolution mechanism) and the message content (excluding the signature field itself).
    *   Failure to verify the signature MUST result in the message being discarded and potentially negative scoring for the sending peer.

2.  **Authorization and Contextual Validation:**
    *   Beyond signature verification, nodes MUST perform contextual validation. For instance:
        *   An `AssignJobV1` message is only valid if it refers to a `JobAnnouncementV1` the recipient (executor) has bid on and if the assignment comes from the original job originator.
        *   A `JobStatusUpdateV1` is only valid if it comes from the `executor_did` to whom the job was assigned and is for a job owned by the recipient (originator).
        *   Interactive messages (`JobInteractiveInputV1`, `JobInteractiveOutputV1`) must be validated against an active, authenticated job session between the correct `originator_did` and `executor_did` for the given `announcement_id`.
    *   Messages received out of context or from unauthorized DIDs MUST be discarded.

3.  **Replay Attack Prevention:**
    *   All messages include a `timestamp` field.
    *   Nodes SHOULD maintain a reasonable window for accepting messages based on their timestamps to discard stale messages.
    *   For interactive streams (`JobInteractiveInputV1`, `JobInteractiveOutputV1`), the `sequence_number` is critical for detecting and rejecting replayed or out-of-order messages within a specific job session.
    *   Unique identifiers like `announcement_id`, `input_id`, and `output_id` also help in identifying and potentially discarding replayed messages if nodes track recently processed IDs.

4.  **Denial of Service (DoS) Mitigation:**
    *   **Resource Limits:** Nodes SHOULD enforce limits on message sizes (e.g., for `data` fields in interactive messages or large fields in other messages).
    *   **Rate Limiting:** Implement rate limiting for incoming messages, especially for direct messages and within interactive sessions. This can be based on `PeerId`, `DID`, or job session.
    *   **Gossipsub Defenses:** Libp2p's Gossipsub includes mechanisms like peer scoring, message validation feedback, and limits on mesh degree which help mitigate spam and DoS on broadcast topics.
    *   **Validation Costs:** The computational cost of signature verification and other validation steps inherently makes spamming expensive for attackers.

5.  **Data Validation (Content-Specific):**
    *   While the protocol ensures message authenticity, the semantic validity of data payloads (e.g., `job_params` in `JobAnnouncementV1`, `data` in interactive messages) is often application-specific.
    *   Executors SHOULD treat WASM code and input data from originators as potentially untrusted. Sandboxing (inherent in WASM runtimes) is critical.
    *   Originators SHOULD validate outputs from executors as per their application logic.

### 7.2. Mandatory Validation Steps upon Receiving any `MeshProtocolMessage`

Regardless of the message variant, receiving nodes SHOULD perform the following initial checks:

1.  **Deserialization:** The message MUST correctly deserialize from CBOR according to its claimed type and version.
2.  **Basic Schema Validation:** All required fields for the specific message variant and version MUST be present and have the correct basic types. Unknown fields in CBOR maps (for extensibility) MAY be ignored if not critical for the current version's processing.
3.  **Signature Verification:** If the message type requires a signature, it MUST be present and successfully verified against the claimed sender's DID and message content.
4.  **Timestamp Check:** The `timestamp` SHOULD be within an acceptable window (not too old, not too far in the future).

Messages failing these initial checks are considered malformed or invalid and MUST be discarded, typically without further processing. Specific contextual validation then follows, as detailed in Section 5 for each message type.

### 7.3. Trust and Reputation

While this P2P protocol provides mechanisms for message-level security, the overall trustworthiness of network participants (e.g., an executor's likelihood of successfully completing a job as bid, an originator's likelihood of fair payment) relies on higher-level systems such as `icn-reputation`. The P2P protocol aims to provide verifiable inputs (e.g., signed messages, `ExecutionReceipts`) that can feed into such reputation systems.

## 8. Future Extensions and Considerations

The V1 protocol specified in this document provides a foundational set of messages for the Planetary Mesh. As the ICN ecosystem evolves, this protocol may be extended. This section outlines potential areas for future development and considerations for protocol evolution.

*   **Advanced Job Types and Capabilities:**
    *   **Multi-Party Jobs:** Support for jobs involving more than one executor or originator (e.g., complex workflows, data sharing collaborations).
    *   **Job Modification/Cancellation:** Formal messages for requesting modifications to an assigned job (e.g., updating inputs, changing policy) or for gracefully cancelling an in-progress job, with defined responses and state transitions.
    *   **Streaming Jobs:** Enhanced support for jobs that inherently process continuous streams of input/output beyond the current interactive model, potentially with more sophisticated flow control.

*   **Enhanced Negotiation and Bidding:**
    *   **Counter-Bids/Negotiation:** A more interactive bidding process where originators and executors can negotiate terms (price, timelines, resources) through multiple message exchanges before settling on an assignment.
    *   **Auction Mechanisms:** Support for different auction types for job assignments (e.g., sealed-bid, second-price auctions).
    *   **Multi-Resource Bids:** More structured ways for executors to bid on parts of a job or offer varied service levels.

*   **Protocol Version Negotiation:**
    *   As discussed in Section 3.3, explicit version negotiation mechanisms for individual message types or sub-protocols (e.g., for interactive sessions) could be introduced. This would allow peers to advertise supported versions and select a mutually understood version for communication, rather than relying solely on sender/receiver tolerance.

*   **Network-Layer Enhancements:**
    *   **More Granular Gossipsub Topics:** Further refinement of topic scoping based on emerging job characteristics or network topology to optimize message propagation.
    *   **Alternative Transports for Specific Messages:** Evaluating other libp2p transports or custom protocols for specific use cases if performance or feature requirements dictate (e.g., for ultra-low-latency interactive streams).

*   **Data Availability and Large Data Transfer:**
    *   While CIDs are used to reference larger data (WASM, inputs, receipts), the protocol currently assumes these are retrievable via a general-purpose DHT or other means. Future extensions might include specific protocol messages to facilitate or orchestrate direct peer-to-peer transfer of large data chunks associated with jobs, potentially with progress tracking and resumability.

*   **Integration with Economic Incentives:**
    *   More direct P2P messages related to staking, micropayments, or proof-of-execution for tying into the `icn-economics` layer more deeply at the protocol level.
    *   Messages for disputing job outcomes or payments.

*   **Observability and Network Health:**
    *   Optional messages for nodes to share anonymized/aggregated network health statistics or diagnostic information, aiding in network monitoring and debugging.

*   **Formal Verification and Testing:**
    *   As the protocol matures, applying formal verification methods to parts of the protocol specification to prove properties like liveness or safety.
    *   Developing standardized test suites and conformance testing tools.

Any future extensions will require new RFCs or updates to this specification, clearly defining new message variants (e.g., `JobAnnouncementV2`), schemas, and interaction patterns, while adhering to the compatibility principles outlined in Section 3.

## 9. References

This section lists documents and standards referenced in this RFC or highly relevant to its understanding.

### 9.1. ICN Documents

*   **RFC-0001: Planetary Mesh Architecture:**
    *   *Link: (Pending Publication of RFC-0001)*
    *   Provides the overall architectural context, defines `MeshNode` components, and describes the conceptual job lifecycle that this P2P protocol facilitates.

*   **ADR-0002-dag-codec: Default Codec for Merkle-DAG structures:**
    *   *Link: (Pending Publication of ADR-0002-dag-codec)*
    *   Specifies CBOR (`dag-cbor`) as the standard codec for ICN data structures, which is adopted by this P2P protocol for message serialization.

*   **(Other relevant ICN RFCs/ADRs will be linked here upon publication, e.g., for Identity, Reputation, Economics, Execution Receipts specific formats)**

### 9.2. External Standards and Technologies

*   **Libp2p Specifications:**
    *   *Link: https://libp2p.io/specs/*
    *   The foundational P2P networking stack providing transports (TCP, QUIC), stream multiplexing, peer discovery (Kademlia DHT), and pub/sub messaging (Gossipsub) utilized by this protocol.

*   **CBOR (Concise Binary Object Representation):**
    *   *Link: RFC 8949 (https://www.rfc-editor.org/info/rfc8949)*
    *   The data serialization format used for all `MeshProtocolMessage` variants.

*   **DIDs (Decentralized Identifiers):**
    *   *Link: https://www.w3.org/TR/did-core/*
    *   The standard for decentralized identity used for `originator_did` and `executor_did` fields in protocol messages.

*   **CIDs (Content Identifiers):**
    *   *Link: https://github.com/multiformats/cid*
    *   Used for content-addressing data like WASM modules, job inputs, and `ExecutionReceipts`.

*   **ISO 8601 (Date and Time Format):**
    *   *Link: https://www.iso.org/iso-8601-date-and-time-format.html*
    *   Used for `timestamp` fields in protocol messages.

## 10. Concluding Summary

This RFC has detailed the V1 specification for the Planetary Mesh P2P Protocol. It defines the set of `MeshProtocolMessage` variants used for communication between `MeshNode`s, covering capability advertisement, job announcement and discovery, bidding, job assignment, status updates, receipt availability, and interactive job data exchange.

Key aspects covered include:

*   **Transport Mechanisms:** Utilization of libp2p's Gossipsub, direct messaging, and Kademlia DHT, with CBOR as the serialization format.
*   **Protocol Versioning:** A strategy for message variant versioning (e.g., `V1`, `V2` suffixes) and guidelines for maintaining backward and forward compatibility.
*   **Detailed Message Specifications:** For each of the eight V1 message types, this document provides its purpose, a conceptual schema, recommended transport, processing logic by receiving nodes, and specific security considerations.
*   **Topic Structure:** A proposed naming convention for Gossipsub topics to ensure clarity and enable efficient message routing and filtering.
*   **Security and Validation:** A summary of overarching security principles, including message authenticity via signatures, contextual authorization, replay attack prevention, DoS mitigation, and the importance of data validation.

This specification aims to provide a clear and robust foundation for developers building or interacting with the ICN Planetary Mesh. It is intended to be a living document, with future extensions and revisions managed through subsequent RFCs or updates, as outlined in the "Future Extensions and Considerations" section.

By adhering to this protocol, `MeshNode` implementations can interoperate effectively, contributing to a resilient, secure, and scalable decentralized computation network. 