# RFC: Planetary Mesh Architecture

## 1. Introduction & Motivation

The **Planetary Mesh** is the peer-to-peer execution substrate of the InterCooperative Network (ICN). It enables decentralized, trust-anchored compute across a global network of participating nodes, called `MeshNodes`. These nodes perform distributed job execution, participate in a cooperative bidding protocol, and anchor verifiable execution receipts into a content-addressed DAG.

The motivation behind the mesh is to eliminate the need for centralized job dispatchers or execution authorities. Instead, jobs are submitted, discovered, executed, and verified via a libp2p-based protocol that propagates work and results across the network, fostering a dynamic and open marketplace for computation. This approach ensures scalability, resilience, and autonomy, while preserving the accountability guarantees of ICN's economic and governance layers.

The mesh advances ICN's mission of **verifiable, decentralized coordination** by:

*   Allowing **any eligible node** to participate in execution via a transparent bidding and assignment protocol.
*   Producing **verifiable receipts** of computation tied to real identities and cooperative economic policies.
*   Anchoring results in a **shared DAG** to ensure auditability and replayability.
*   Integrating seamlessly with the **ICN runtime**, **identity system**, and **reputation service**.

## 2. Core Concepts

The Planetary Mesh operates on a set of core concepts that define its participants, the lifecycle of work, communication patterns, and its integration with the broader ICN ecosystem.

### 2.1. `MeshNode`

A **`MeshNode`** is an autonomous participant in the Planetary Mesh. Each `MeshNode` is identified by an ICN Decentralized Identifier (DID) (e.g., `did:key:...`) and a libp2p `PeerId` derived from its cryptographic keypair. `MeshNodes` serve dual roles:

1.  **P2P Network Participant:** All `MeshNodes` contribute to the mesh's P2P infrastructure by relaying messages, participating in discovery protocols, and maintaining connectivity.
2.  **Potential Job Executor:** `MeshNodes` can optionally advertise capabilities (e.g., available resources, supported WASM engines, geographical region) and act as executors for computational jobs.

Key responsibilities of a `MeshNode` include:
    *   Discovering available jobs announced on the mesh.
    *   Optionally, originating new jobs and announcing them.
    *   Advertising its execution capabilities.
    *   Evaluating jobs against its capabilities and local policies.
    *   Expressing interest or submitting bids for executable jobs.
    *   If assigned a job:
        *   Securely fetching the job's WASM module(s) and input data.
        *   Executing the job using its embedded ICN Runtime, which enforces the `MeshHostAbi`.
        *   Managing the job's lifecycle and providing status updates.
        *   Generating a cryptographically signed `ExecutionReceipt` upon job completion or failure.
        *   Making the `ExecutionReceipt` available to the network (e.g., via the Kademlia DHT) and announcing its availability.

### 2.2. Job Lifecycle Overview

Computational tasks, referred to as "jobs," progress through a distinct lifecycle within the Planetary Mesh:

1.  **Definition & Announcement:** A job is defined by `MeshJobParams` (from `icn-types`), detailing its WASM module(s), input data CIDs, execution policies, interactive nature, and potentially workflow stages. This definition (or a pointer to it, often as a `JobManifest`) is announced on the mesh, typically via Gossipsub, making it discoverable by potential executors.
2.  **Discovery & Interest/Bidding:** `MeshNode`s continuously monitor the network for relevant job announcements. Executor nodes evaluate these jobs against their capabilities and local policies. Interested executors can then:
    *   Express interest in a job (e.g., via a `JobInterestV1` message).
    *   Submit a formal bid (e.g., via a `JobBidV1` message), which may include a price and other terms.
3.  **Assignment:** The job originator (or a designated assignment mechanism, which could be centralized or decentralized) evaluates the received expressions of interest or bids. A suitable `MeshNode` is selected as the executor, and the job is formally assigned to it (e.g., via an `AssignJobV1` message).
4.  **Execution:** The assigned `MeshNode` proceeds with job execution:
    *   It retrieves the necessary WASM module(s) and input data (referenced by CIDs).
    *   It utilizes its local ICN Runtime instance to execute the WASM code in a sandboxed environment, enforcing resource limits and providing access to host capabilities via the `MeshHostAbi`.
    *   For interactive or multi-stage jobs, the executor manages ongoing P2P communication for status updates, user input, and intermediate outputs.
5.  **Receipting & Anchoring:** Upon job completion (successful or failed), the executor `MeshNode`:
    *   Generates a canonical, cryptographically signed `ExecutionReceipt` (as defined in `icn-mesh-receipts`). This receipt contains comprehensive details about the execution, including job parameters, executor identity, resource usage metrics, output CIDs (if any), and status.
    *   Makes the full `ExecutionReceipt` available to the network (e.g., by adding it to the Kademlia DHT).
    *   Announces the availability of the receipt (typically by broadcasting the receipt's CID and key metadata) via Gossipsub.
    *   The receipt is eventually anchored into the global ICN DAG, providing an immutable and verifiable record of the computation.

### 2.3. P2P Communication Paradigm

The Planetary Mesh relies on the **libp2p** framework for all peer-to-peer communication, leveraging several of its modules:

*   **Gossipsub:** Used for scalable, topic-based publish/subscribe messaging. Key uses include:
    *   Broadcasting `JobAnnouncementV1` messages on a global or scoped job topic.
    *   Disseminating `CapabilityAdvertisementV1` messages from executor nodes.
    *   Announcing `ExecutionReceiptAvailableV1` messages (containing receipt CIDs and metadata).
    *   Potentially for general `JobStatusUpdateV1` broadcasts if not sent directly.
*   **Kademlia (Kad-DHT):** Employed as a distributed hash table for:
    *   Storing and retrieving full `ExecutionReceipt` objects using their CIDs as keys. Nodes that generate receipts are expected to `put` them into the DHT, and interested parties can `get` them.
    *   Potentially for discovering peers with specific capabilities or services, although Gossipsub is also used for capability advertisements.
*   **Request-Response & Direct Messaging:** While Gossipsub handles broad dissemination, more targeted interactions likely use libp2p's request-response protocols or direct peer-to-peer messaging. Examples include:
    *   Submission of a specific `JobBidV1` to a job originator or a designated bid collection point.
    *   Transmission of an `AssignJobV1` message from an originator to a chosen executor.
    *   Direct relay of `JobInteractiveInputV1` and `JobInteractiveOutputV1` messages between an originator and an executor for interactive jobs.

The specific P2P messages (e.g., `JobAnnouncementV1`, `JobBidV1`, `AssignJobV1`, `ExecutionReceiptAvailableV1`) are defined as variants of `MeshProtocolMessage` within the `planetary-mesh` crate.

### 2.4. Verifiable Execution & Receipts

A cornerstone of the Planetary Mesh is the principle of verifiable execution. Every job executed on the mesh, regardless of its outcome, must result in an **`ExecutionReceipt`**. This receipt serves as a tamper-proof, auditable record of the computation.

Key characteristics:

*   **Cryptographically Signed:** The `ExecutionReceipt` is signed by the DID of the executor `MeshNode`, attesting to its authenticity and integrity.
*   **Comprehensive Metadata:** Receipts include detailed information such as the original job parameters, the executor's identity, resource consumption metrics (e.g., fuel used), timestamps for start and end of execution, CIDs of any outputs produced, and the final job status.
*   **Content-Addressable & DAG Anchored:** The canonical `ExecutionReceipt` is structured as a `DagNode` and is identified by a Content Identifier (CID). This CID, along with the receipt itself, is anchored into the ICN's global DAG, making it immutable, globally discoverable (given the CID), and permanently auditable.
*   **Basis for Trust & Accountability:** Verifiable receipts are fundamental for:
    *   **Billing and Rewards:** Confirming job completion and resource usage for economic settlement.
    *   **Reputation Systems:** Providing objective data for assessing executor reliability and performance.
    *   **Dispute Resolution:** Offering evidence in case of disagreements about job outcomes.
    *   **Governance Oversight:** Allowing federated entities or automated auditors to verify that computations adhere to network policies.

### 2.5. Economic Integration

The Planetary Mesh is designed to interoperate with the ICN's economic layer, creating a market-driven ecosystem for computation. While detailed economic mechanisms are beyond the scope of this specific architectural RFC, key integration points include:

*   **Job Parameters:** `MeshJobParams` can include economic elements such as a `max_acceptable_bid_tokens` field, indicating the originator's budget, or a `ScopedResourceToken` that pre-authorizes resource consumption.
*   **Bidding Protocol:** The process of `MeshNode`s submitting bids for jobs inherently involves pricing for computational services.
*   **Metered Execution:** The ICN Runtime meters resource usage during job execution (e.g., CPU, memory, host ABI calls, as "fuel"). This metered usage, recorded in the `ExecutionReceipt`, forms the basis for accounting and settlement within the ICN economic model.
*   **Reputation and Staking:** Future enhancements may involve staking mechanisms or economic incentives tied to executor reputation, influencing bid selection and job assignment.

These integrations ensure that computation on the mesh is not only verifiable but also economically sustainable and aligned with the cooperative principles of the ICN.

## 3. MeshNode Detailed Architecture

This section delves into the internal architecture of the `MeshNode`, primarily implemented within the `planetary-mesh/src/node.rs` crate. The `MeshNode` is the cornerstone of a node's participation in the ICN P2P network, responsible for managing job lifecycle events, facilitating P2P communication, and interacting with the ICN runtime environment.

### 3.1 Core Components

The `MeshNode` struct is a composite of several critical components:

*   **Libp2p `Swarm`:** This is the networking engine, managing all P2P interactions. It handles peer discovery (utilizing protocols like mDNS and Kademlia), message transport, and publish/subscribe messaging (via Gossipsub). The `Swarm` is configured with a custom `MeshBehaviour` that aggregates various libp2p protocols essential for ICN operations.
*   **State Management:** The `MeshNode` maintains several HashMaps to track the state of various entities:
    *   `jobs_state`: Monitors jobs originated by the current node.
    *   `bids_state`: Stores bids received from executor nodes for jobs originated locally.
    *   `assigned_jobs_state`: Tracks jobs that have been assigned to the current node for execution.
    *   `job_receipts_state`: Caches execution receipts for completed jobs.
    *   `local_capabilities`: Defines and stores the execution capabilities of the node itself (e.g., supported WASM runtimes, available resource types, specific hardware features).
*   **`local_runtime_context`:** An instance of `RuntimeContext` (provided by the `icn-runtime` crate). This context is crucial for job execution, offering access to shared services like the `DagStore` (for data anchoring) and the `EconomicsHandle` (for resource management and accounting). It effectively provides the sandboxed environment where WASM payloads are executed.
*   **`internal_action_tx` (MPSC Channel Sender):** An asynchronous multi-producer, single-consumer channel sender. This is used to decouple the libp2p `Swarm` event loop (which handles network events) from the processing of internal node actions. Complex or potentially blocking tasks triggered by network events are sent over this channel to be handled by a separate task, ensuring the network event loop remains responsive.
*   **Cryptographic Identity:** Each `MeshNode` is equipped with a unique cryptographic identity, typically a Decentralized Identifier (DID) and associated KeyPair (e.g., Ed25519). This identity is used for signing outgoing messages (like bids and receipts) and verifying the authenticity and integrity of messages received from other peers.

### 3.2 Operational Flows

The `MeshNode` orchestrates a variety of interconnected operational flows essential for the functioning of the decentralized compute mesh:

*   **Capability Advertisement:**
    *   Nodes periodically broadcast their `NodeCapability` (e.g., available CPU, RAM, supported WASM instruction sets, special hardware) to the network using `CapabilityAdvertisementV1` messages over Gossipsub.
    *   This allows job originators to discover nodes that meet the resource and technical requirements of their jobs.

*   **Job Lifecycle Management (as Job Originator):**
    1.  **Job Submission:** A user or process submits a `JobRequest` (containing `MeshJobParams` and an `ExecutionPolicy`) to their local `MeshNode`.
    2.  **Job Announcement:** The `MeshNode` announces the job to the network via a `JobAnnouncementV1` Gossipsub message. This message includes the Job ID and its `ExecutionPolicy` to allow potential executors to pre-filter.
    3.  **Bid Collection:** Interested and capable executor nodes respond with `JobBidV1` messages. These bids include the executor's proposed terms (e.g., price) and relevant metadata (like their region, if included in the protocol). The originator node collects these bids, potentially applying initial filtering based on the `ExecutionPolicy` (e.g., `max_price`, `min_reputation`).
    4.  **Executor Selection:** After a defined bidding period or once a sufficient number of bids are received, the originator's `MeshNode` executes its selection logic (e.g., in `select_executor_for_originated_jobs`). This logic evaluates bids against the `ExecutionPolicy` (considering price, reputation, region constraints) and other criteria to choose the most suitable executor.
    5.  **Job Assignment:** The chosen executor is formally assigned the job via an `AssignJobV1` message, typically sent directly or reliably over the network.
    6.  **Status Tracking:** The originator node listens for `JobStatusUpdateV1` messages from the executor to monitor the job's progress.
    7.  **Receipt Handling:** Upon job completion, the executor signals receipt availability (e.g., `ExecutionReceiptAvailableV1`). The originator then fetches the `ExecutionReceipt` (e.g., via Kademlia GET or direct request), verifies its signature and content against the original job parameters, and may anchor it or its CID to a shared DAG for provenance.

*   **Job Lifecycle Management (as Job Executor):**
    1.  **Job Discovery:** The `MeshNode` listens for `JobAnnouncementV1` messages on relevant Gossipsub topics.
    2.  **Bidding Decision:** For each announced job, the node evaluates its `ExecutionPolicy` against its own capabilities, current load, and internal policies. If it decides to bid, it constructs and sends a `JobBidV1` message to the originator.
    3.  **Assignment Handling:** If the node receives an `AssignJobV1` message for a job it bid on, it prepares the execution environment.
    4.  **Job Execution:**
        *   The WASM payload for the job is retrieved (e.g., from IPFS/IPLD based on a CID).
        *   The `icn_runtime::execute_mesh_job` function is invoked, utilizing the `local_runtime_context` and the `CoVm` (Cooperative Virtual Machine) to run the WASM binary in a sandboxed and metered environment.
        *   During execution, the WASM module can interact with the host system via the `MeshHostAbi` (e.g., to anchor data to the DAG, send interactive messages, or report resource usage).
        *   The node sends periodic `JobStatusUpdateV1` messages to the job originator.
    5.  **Receipt Generation & Announcement:**
        *   Upon completion (successful or otherwise), an `ExecutionReceipt` is generated. This receipt details the execution outcome, resource consumption metrics, any resulting CIDs of generated data, and is signed by the executor node.
        *   The executor typically anchors this receipt to a shared DAG.
        *   The availability of the receipt is announced to the job originator (e.g., via `ExecutionReceiptAvailableV1`).

*   **Kademlia Distributed Hash Table (DHT) Usage:**
    *   Primarily used for decentralized peer discovery.
    *   Also employed for content discovery and retrieval, allowing nodes to publish and resolve CIDs for data objects such as job payloads, WASM modules, and `ExecutionReceipts`.

*   **Interactive Job Support:**
    *   For jobs designated as `is_interactive`, the `MeshNode` and its underlying P2P protocol (`MeshProtocolMessage`) facilitate the exchange of data streams between the originator and executor during active job execution. This enables use cases requiring real-time feedback or control.

This architecture enables the `MeshNode` to function autonomously within the ICN, capable of both originating computational tasks and executing tasks on behalf of others, all while maintaining secure, verifiable, and resource-aware operations.