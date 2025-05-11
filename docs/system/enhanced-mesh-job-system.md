# The ICN Enhanced Mesh Job System

This document details the architecture, capabilities, and lifecycle of the enhanced mesh job system within the InterCooperative Network (ICN). This system is designed to support complex, potentially multi-stage, interactive, and AI-driven tasks, furthering the ICN's mission to enable sophisticated cooperative management, governance, and the development of decentralized applications that address social needs.

## Core Concepts of the Enhanced Mesh Job System

The enhanced mesh job system introduces several new and modified concepts that are fundamental to understanding its capabilities. These concepts revolve around how jobs are defined, how their lifecycle is managed, and how they communicate.

### 1. The Transformed `MeshJobParams`: Blueprint for Complexity

The `MeshJobParams` struct, found in the `icn-types` crate, is the primary descriptor for any job submitted to the network. It has been significantly enhanced to support more sophisticated execution patterns:

*   **`workflow_type: WorkflowType`**: This crucial field dictates the overall structure of the job.
    *   `SingleWasmModule`: The traditional model where the job consists of executing a single WebAssembly module.
    *   `SequentialWorkflow`: Defines the job as a sequence of distinct stages, where each stage is typically a WASM module. Stages execute one after another, potentially passing output from one stage as input to the next. This enables the creation of multi-step processing pipelines.
    *   `GraphWorkflow` (Future Vision): While not yet fully implemented, this type anticipates jobs defined as a directed acyclic graph (DAG) of stages, allowing for more complex parallel and conditional execution paths.

*   **`stages: Option<Vec<StageDefinition>>`**: Used when `workflow_type` is `SequentialWorkflow` (or eventually `GraphWorkflow`). It's a vector defining each stage in the workflow:
    *   **`stage_id: String`**: A unique identifier for the stage within the workflow, useful for referencing outputs or managing stage-specific state.
    *   **`wasm_cid: String`**: The CID of the WASM module to be executed for this stage. This allows different WASM modules to be composed into a single job.
    *   **`description: Option<String>`**: A human-readable description of the stage's purpose.
    *   **`input_source: StageInputSource`**: Defines where this stage gets its primary input from:
        *   `JobInput(String)`: Input is taken from the initial `MeshJobParams.input_data_cid`, or if the job defines multiple named initial inputs (a future enhancement), this string key would specify which one.
        *   `PreviousStageOutput(String, String)`: Input is taken from the output of a previous stage. The first string is the `stage_id` of the producer stage, and the second string is an optional key if that stage produced multiple named outputs.
        *   `NoInput`: The stage does not require a primary CID-based input (it might receive all necessary data via interactive means or generate it).
    *   **`timeout_seconds: Option<u64>`**: An optional execution timeout specific to this stage.
    *   **`retry_policy: Option<StageRetryPolicy>`**: (Conceptual) Defines how and if a failed stage should be retried.

*   **`is_interactive: bool`**: A boolean flag that, when set to `true`, signals to the ICN Runtime that the job's WASM module(s) intend to use the interactive portions of the `MeshHostAbi`. This enables capabilities like prompting the user for input during execution and sending real-time data back to the originator. This is the cornerstone of creating responsive, conversational, or human-in-the-loop jobs.

*   **`expected_output_schema_cid: Option<String>`**: An optional CID pointing to a schema (e.g., JSON Schema, CBOR-LD schema) that describes the expected structure of the final output of the entire job. This can be used for validation or by clients to understand how to interpret the job's results.

These enhancements transform `MeshJobParams` from a simple job ticket into a rich blueprint for defining complex, multi-stage, and interactive processes.

### 2. The Evolved `JobStatus` Lifecycle: Tracking Active Processes

The `JobStatus` enum, defined in `planetary-mesh/src/lib.rs`, tracks the state of a job throughout its execution. It has been augmented to reflect the more dynamic nature of enhanced jobs:

*   **`Created`**: Initial state before submission.
*   **`Submitted`**: Job has been announced to the network.
*   **`Assigned { node_id }`**: Job has been assigned to an executor node.
*   **`Running { node_id, current_stage_index, current_stage_id, progress_percent, status_message }`**:
    The job (or a specific stage) is actively executing on the assigned node.
    *   `current_stage_index` and `current_stage_id`: Indicate which stage is running in a multi-stage workflow.
    *   `progress_percent` and `status_message`: Allow the job to report fine-grained progress back to the originator.
*   **`PendingUserInput { node_id, prompt_cid, current_stage_index, current_stage_id }`**:
    A critical new state for interactive jobs. The job is paused, awaiting input from the originator (or another designated user).
    *   `prompt_cid`: Optionally points to data (e.g., a form schema, a detailed question) describing the input needed.
*   **`AwaitingNextStage { node_id, next_stage_index, next_stage_id }`**:
    In a sequential workflow, the current stage has completed successfully, and the runtime is preparing to execute the next stage.
*   **`Completed { node_id, receipt_cid }`**: The job has finished all its work successfully.
    *   `receipt_cid`: Points to an `ExecutionReceipt` summarizing the job's execution and outputs.
*   **`Failed { node_id, error, stage_index, stage_id }`**: The job (or a specific stage) encountered an error and could not complete.
    *   `stage_index` and `stage_id`: Pinpoint where in a workflow the failure occurred.
*   **`Cancelled`**: The job was cancelled by the originator or a governance mechanism.

This richer lifecycle allows for more transparent and manageable execution of jobs, especially those involving user interaction or multiple processing steps.

### 3. The Language of Interaction: Core P2P Messages

To support these enhanced capabilities, new and refined P2P messages, defined in `planetary-mesh/src/protocol.rs` as variants of `MeshProtocolMessage`, facilitate the necessary communication:

*   **`JobStatusUpdateV1 { job_id, executor_did, status: JobStatus }`**:
    This message is sent by the executor node to the job originator (and potentially other interested parties) whenever there's a significant change in the job's `JobStatus`. It's the primary way the originator stays informed about the job's progress, including when it's `PendingUserInput` or has moved to a new stage.

*   **`JobInteractiveInputV1 { job_id, target_executor_did, source_user_did, sequence_num, payload_cid, payload_inline }`**:
    When an interactive job is `PendingUserInput`, the originator (or an authorized user) sends this message to the `target_executor_did` to provide the requested input.
    *   `payload_cid` / `payload_inline`: Allows input to be provided either as a CID (for larger data) or directly inline within the message (for smaller inputs).
    *   `sequence_num`: Helps in ordering inputs if multiple are sent rapidly, though the primary ordering is typically managed by the job's request-response flow.

*   **`JobInteractiveOutputV1 { job_id, executor_did, target_originator_did, sequence_num, payload_cid, payload_inline, is_final_chunk, output_key }`**:
    This message is sent by an executing interactive job (via the executor node) back to the `target_originator_did`. It's how the job sends real-time data, responses, or partial results.
    *   `payload_cid` / `payload_inline`: Similar to input, output can be inline or via CID.
    *   `sequence_num`: Maintained by the job for ordered delivery of output chunks.
    *   `is_final_chunk`: Indicates if this is the last piece of a potentially larger, streamed output.
    *   `output_key`: Allows the job to associate the output with a specific context or previous input, useful for UIs or stateful interactions.

These messages form the communication backbone for the dynamic and interactive features of the enhanced mesh job system, enabling a responsive dialogue between users, executing contracts, and the ICN runtime.

## Architecture of the Enhanced Mesh Job System

The enhanced mesh job system is a coordinated effort across several key crates within the InterCooperative Network. Each component plays a distinct role, from defining the structure of jobs to executing them and facilitating communication. Understanding this architecture is key to developing on and contributing to the ICN.

### Key Components and Their Roles

The following diagram provides a high-level overview of the primary components involved and their interactions. (Conceptual diagram description follows, as actual diagram generation is beyond this text-based interaction).

```
+---------------------+      +---------------------+      +-----------------------+
|   User/Originator   |<---->|   Planetary Mesh    |<---->|  ICN Node (Executor)  |
+---------------------+      |(P2P Communication)  |      +-----------------------+
                               +---------------------+                |
                                                                      | (Job Execution & ABI)
                                                                      v
+------------------------------------------------------------------------------------------+
| ICN Runtime (on Executor Node)                                                           |
| +-------------------------+   +---------------------------+   +------------------------+ |
| | JobExecutionContextMgr  |<->| ConcreteHostEnvironment   |<->| WASM VM (e.g., Wasmer) | |
| | (Manages JobContexts)   |   | (Implements MeshHostAbi)  |   | +--------------------+ | |
| +-------------------------+   +---------------------------+   | | CCL Contract (WASM)| | |
|            ^                          ^          ^            | +--------------------+ | |
|            |                          |          |            +------------------------+ |
|            | (P2P Msg In/Out)         |          | (Host Service Calls)                   |
|            v                          |          v                                        |
| +-------------------------+           |   +------------------------+                     |
| | P2P Service Interface   |-----------+   | Storage Service Interface|                     |
| +-------------------------+               +------------------------+                     |
+------------------------------------------------------------------------------------------+

      ^                                       ^
      | (Types & Definitions)                 | (ABI Contract)
      |                                       |
+---------------------+      +---------------------+      +----------------------------+
|     icn-types       |      |   icn-host-abi      |      | icn-ccl-* (Compiler/StdEnv)|
+---------------------+      +---------------------+      +----------------------------+
```

*   **`icn-types`**: This foundational crate provides the Rust struct definitions for core data types used across the ICN, including the enhanced mesh job system.
    *   **Key Contributions**: Defines `MeshJobParams` (which includes `WorkflowType`, `StageDefinition`, `StageInputSource`), `JobId`, and other shared types. It ensures that all components have a common understanding of what a job and its parameters look like.

*   **`planetary-mesh`**: This crate is responsible for all P2P network communication. For the mesh job system, it handles:
    *   **Key Contributions**: Broadcasting `JobAnnouncementV1` messages. Relaying `JobInterestV1` from potential executors. Transmitting `JobStatusUpdateV1` messages from the executor to the originator. Ferrying `JobInteractiveInputV1` (from originator to executor) and `JobInteractiveOutputV1` (from executor to originator) for interactive jobs. It defines the `JobStatus` enum that is used in these P2P communications.

*   **`icn-host-abi`**: This crate defines the critical Application Binary Interface (ABI) – a formal contract – that WebAssembly (WASM) modules (such as compiled Cooperative Contract Language - CCL contracts) use to interact with the host ICN Runtime.
    *   **Key Contributions**: Defines the `MeshHostAbi` trait, which lists all functions a WASM module can call (e.g., `host_job_get_id`, `host_interactive_send_output`, `host_data_read_cid`). It also defines associated data structures like `HostAbiError`, `ReceivedInputInfo`, and `LogLevel` that are passed across the WASM/host boundary.

*   **`icn-runtime`**: This is the engine room on an executor node, responsible for managing and executing mesh jobs.
    *   **Key Contributions**: 
        *   Listens for assigned jobs.
        *   Fetches WASM modules.
        *   Manages `JobExecutionContext` for each active job, holding its state, parameters, input queues, permissions, and resource usage.
        *   Instantiates a WASM Virtual Machine (e.g., Wasmer, Wasmtime) to run the job's WASM code.
        *   Provides the `ConcreteHostEnvironment`, which is the concrete implementation of the `MeshHostAbi` trait. This implementation bridges calls from the WASM module to the node's underlying capabilities (P2P communication, storage, etc.) via service interfaces.
        *   Handles the lifecycle of the job based on WASM execution and incoming P2P messages (e.g., queueing interactive input, updating status).

*   **`icn-ccl-*` (Compiler, Standard Environment)**: These crates are responsible for the Cooperative Contract Language.
    *   **Key Contributions**:
        *   The `icn-ccl-compiler` translates CCL code into WASM modules that can run on the `icn-runtime`.
        *   A CCL Standard Environment/Library (conceptually `ccl_std_env`, itself compiled into the CCL contract WASM or provided as intrinsic functions) provides CCL developers with safe and ergonomic wrappers around the raw `MeshHostAbi` calls. This includes managing memory allocation within the WASM module for buffers passed to/from the host and parsing complex structures like `ReceivedInputInfo`.

### High-Level Interaction Flows & Data Flow

1.  **Job Submission and Initiation:**
    *   **Data:** `MeshJobParams` (defined in `icn-types`).
    *   **Flow:** An Originator crafts `MeshJobParams`. These are wrapped in a `JobAnnouncementV1` message (`planetary-mesh`) and broadcast. An Executor Node's `icn-runtime` receives this (or is directly assigned the job). The `icn-runtime` creates a `JobExecutionContext` to manage the job's state.

2.  **WASM Execution and Host ABI Interaction:**
    *   **Data:** Function arguments (pointers, lengths), return codes (`HostAbiError`), data buffers (for CIDs, strings, interactive payloads).
    *   **Flow:** The `icn-runtime` instantiates the job's WASM module (from `wasm_cid` in `MeshJobParams`) within a WASM VM.
        *   The CCL contract (WASM) makes calls to the host (e.g., `Host.Abi.interactive_send_output(...)`). These are calls to functions defined in the `MeshHostAbi` trait (`icn-host-abi`).
        *   The `ConcreteHostEnvironment` (in `icn-runtime`), which implements `MeshHostAbi`, receives these calls.
        *   To fulfill these calls, the `ConcreteHostEnvironment` interacts with the `JobExecutionContext` (to get job ID, check permissions, update sequence numbers, manage input queues) and with other node services (like a P2P service to send a `JobInteractiveOutputV1` message, or a Storage service to handle `host_data_read_cid`).
        *   Results and errors are passed back to the WASM module across the ABI.

3.  **P2P Communication for Status and Interactivity:**
    *   **Data:** `JobStatusUpdateV1`, `JobInteractiveInputV1`, `JobInteractiveOutputV1` messages (defined in `planetary-mesh`), containing `JobStatus` (from `planetary-mesh`), DIDs, sequence numbers, and payloads (inline bytes or CIDs).
    *   **Flow:**
        *   **Status Updates:** When the `icn-runtime` (e.g., via `ConcreteHostEnvironment` logic or direct `JobExecutionContext` updates) changes a job's state (e.g., to `PendingUserInput` after a `host_interactive_prompt_for_input` call), it uses its P2P service interface to send a `JobStatusUpdateV1` message out via `planetary-mesh`.
        *   **Interactive Input:** An external user sends a `JobInteractiveInputV1` message. `planetary-mesh` routes this to the target Executor Node. The node's P2P service layer passes it to the `icn-runtime`, which queues it in the appropriate `JobExecutionContext`. The WASM module later retrieves this via `host_interactive_receive_input`.
        *   **Interactive Output:** A WASM module calls `host_interactive_send_output`. The `ConcreteHostEnvironment` in `icn-runtime` constructs a `JobInteractiveOutputV1` message and sends it via its P2P service interface, which then uses `planetary-mesh` to deliver it to the Originator.

This interconnected architecture allows the ICN to support robust, stateful, and interactive decentralized applications by clearly delineating responsibilities while ensuring efficient communication and data flow between components. The `MeshHostAbi` acts as the linchpin, enabling controlled interaction between untrusted WASM modules and the trusted host runtime environment.

## The Interactive EchoBot: A Lifecycle Walkthrough

To understand the enhanced mesh job system in action, let's trace the lifecycle of a simple yet illustrative interactive application: the "EchoBot." This Cooperative Contract Language (CCL) based job will prompt a user for text input and echo it back, continuing until the user sends a specific "exit" command. This example, while basic, showcases the core mechanics of job submission, interactive communication, status updates, and WASM execution within the ICN.

**Actors:**

*   **Originator (Alice):** A user or an automated agent initiating the EchoBot job.
*   **Executor Node (Bob's Node):** An ICN node that picks up and runs the EchoBot job.
*   **ICN Planetary Mesh:** The P2P network facilitating communication.
*   **ICN Runtime (on Bob's Node):** The environment that loads and executes the EchoBot WASM module.
*   **EchoBot CCL Contract (WASM):** The compiled CCL code containing the EchoBot's logic.

---

**Phase 1: Job Definition and Submission (Alice, the Originator)**

Alice wishes to run the EchoBot. She (or her client application) constructs `MeshJobParams` that define the job:

1.  **`job_id`**: A unique identifier for this job instance (e.g., generated by Alice's client).
2.  **`wasm_cid`**: The Content Identifier (CID) of the compiled EchoBot CCL contract WASM module, already stored on the ICN.
3.  **`description`**: "Interactive EchoBot: Prompts for input and echoes it back."
4.  **`workflow_type`**: `WorkflowType::SingleWasmModule` (as it's a single WASM execution).
5.  **`stages`**: `None` (not a multi-stage workflow).
6.  **`is_interactive`**: `true` (crucial for enabling interactive ABI calls).
7.  **`input_data_cid`**: `None` (EchoBot doesn't require initial bulk input).
8.  **`expected_output_schema_cid`**: `None` (EchoBot's final output is implicit in its interactive nature, though a schema could define a final "session_summary_cid" if desired).
9.  **`max_acceptable_bid_tokens`**: An amount Alice is willing to pay.
10. **`deadline`**: Optional.
11. **`qos_profile`**: Desired Quality of Service.
12. **`resources_required`**: Estimated resource needs (CPU, memory).

Alice's client then broadcasts a `MeshProtocolMessage::JobAnnouncementV1` containing these `MeshJobParams` over the ICN Planetary Mesh.

---

**Phase 2: Job Discovery and Assignment (Bob's Node, the Executor)**

1.  **Discovery:** Bob's Node, configured to execute jobs, monitors the mesh for `JobAnnouncementV1` messages. It evaluates if it meets the `qos_profile` and resource requirements for Alice's EchoBot job.
2.  **Interest & Bidding (Simplified):** Bob's Node expresses interest (e.g., via a `JobInterestV1` message, potentially including a bid if a bidding mechanism is active).
3.  **Assignment (Simplified):** Alice (or a decentralized assignment mechanism) selects Bob's Node as the executor. The job's status conceptually transitions to `Submitted` and then `Assigned { node_id: Bob's_Node_DID }`. For this walkthrough, we assume Bob's Node is assigned.

---

**Phase 3: Runtime Setup and Initial Execution (Bob's Node & ICN Runtime)**

1.  **Context Creation:** Upon assignment, the ICN Runtime on Bob's Node:
    *   Fetches the EchoBot WASM module using its `wasm_cid`.
    *   Creates a new `JobExecutionContext` instance, populating it with:
        *   `job_id`, `originator_did` (Alice's DID), full `job_params`.
        *   `current_status` initialized to `JobStatus::Running { node_id: Bob's_Node_DID, progress_percent: Some(0), status_message: Some("Initializing EchoBot...") }`.
        *   `is_interactive: true` (from `job_params`).
        *   An empty `interactive_input_queue`.
        *   `interactive_output_sequence_num: 0`.
        *   Relevant `permissions` based on `job_params` (e.g., `can_send_interactive_output = true`).
2.  **WASM Instantiation:** The Runtime instantiates the EchoBot WASM module, providing it with an environment that implements the `MeshHostAbi`. This environment is linked to the newly created `JobExecutionContext` (e.g., via an `Arc<Mutex<JobExecutionContext>>` made available to host function implementations).
3.  **CCL Entry Point:** The Runtime calls the designated entry point in the EchoBot CCL contract, which we'll assume is `public function run_interactive_job() -> I32`.

---

**Phase 4: The Interactive Loop (EchoBot CCL Contract & ICN Runtime on Bob's Node)**

The `run_interactive_job` function in the EchoBot CCL contract now executes:

1.  **Initial Prompt (CCL -> Host -> P2P):**
    *   **CCL:** The EchoBot contract wants to prompt Alice for input. It might prepare a prompt message like "Welcome to EchoBot! Type something:" and store this prompt message using `host_data_write_buffer` to get a `prompt_cid`.
    *   **CCL calls ABI:** `Host.Abi.interactive_prompt_for_input(prompt_cid_ptr, prompt_cid_len)` (or a CCL stdlib wrapper like `ICN.Interactive.prompt_with_cid(prompt_cid)`).
    *   **Runtime (`host_interactive_prompt_for_input`):**
        *   Receives the call, locks the `JobExecutionContext`.
        *   Updates `current_status` to `JobStatus::PendingUserInput { node_id: Bob's_Node_DID, prompt_cid: Some(the_prompt_cid), ... }`.
        *   Constructs a `JobStatusUpdateV1` message with this new status.
        *   Sends this `JobStatusUpdateV1` via the P2P service to Alice (the `originator_did`).
        *   Returns `HostAbiError::Success` to the CCL contract.
    *   **CCL:** The contract might now enter a loop, awaiting input.

2.  **User Input (Alice -> P2P -> Host):**
    *   **Alice's Client:** Receives the `JobStatusUpdateV1` indicating `PendingUserInput`. It displays the prompt (potentially fetching the content of `prompt_cid`).
    *   Alice types "Hello ICN!" and hits send.
    *   **Alice's Client sends P2P:** `JobInteractiveInputV1 { job_id, target_executor_did: Bob's_Node_DID, source_user_did: Some(Alice's_DID), sequence_num: 1, payload_inline: Some("Hello ICN!".as_bytes()) }`.

3.  **Input Reception and Processing (Runtime -> CCL -> Host -> P2P):**
    *   **Runtime (Bob's Node):** Receives the `JobInteractiveInputV1`.
        *   Verifies `target_executor_did` matches itself.
        *   Locks the `JobExecutionContext` for the given `job_id`.
        *   Pushes the `JobInteractiveInputV1` message onto the `interactive_input_queue`.
        *   If the WASM task was suspended waiting for input (an advanced async feature), it might wake the WASM task.
    *   **CCL (in its loop, calls ABI):** `Host.Abi.interactive_receive_input(buffer_ptr, buffer_len, timeout_ms)` (or a CCL stdlib wrapper like `ICN.Interactive.receive_data(timeout)`).
        *   The CCL stdlib wrapper would have called `host_interactive_peek_input_len` first to allocate an appropriately sized buffer.
    *   **Runtime (`host_interactive_receive_input`):**
        *   Locks `JobExecutionContext`.
        *   Checks `interactive_input_queue`. Finds "Hello ICN!".
        *   Pops the message.
        *   Constructs `ReceivedInputInfo { input_type: ReceivedInputType::InlineData, data_len: "Hello ICN!".len() }`.
        *   Writes `ReceivedInputInfo` and then "Hello ICN!" into the WASM module's provided buffer.
        *   If the `current_status` was `PendingUserInput`, transitions it back to `JobStatus::Running { ..., status_message: Some("Input received, processing...") }`. (Optionally sends a `JobStatusUpdateV1`).
        *   Returns the total bytes written to the CCL contract.
    *   **CCL:**
        *   The stdlib wrapper parses `ReceivedInputInfo` and extracts "Hello ICN!".
        *   The EchoBot logic checks if the input is the "exit" command. It's not.
        *   It prepares the echo response: "Echo: Hello ICN!".
    *   **CCL calls ABI:** `Host.Abi.interactive_send_output(payload_ptr, payload_len, output_key_ptr: 0, output_key_len: 0, is_final_chunk: 1)` (or `ICN.Interactive.send_reply(b"Echo: Hello ICN!")`).
    *   **Runtime (`host_interactive_send_output`):**
        *   Locks `JobExecutionContext`.
        *   Reads "Echo: Hello ICN!" from WASM memory.
        *   Determines it's small enough for `payload_inline`.
        *   Increments `interactive_output_sequence_num`.
        *   Constructs `JobInteractiveOutputV1 { job_id, executor_did: Bob's_Node_DID, target_originator_did: Alice's_DID, sequence_num, payload_inline: Some(b"Echo: Hello ICN!"), is_final_chunk: true, ... }`.
        *   Sends this message via P2P service to Alice.
        *   Returns `HostAbiError::Success` to CCL.
    *   **Alice's Client:** Receives `JobInteractiveOutputV1`, displays "Echo: Hello ICN!".

4.  **Loop Continues:**
    *   The CCL contract loops back. It might call `host_interactive_prompt_for_input` again (perhaps with a generic "Ready for next input" CID, or no CID if the client UI implies it). This sends another `JobStatusUpdateV1 { PendingUserInput }` to Alice.
    *   Alice sends more input. The cycle repeats.

---

**Phase 5: Job Termination (User Initiates Exit)**

1.  **User Sends Exit Command:**
    *   Alice types "exit" and sends it.
    *   This flows through P2P as a `JobInteractiveInputV1` to Bob's Node, gets queued, and is picked up by `host_interactive_receive_input` in the CCL contract.
2.  **CCL Recognizes Exit:**
    *   The EchoBot CCL logic receives "exit".
    *   It determines the loop should terminate.
    *   It might send a final confirmation: `Host.Abi.interactive_send_output(b"EchoBot shutting down. Goodbye!", ..., is_final_chunk: true)`. Alice's client displays this.
3.  **CCL Exits:** The `run_interactive_job` function in the CCL contract finishes and returns `HostAbiError::Success` (as an `I32` value of 0).

---

**Phase 6: Job Finalization (Bob's Node & ICN Runtime)**

1.  **Runtime Detects Completion:** The ICN Runtime on Bob's Node sees that the WASM entry point (`run_interactive_job`) has returned successfully.
2.  **Final Status Update:**
    *   The Runtime locks the `JobExecutionContext`.
    *   Updates `current_status` to `JobStatus::Completed { node_id: Bob's_Node_DID, receipt_cid: "some_final_receipt_cid" }`. (The `receipt_cid` would point to an `ExecutionReceipt` detailing resource usage, final state, etc., created by the runtime).
    *   Sends a final `JobStatusUpdateV1` with this `Completed` status to Alice.
3.  **Execution Receipt (Simplified):** The Runtime generates an `ExecutionReceipt` (which could include total mana consumed, CPU time, etc., from `JobExecutionContext`), signs it, and stores it (getting `some_final_receipt_cid`). This receipt might be announced via `ExecutionReceiptAvailableV1`.
4.  **Cleanup:** The Runtime cleans up resources associated with this job instance (WASM instance, `JobExecutionContext`).

---

**Conclusion of Lifecycle**

Alice's client receives the final `Completed` status. The EchoBot session is finished. Through this lifecycle, we've seen how `MeshJobParams` define an interactive job, how `JobStatus` tracks its evolving state, how `MeshProtocolMessage` variants facilitate communication of status and interactive data, and how `MeshHostAbi` functions bridge the CCL contract with the runtime environment, all orchestrated by the ICN Runtime on the executor node. This dance of components enables sophisticated, stateful, and interactive applications to run securely and resiliently across the InterCooperative Network. 