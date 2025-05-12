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

        // PeerId of the node, for direct network addressing.
        // Type: libp2p_identity::PeerId (String representation or bytes)
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
        // DID of the executor bidding for the job.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // Price for the job.
        // Type: icn_types::mesh::Price (Decimal)
        pub price: icn_types::mesh::Price,

        // Region of the job.
        // Type: icn_types::jobs::policy::ExecutionPolicy::region_filter (String)
        pub region: String,

        // Timestamp of when this bid was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the executor_did's private key.
        // Ensures authenticity and integrity of the bid.
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
    *   Evaluate the bid against the job's requirements and the executor's capabilities.
    *   If the bid is acceptable, the node may proceed to the next stage of job assignment.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` is crucial to ensure the bid is from the claimed `executor_did` and hasn't been altered.
    *   **Replay Attacks:** The `timestamp` helps differentiate bids and can mitigate replay attacks if nodes track recently seen IDs.
    *   **Job Validity:** This message announces a bid; it doesn't guarantee the bid itself (e.g., the price or region) is valid or non-malicious. The bid must be evaluated against the job's requirements and the executor's capabilities.

### 5.4. `AssignJobV1`

*   **Purpose:**
    Sent by a job originator to a selected executor to formally assign them the job.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::AssignJobV1
    pub struct JobAssignment {
        // DID of the job originator.
        // Type: icn_types::identity::Did (String)
        pub originator_did: String,

        // DID of the selected executor.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // Unique identifier for this job assignment instance.
        // Could be a UUID or a CID of the assignment content.
        // Type: String
        pub assignment_id: String,

        // Timestamp of when this job assignment was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the originator_did's private key.
        // Ensures authenticity and integrity of the job assignment.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Direct messaging.
    *   **Topic:** Not applicable.

*   **Processing by Receiving Nodes:**
    *   Verify the `signature` against the `originator_did`. Invalid assignments MUST be discarded.
    *   Validate the `timestamp` to prevent processing of excessively old assignments.
    *   Ensure the assignment is valid and authorized.
    *   If the assignment is valid and authorized, the node may proceed to the next stage of job execution.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` is crucial to ensure the assignment is from the claimed `originator_did` and hasn't been altered.
    *   **Replay Attacks:** The `timestamp` helps differentiate assignments and can mitigate replay attacks if nodes track recently seen IDs.
    *   **Job Validity:** This message assigns a job; it doesn't guarantee the job itself (e.g., the job parameters or execution policy) is valid or non-malicious. The assignment must be valid and authorized.

### 5.5. `JobStatusUpdateV1`

*   **Purpose:**
    Sent by an executor to the job originator to provide updates on the current status of an assigned job.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::JobStatusUpdateV1
    pub struct JobStatusUpdate {
        // DID of the executor.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // Unique identifier for this job status update instance.
        // Could be a UUID or a CID of the update content.
        // Type: String
        pub update_id: String,

        // Status of the job.
        // Type: icn_types::mesh::JobStatus (String)
        pub status: String,

        // Timestamp of when this job status update was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the executor_did's private key.
        // Ensures authenticity and integrity of the job status update.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Direct messaging.
    *   **Topic:** Not applicable.

*   **Processing by Receiving Nodes:**
    *   Verify the `signature` against the `executor_did`. Invalid updates MUST be discarded.
    *   Validate the `timestamp` to prevent processing of excessively old updates.
    *   Ensure the update is valid and authorized.
    *   If the update is valid and authorized, the node may proceed to update the job status.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` is crucial to ensure the update is from the claimed `executor_did` and hasn't been altered.
    *   **Replay Attacks:** The `timestamp` helps differentiate updates and can mitigate replay attacks if nodes track recently seen IDs.
    *   **Job Validity:** This message updates a job status; it doesn't guarantee the status itself is valid or non-malicious. The update must be valid and authorized.

### 5.6. `ExecutionReceiptAvailableV1`

*   **Purpose:**
    Sent by an executor after job completion to announce that the signed `ExecutionReceipt` (identified by its CID) is available.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::ExecutionReceiptAvailableV1
    pub struct ExecutionReceiptAvailable {
        // CID of the signed ExecutionReceipt.
        // Type: icn_types::cid::Cid (String)
        pub cid: String,

        // Timestamp of when this ExecutionReceipt was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the executor's private key.
        // This ensures authenticity and integrity of the ExecutionReceipt.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Direct messaging.
    *   **Topic:** Not applicable.

*   **Processing by Receiving Nodes:**
    *   Verify the `signature` against the executor's private key. Invalid receipts MUST be discarded.
    *   Validate the `timestamp` to prevent processing of excessively old receipts.
    *   Ensure the receipt is valid and authorized.
    *   If the receipt is valid and authorized, the node may proceed to store the receipt.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` is crucial to ensure the receipt is from the claimed executor and hasn't been tampered with.
    *   **Replay Attacks:** The `timestamp` helps differentiate receipts and can mitigate replay attacks if nodes track recently seen CIDs.
    *   **Job Validity:** This message announces a receipt; it doesn't guarantee the receipt itself is valid or non-malicious. The receipt must be valid and authorized.

### 5.7. `JobInteractiveInputV1`

*   **Purpose:**
    Used in interactive jobs to send data from the originator to the executor during active job execution.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::JobInteractiveInputV1
    pub struct JobInteractiveInput {
        // DID of the job originator.
        // Type: icn_types::identity::Did (String)
        pub originator_did: String,

        // DID of the executor.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // Unique identifier for this job interactive input instance.
        // Could be a UUID or a CID of the input content.
        // Type: String
        pub input_id: String,

        // Timestamp of when this job interactive input was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Data for the job.
        // Type: Vec<u8> (Bytes of the input data)
        pub data: Vec<u8>,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the originator_did's private key.
        // Ensures authenticity and integrity of the job interactive input.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Direct messaging.
    *   **Topic:** Not applicable.

*   **Processing by Receiving Nodes:**
    *   Verify the `signature` against the `originator_did`. Invalid inputs MUST be discarded.
    *   Validate the `timestamp` to prevent processing of excessively old inputs.
    *   Ensure the input is valid and authorized.
    *   If the input is valid and authorized, the node may proceed to pass the input to the job.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` is crucial to ensure the input is from the claimed `originator_did` and hasn't been tampered with.
    *   **Replay Attacks:** The `timestamp` helps differentiate inputs and can mitigate replay attacks if nodes track recently seen IDs.
    *   **Job Validity:** This message sends job input data; it doesn't guarantee the data itself is valid or non-malicious. The input must be valid and authorized.

### 5.8. `JobInteractiveOutputV1`

*   **Purpose:**
    Used in interactive jobs to send data from the executor back to the originator during active job execution.

*   **Schema (Conceptual Rust Struct):**
    ```rust
    // Contained within MeshProtocolMessage::JobInteractiveOutputV1
    pub struct JobInteractiveOutput {
        // DID of the executor.
        // Type: icn_types::identity::Did (String)
        pub executor_did: String,

        // DID of the job originator.
        // Type: icn_types::identity::Did (String)
        pub originator_did: String,

        // Unique identifier for this job interactive output instance.
        // Could be a UUID or a CID of the output content.
        // Type: String
        pub output_id: String,

        // Timestamp of when this job interactive output was created (UTC, ISO 8601).
        // Type: String
        pub timestamp: String,

        // Data for the job.
        // Type: Vec<u8> (Bytes of the output data)
        pub data: Vec<u8>,

        // Cryptographic signature of the fields above (excluding the signature itself),
        // created by the executor_did's private key.
        // Ensures authenticity and integrity of the job interactive output.
        // Type: Vec<u8> (Bytes of the signature)
        pub signature: Vec<u8>,
    }
    ```

*   **Transport & Topic:**
    *   **Mechanism:** Direct messaging.
    *   **Topic:** Not applicable.

*   **Processing by Receiving Nodes:**
    *   Verify the `signature` against the `executor_did`. Invalid outputs MUST be discarded.
    *   Validate the `timestamp` to prevent processing of excessively old outputs.
    *   Ensure the output is valid and authorized.
    *   If the output is valid and authorized, the node may proceed to pass the output to the job.

*   **Security Considerations:**
    *   **Authenticity & Integrity:** The `signature` is crucial to ensure the output is from the claimed `executor_did` and hasn't been tampered with.
    *   **Replay Attacks:** The `timestamp` helps differentiate outputs and can mitigate replay attacks if nodes track recently seen IDs.
    *   **Job Validity:** This message sends job output data; it doesn't guarantee the data itself is valid or non-malicious. The output must be valid and authorized. 