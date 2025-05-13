# The Trust Loop in ICN

This document explains how trust is established, propagated, and maintained within the ICN v3 ecosystem. Trust is not merely an abstract concept but a quantifiable resource managed through verifiable actions and transparent scoring.

## 1. The Trust Loop

The core mechanism for trust propagation follows a verifiable loop:

```
DID → Execution (WASM) → Receipt → Signature Verification → Anchoring (DAG) → Reputation Submission → Scoring → Future Influence
```

1.  **DID (Decentralized Identifier):** All actors (users, nodes, cooperatives) are identified by DIDs, providing a secure and self-sovereign foundation.
2.  **Execution (WASM):** Computations are performed within a secure WASM runtime. Actors request or offer execution services.
3.  **Receipt:** Upon completion (success or failure), the executing node generates a `RuntimeExecutionReceipt` or `MeshExecutionReceipt`. This receipt contains details of the execution, including inputs, outputs (or hashes thereof), resource usage (like mana cost), and the identities of the involved parties.
4.  **Signature Verification:** The receipt must implement the `VerifiableReceipt` trait, requiring it to be cryptographically signed by the executor. This signature is verified by the submitter or relevant observers.
5.  **Anchoring (DAG):** Verified receipts are anchored to a shared Directed Acyclic Graph (DAG), providing an immutable, tamper-proof ledger of execution outcomes. This ensures consensus on the history of events.
6.  **Reputation Submission:** Anchored receipts are submitted to the Reputation System (`HttpReputationUpdater` or similar).
7.  **Scoring:** The Reputation System evaluates the receipt based on the configured `ReputationScoringConfig` (e.g., considering success/failure, mana cost, resource usage). It calculates a score delta and updates the `ReputationRecord` for the involved DIDs within the relevant scope (cooperative, community).
8.  **Future Influence:** The updated reputation score directly impacts an actor's future standing and capabilities within the system. Higher reputation can lead to preferential job assignment, increased mana regeneration rates, or access to more sensitive operations. Lower reputation can lead to throttling or exclusion.

## 2. Verifiability Guarantees

The integrity of the trust loop relies on several key guarantees:

*   **Signature Verification:** The `VerifiableReceipt` trait ensures that only the legitimate executor can produce a valid receipt for their work. Signatures are checked using the DID's associated public key.
*   **Content Immutability (CIDs):** Receipts and other critical data often use Content Identifiers (CIDs) to ensure that the data linked or referenced cannot be altered without changing its identifier.
*   **Shared State (Anchor DAG):** Anchoring receipts to the DAG creates a publicly verifiable, chronological record of events, preventing double-spending or retroactive alteration of history.
*   **Transparent Scoring:** The `ReputationScoringConfig` is accessible (potentially via configuration endpoints or discovery mechanisms), allowing participants to understand how scores are calculated. The logic within the reputation system is auditable.

## 3. Trust as a Shared Resource

In ICN, reputation is not just a score; it's a dynamic resource reflecting an entity's reliability and contribution:

*   **Influence:** Reputation directly translates into influence within cooperatives and communities. High-reputation actors are prioritized for tasks and potentially granted more privileges.
*   **Economic Link (Mana):** Mana pools act as a throttling mechanism, limiting the rate of actions. Reputation influences mana regeneration rates – trustworthy actors can act more frequently. Mana cost itself is factored into reputation scoring.
*   **Visible Standing:** Reputation scores and history are queryable (respecting privacy configurations), allowing actors to make informed decisions about whom to interact with. Success builds reputation; failure or malicious behavior diminishes it.

## 4. Scope-aware Metrics & Reputation

Trust and reputation are context-dependent. The system tracks and scores reputation records based on specific scopes:

*   `coop_id`: The cooperative within which the interaction occurred.
*   `community_id`: A potential sub-scope within a cooperative.
*   `issuer_did`: The DID that requested the work or initiated the interaction.
*   `executor_did`: The DID that performed the work.
*   `subject_did`: The DID whose reputation is being updated (often the executor).

Prometheus metrics associated with reputation events (`icn_reputation_score_updated`, `icn_reputation_receipt_processed`, etc.) are labeled with these identifiers, allowing for fine-grained monitoring and analysis of trust dynamics within different contexts.

## 5. Visual Example

*(Placeholder for a diagram illustrating the loop)*

```mermaid
graph TD
    A[User (DID)] -- 1. Submits Job --> B(Executor Node (DID));
    B -- 2. Executes Job (WASM) --> C{Generates Signed Receipt};
    C -- VerifiableReceipt --> C;
    C -- 3. Submits Receipt --> D[Reputation System];
    D -- Anchors to DAG --> E((DAG));
    D -- 4. Calculates Score Delta --> F[Reputation Record (subject_did, score_delta)];
    F -- Updates Score --> G(DID's Reputation);
    G -- Affects --> H(Mana Pool / Regeneration);
    H -- 5. Influences --> I[Future Job Priority/Access];
    I -- Feedback --> A;
    style E fill:#eee,stroke:#333,stroke-width:2px;
    style G fill:#ccf,stroke:#66f,stroke-width:2px;
```

This diagram shows the flow from job submission to execution, receipt generation, verification, anchoring, scoring, and the resulting impact on the executor's reputation and future capabilities within the ICN network. 