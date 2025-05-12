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

## 4. Data Models & State

The Planetary Mesh, while deeply integrated with the ICN, maintains its own set Aof local data models primarily for P2P communication efficiency and internal state tracking within a `MeshNode`. This section compares these local types with their canonical counterparts in `icn-types`, discusses their specific roles, and identifies areas of overlap or divergence. Understanding these distinctions is crucial for developers working on either the mesh layer or the core ICN services.

### 4.1. `Bid` Data Model

*   **`planetary-mesh::protocol::Bid` (Local P2P Type):**
    *   **Definition:** A simplified structure used in the `JobBidV1` P2P message. As of the latest review, it primarily contains:
        *   `job_id: String`
        *   `job_originator: PeerId` (or similar identifier for the originator node)
        *   `executor_did: Did` (DID of the bidding executor node)
        *   `price: Option<TokenAmount>` (or a simple numerical type)
        *   `execution_node_id: PeerId` (PeerId of the bidding executor)
        *   `region: Option<String>` (Added to support region-based filtering)
        *   Other potential P2P-specific metadata.
    *   **Purpose:** Designed for lightweight network transmission during the job bidding phase. It carries the essential information an originator needs to evaluate a bid from an executor.

*   **`icn_types::jobs::Bid` (Canonical Type):**
    *   **Definition:** A more comprehensive structure intended for on-chain records, governance processes, or detailed off-chain agreements. It might include:
        *   `job_id: JobId` (Typedef for Job ID)
        *   `bidder_did: Did` (DID of the bidder)
        *   `terms: BidTerms` (A nested struct possibly containing price, execution timeline, resource guarantees, etc.)
        *   `collateral: Option<TokenAmount>`
        *   `signature: VerifiableCredential` (or similar proof of bid authenticity)
        *   `node_metadata_cid: Option<Cid>` (Link to detailed, verifiable metadata about the node's capabilities and attestations)
    *   **Purpose:** Serves as a formal, verifiable record of a bid. It's designed for scenarios requiring stronger cryptographic assurance and richer detail than a transient network message.

*   **Comparison & Reconciliation:**
    *   **Overlap:** Both represent an offer to execute a job for certain terms. Fields like `job_id`, `executor_did`/`bidder_did`, and `price` are common.
    *   **Divergence:**
        *   The P2P `Bid` is significantly leaner, omitting complex structures like detailed `BidTerms` or cryptographic signatures directly within the bid message itself (though the `JobBidV1` message as a whole might be signed).
        *   The canonical `Bid` is built for verifiability and comprehensive detail, suitable for storage and auditable processes. The `region` field recently added to the P2P bid is not present in the canonical `icn-types::jobs::Bid` but is handled by the `ExecutionPolicy` which the canonical bid would be evaluated against.
    *   **Rationale for Divergence:** The P2P `Bid` prioritizes network efficiency for rapid bid dissemination and collection. The canonical `Bid` prioritizes completeness and verifiability for dispute resolution, payment, and reputation.
    *   **Unification/Clarification:**
        *   It's appropriate for these two types to remain distinct. The P2P `Bid` acts as a "bid announcement" or "expression of interest with basic terms."
        *   A selected P2P `Bid` might then be formalized into a canonical `icn_types::jobs::Bid` by the originator and/or executor if required for on-chain settlement or more formal agreements, potentially by referencing the P2P bid details and adding necessary cryptographic material or linking to richer metadata.

### 4.2. `JobStatus` Data Model

*   **`planetary-mesh::JobStatus` (Local Enum):**
    *   **Definition:** A detailed, state-machine-oriented enum used internally by `MeshNode` to track the fine-grained status of jobs it is involved with (either as originator or executor). Examples from `enhanced-mesh-job-system.md` and observed behavior suggest states like:
        *   `PendingAnnouncement`, `Announced`, `BiddingOpen`, `BiddingClosed`, `AwaitingAssignmentConfirmation`, `Assigned`, `PreparingExecution`, `Running`, `AwaitingInputs`, `ProducingOutputs`, `Completed`, `Failed`, `ReceiptGenerated`, `ReceiptAnchored`.
    *   **Purpose:** Manages the complex lifecycle of a job within the P2P network and execution environment, supporting detailed internal logic and potentially more granular updates to a UI or monitoring system.

*   **`icn_types::jobs::StandardJobStatus` (Canonical Enum):**
    *   **Definition:** A simpler, standardized enum representing the high-level status of a job, suitable for broader system understanding and interoperability. Typically includes states like:
        *   `Pending`
        *   `Running`
        *   `CompletedSuccessfully`
        *   `Failed`
        *   `Cancelled`
    *   **Purpose:** Provides a common, interoperable status representation for use in `ExecutionReceipts`, governance systems, and external job tracking services.

*   **Comparison & Reconciliation:**
    *   **Overlap:** Both track the progression of a job. The canonical statuses are typically subset abstractions of the local statuses (e.g., `PreparingExecution`, `Running`, `AwaitingInputs` in local `JobStatus` might all map to `Running` in `StandardJobStatus`).
    *   **Divergence:** The local `JobStatus` is much more granular, reflecting internal P2P protocol states and execution sub-phases. The canonical `StandardJobStatus` is a higher-level abstraction.
    *   **Rationale for Divergence:** The local `JobStatus` is necessary for the `MeshNode` to manage its internal operations and interactions correctly. The `StandardJobStatus` is for broader, less detailed communication and recording.
    *   **Unification/Clarification:**
        *   These two should remain distinct but have a clear mapping.
        *   The `MeshNode` should be responsible for translating its internal, detailed `JobStatus` into the appropriate canonical `StandardJobStatus` when:
            *   Generating an `ExecutionReceipt`.
            *   Reporting status to external systems that expect the canonical form.
        *   The `JobStatusUpdateV1` P2P message could potentially carry either the fine-grained local status (for detailed tracking between involved parties) or the canonical status, depending on the context and recipient. The `enhanced-mesh-job-system.md` implies that the more detailed status is communicated.

### 4.3. `JobManifest` / `JobRequest`

*   **`planetary-mesh::JobManifest` (Local Concept/Struct):**
    *   **Definition:** Often a local representation or wrapper around the core job parameters. It might be the structure directly serialized for the `JobAnnouncementV1` P2P message. It would contain the necessary details for a potential executor to understand the job, such as CIDs for WASM binaries, input data, and the `ExecutionPolicy`.
    *   **Purpose:** Efficient network transmission of job details.

*   **`icn_types::jobs::JobRequest` and `icn_types::mesh::MeshJobParams` (Canonical Types):**
    *   **Definition:**
        *   `JobRequest`: A higher-level structure that might include user identity, submission timestamp, and references `MeshJobParams`.
        *   `MeshJobParams`: The detailed specification of the computational job, including `wasm_cid`, `input_cids`, `timeout_seconds`, `max_fuel`, `workflow_type`, `stages`, `is_interactive`, and the crucial `execution_policy: Option<ExecutionPolicy>`.
    *   **Purpose:** Provides a complete, canonical definition of a job suitable for origination, policy enforcement, and inclusion in receipts.

*   **Comparison & Reconciliation:**
    *   **Overlap:** High degree of overlap. The P2P `JobManifest` (or the payload of `JobAnnouncementV1`) essentially carries the content of `MeshJobParams`.
    *   **Divergence:** `JobRequest` might contain additional metadata not strictly needed for the initial P2P announcement but relevant for the originating system (e.g., `icn-mesh-jobs` service). The P2P message might omit some fields if they can be inferred or are too large for an initial broadcast.
    *   **Unification/Clarification:**
        *   The P2P `JobAnnouncementV1` message should directly serialize `icn_types::mesh::MeshJobParams` or a very close subset. This ensures that executors receive the canonical job definition.
        *   If a separate `JobManifest` struct exists in `planetary-mesh`, it should be a direct pass-through or a minimal adaptation of `MeshJobParams` for P2P transport. Avoid drift between these definitions.

### 4.4. `NodeCapability`

*   **`planetary-mesh::protocol::NodeCapability` (Local P2P Type):**
    *   **Definition:** A structure advertised by executor nodes (e.g., via `CapabilityAdvertisementV1`) to declare their resources and supported features (CPU, RAM, supported WASM runtimes, specific hardware, region).
    *   **Purpose:** Allows job originators to discover suitable executors and allows executors to pre-filter jobs they are interested in.

*   **`icn-types` Counterpart:**
    *   There isn't a direct, one-to-one canonical "NodeCapability" type in `icn-types` that is advertised independently.
    *   However, `icn_types::jobs::policy::ExecutionPolicy` contains fields like `region_filter`, `min_reputation`, and implies requirements for resources (though not explicitly listing them as capabilities).
    *   The concept of node capabilities is implicitly present in the `ExecutionPolicy` which defines *requirements* for a node, and also in `icn_types::mesh::ResourceType` which is used in economics.

*   **Comparison & Reconciliation:**
    *   **Divergence:** `planetary-mesh` has an explicit capability advertisement message. `icn-types` focuses more on the *requirements* specified by a job's `ExecutionPolicy`.
    *   **Rationale for Divergence:** P2P discovery benefits from explicit capability advertisements to reduce unnecessary communication. The `ExecutionPolicy` serves as the contract from the job's perspective.
    *   **Unification/Clarification:**
        *   The fields within `planetary-mesh::protocol::NodeCapability` should ideally align with the types of constraints that can be specified in an `ExecutionPolicy` (e.g., if `ExecutionPolicy` can filter by `region`, `NodeCapability` should advertise `region`).
        *   Consider deriving `NodeCapability` fields from or making them directly compatible with `ResourceType` definitions and `ExecutionPolicy` constraints where applicable.
        *   It may be beneficial to introduce a canonical `NodeAttestation` or `NodeProfile` type in `icn-types` in the future, which could be a verifiable credential containing detailed capabilities, and the P2P `NodeCapability` message could be a summary or a pointer to this.

### 4.5. `JobExecutionReceipt` (Local) vs. `ExecutionReceipt` (Canonical)

*   **`planetary-mesh::JobExecutionReceipt` (Local Struct, if distinct):**
    *   **Definition:** Potentially a local struct within `planetary-mesh/src/lib.rs` or `node.rs` used to assemble receipt information before it's formalized into the canonical `icn-mesh-receipts` version.
    *   **Purpose:** Internal state representation during receipt generation.

*   **`icn_mesh_receipts::ExecutionReceipt` (Canonical Type):**
    *   **Definition:** The formal, cryptographically signed, and DAG-anchorable receipt. Contains comprehensive details: original `MeshJobParams` (or a CID to them), executor DID, resource usage (`fuel_used`), output CIDs, `StandardJobStatus`, timestamps, and signature.
    *   **Purpose:** Provides the verifiable proof of computation for the ICN.

*   **Comparison & Reconciliation:**
    *   **Overlap:** Should be nearly identical in terms of content. The local version is a precursor to the canonical one.
    *   **Divergence:** The local version might exist temporarily without a signature or before all DAG CIDs are finalized. The canonical version is the complete, signed, and final artifact.
    *   **Unification/Clarification:**
        *   The `planetary-mesh` node's primary role is to *generate* the canonical `icn_mesh_receipts::ExecutionReceipt`.
        *   Any internal "local receipt" struct should simply be the `icn_mesh_receipts::ExecutionReceipt` struct in a mutable state during its construction (e.g., fields being filled in, then signed).
        *   The `ExecutionReceiptAvailableV1` P2P message should announce the availability of the canonical, signed `icn_mesh_receipts::ExecutionReceipt` (typically by its CID).

### 4.6. Context of Type Usage

*   **Networking (P2P - `planetary-mesh`):**
    *   Uses lean, often simplified versions of data models optimized for minimal bandwidth and fast serialization/deserialization (e.g., `protocol::Bid`, `JobAnnouncementV1` payload, `NodeCapability`).
    *   Focus is on discovery, negotiation, and status updates.
    *   Types: `planetary-mesh::protocol::MeshProtocolMessage` and its variants.

*   **Runtime Execution (`icn-runtime`, `icn-core-vm`):**
    *   Interacts with `icn_types::mesh::MeshJobParams` to understand what to execute.
    *   Uses `icn_types::host_abi` for WASM module interactions.
    *   Produces data that feeds into the canonical `icn_mesh_receipts::ExecutionReceipt` (e.g., `fuel_used`, output CIDs).

*   **Persistence & Anchoring (`DagStore`, `icn-mesh-receipts`):**
    *   Primarily deals with canonical, often IPLD-encoded types.
    *   `icn_mesh_receipts::ExecutionReceipt` is a key persisted type.
    *   Job definitions (`MeshJobParams`) and input/output data (as CIDs) are also relevant.

*   **Canonical State & Governance (`icn-types`, `icn-economics`, `icn-reputation`):**
    *   Relies on `icn-types` for foundational definitions (e.g., `TokenAmount`, `Did`, `VerifiableCredential`, `StandardJobStatus`, `ExecutionPolicy`).
    *   `icn_mesh_receipts::ExecutionReceipt` is critical for economic settlement and reputation updates.
    *   Canonical `icn_types::jobs::Bid` might be used if bids are recorded on-chain or in a formal registry.

*   **Local Node State (`planetary-mesh::node`):**
    *   Maintains HashMaps (`jobs_state`, `bids_state`, etc.) using a mix of identifiers (Job IDs, PeerIDs) and potentially local versions of the P2P types or canonical types where appropriate.
    *   The detailed local `JobStatus` enum is crucial here.

**Conclusion for Section 4:** A clear distinction between P2P-optimized data models and canonical ICN types is generally beneficial. The `planetary-mesh` types serve the immediate needs of network communication and internal node state machines, while `icn-types` provide the stable, verifiable, and comprehensive definitions for the broader ICN ecosystem. The key is to ensure clear mapping and translation mechanisms where these different views of the same underlying concepts interact, particularly during job definition, receipt generation, and status reporting. Future work should focus on minimizing unnecessary divergence and ensuring that P2P types can be easily and losslessly (where required) converted to or from their canonical counterparts.

## 5. P2P Protocol Summary

The Planetary Mesh relies on a set of P2P messages, primarily defined within `planetary-mesh/src/protocol.rs` as variants of the `MeshProtocolMessage` enum. These messages are exchanged over libp2p, utilizing various transport protocols like Gossipsub for broadcast, Kademlia for DHT operations, and potentially direct messaging for unicast interactions. This section provides a high-level overview of these messages and their roles.

A more detailed specification, including wire formats and precise message sequencing, will be covered in a forthcoming document: "RFC: Mesh P2P Protocol Specification."

### 5.1. `MeshProtocolMessage` Variants and Roles

The `MeshProtocolMessage` enum encapsulates the different types of messages exchanged between `MeshNode`s. Key variants include:

*   **`CapabilityAdvertisementV1`:**
    *   **Role:** Allows executor nodes to broadcast their capabilities (e.g., resources, supported WASM versions, region) to the network.
    *   **Transport:** Typically disseminated via Gossipsub on a well-known topic for capabilities.
    *   **Purpose:** Enables job originators to discover suitable executors and allows executors to signal their availability and specialties.

*   **`JobAnnouncementV1`:**
    *   **Role:** Used by job originators to announce new jobs to the network. Contains the job's `MeshJobParams` (or a CID pointing to them), including its `ExecutionPolicy`.
    *   **Transport:** Broadcast via Gossipsub, potentially on a global "all jobs" topic or more specific, scoped topics (e.g., based on job type or required resources).
    *   **Purpose:** Makes jobs discoverable by potential executor nodes.

*   **`JobInterestV1` (Optional/Alternative to Direct Bid):**
    *   **Role:** Allows potential executors to express non-binding interest in an announced job without immediately submitting a full bid.
    *   **Transport:** Could be sent via Gossipsub to a job-specific topic or directly to the originator.
    *   **Purpose:** Helps originators gauge initial interest and can be a lighter-weight first step before full bidding.

*   **`JobBidV1`:**
    *   **Role:** Carries an executor's formal bid for a job, including their proposed price and relevant metadata (like their `executor_did` and `region`). The payload is typically the local `planetary-mesh::protocol::Bid` struct.
    *   **Transport:** Usually sent directly to the job originator or to a designated bid collection point. In some models, bids might be gossiped on a job-specific topic if public bidding is desired.
    *   **Purpose:** Allows executors to compete for job execution based on the terms defined in the `ExecutionPolicy` and their own pricing.

*   **`AssignJobV1`:**
    *   **Role:** Sent by a job originator to a selected executor to formally assign them the job. Includes the Job ID and any final parameters or confirmations.
    *   **Transport:** Sent directly to the chosen executor node.
    *   **Purpose:** Confirms executor selection and initiates the job execution phase.

*   **`JobStatusUpdateV1`:**
    *   **Role:** Sent by an executor to the job originator to provide updates on the current status of an assigned job. May carry the detailed local `planetary-mesh::JobStatus`.
    *   **Transport:** Typically sent directly to the job originator.
    *   **Purpose:** Keeps the originator informed of the job's progress through its lifecycle (e.g., `PreparingExecution`, `Running`, `Completed`).

*   **`ExecutionReceiptAvailableV1`:**
    *   **Role:** Sent by an executor node after a job is completed and an `ExecutionReceipt` has been generated and typically anchored (e.g., to the Kademlia DHT or IPFS). Contains the CID of the `ExecutionReceipt` and key metadata.
    *   **Transport:** Broadcast via Gossipsub on a relevant topic (e.g., a job-specific topic or a general receipts topic), and/or sent directly to the job originator.
    *   **Purpose:** Informs the originator and other interested parties that the job's verifiable outcome is available.

*   **`JobInteractiveInputV1` / `JobInteractiveOutputV1`:**
    *   **Role:** Facilitates the exchange of data streams for interactive jobs. `JobInteractiveInputV1` sends data from the originator to the executor, and `JobInteractiveOutputV1` sends data from the executor back to the originator during active job execution.
    *   **Transport:** Sent directly between the originator and the executor.
    *   **Purpose:** Enables real-time interaction and data streaming for jobs that require it (e.g., those with `is_interactive: true` in `MeshJobParams`).

### 5.2. Gossipsub Topic Structure

Gossipsub is the primary mechanism for broadcast and multicast communication. The topic structure is designed to balance discoverability with network efficiency:

*   **Global/Well-Known Topics:**
    *   Example: `/icn/mesh/capabilities/v1` - For `CapabilityAdvertisementV1` messages.
    *   Example: `/icn/mesh/jobs/announce/v1` - A general topic for all `JobAnnouncementV1` messages.
    *   **Purpose:** Broad dissemination for initial discovery of nodes and jobs. Nodes subscribe to these to get a wide view of network activity.

*   **Scoped/Job-Specific Topics (Potentially):**
    *   Example: `/icn/mesh/job/{job_id}/bids/v1` - If bids were to be public on a specific job.
    *   Example: `/icn/mesh/job/{job_id}/status/v1` - For status updates related to a particular job, if not sent directly.
    *   **Purpose:** More targeted communication related to a specific job instance, reducing noise for nodes not involved with that particular job. The design of these topics needs to consider cardinality and churn.

The exact topic strings and scoping strategies are subject to further refinement in the dedicated P2P protocol specification.

### 5.3. Complementary Use of Kademlia and Direct Messaging

While Gossipsub handles broad dissemination, Kademlia (Kad-DHT) and direct libp2p messaging play crucial complementary roles:

*   **Kademlia (Kad-DHT):**
    *   **Peer Discovery:** Helps nodes find each other in the network, bootstrapping connections for Gossipsub and direct messaging.
    *   **Content Discovery & Retrieval:** Used to store and retrieve content-addressable data. The primary use case is for `ExecutionReceipts`, where an executor `puts` the receipt into the DHT (identified by its CID), and the originator or other interested parties can `get` it. It can also be used for retrieving WASM modules or large job input data if not transferred directly.
    *   **Provider Records:** Nodes can advertise that they are "providers" for certain CIDs (e.g., they hold a copy of a specific `ExecutionReceipt`).

*   **Direct Messaging (Request-Response or Unicast Streams):**
    *   **Targeted Communication:** Used when a message is intended for a specific peer, avoiding the overhead of broadcasting to an entire topic.
    *   Examples:
        *   Sending a `JobBidV1` directly to the known job originator.
        *   Sending an `AssignJobV1` message to the chosen executor.
        *   Exchanging `JobInteractiveInputV1` and `JobInteractiveOutputV1` messages.
        *   Directly requesting an `ExecutionReceipt` from an executor if a DHT GET fails or for faster retrieval from a known source.
    *   **Reliability:** Direct messaging can more easily incorporate acknowledgments or use reliable transport mechanisms if required by the interaction.

By combining these libp2p protocols, the Planetary Mesh achieves a flexible and efficient communication system capable of supporting its diverse operational needs, from broad discovery to targeted, reliable data exchange.