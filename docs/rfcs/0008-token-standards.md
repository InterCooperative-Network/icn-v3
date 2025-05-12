# RFC-0008: ICN Token Standards

**Status:** Draft
**Date:** YYYY-MM-DD
**Authors:** ICN Core Team, AI Assistant

## 1. Abstract

This RFC proposes a set of standardized token types for the InterCooperative Network (ICN). Clear token standards are crucial for interoperability, enabling seamless interaction between CCL contracts, autonomous agents, and external tooling. These standards will define core metadata, transferability rules, optional extensions, and expected Host ABI interactions for different categories of tokens, fostering a robust and predictable token economy within the ICN.

## 2. Motivation

As the ICN ecosystem grows, the need for well-defined token types becomes paramount. Without standards, cooperatives and developers might implement tokens in disparate ways, leading to:
- Difficulty in creating generalized tools (wallets, explorers, dashboards).
- Increased complexity in CCL contracts that need to interact with multiple token types.
- Barriers to interoperability between different cooperatives and federations.
- Ambiguity in the meaning and utility of tokens, hindering autonomous agent coordination.

By establishing these standards, we aim to provide a common language for token representation and interaction, promoting clarity, security, and composability across the network.

## 3. Proposed Standards

We propose the following initial set of ICN Token Standards. Each standard will have a designated `standard_id` (e.g., "ICN-Fungible-1.0") for versioning and identification.

### 3.1. ICN-Fungible (ICN-F)

*   **Purpose:** Represents fungible assets, such as cooperative credits, utility tokens for service access (e.g., mesh compute, storage), community currencies, or governance tokens where each token unit carries equal weight.
*   **Core Use Cases:**
    *   Payments for services within the ICN.
    *   Staking for resource access or service provision.
    *   Rewards and incentives.
    *   Basic governance participation (e.g., 1 token = 1 vote in simple systems).
*   **Required Metadata Fields:**
    *   `standard_id`: String (e.g., "ICN-F-1.0")
    *   `name`: String (e.g., "Cooperative Credit", "Compute Unit Token")
    *   `symbol`: String (e.g., "COCR", "CUT")
    *   `decimals`: u8 (Number of decimal places the token supports, e.g., 6)
    *   `total_supply`: u128 (Initial total supply. Can be fixed or allow for further minting if defined by issuer rules)
    *   `issuer_did`: String (DID of the cooperative or entity that minted the token)
    *   `description_cid`: Optional String (CID pointing to a more detailed description)
*   **Transferability Rules:** Fully transferable by default. Issuer can define non-transferable instances if needed, though this blurs lines with Reputation tokens.
*   **Optional Extensions:**
    *   `max_supply`: Optional u128 (If different from `total_supply`, indicating potential for future minting up to this cap)
    *   `mintable`: Boolean (Indicates if more tokens can be minted by the issuer beyond `total_supply` up to `max_supply`)
    *   `burnable`: Boolean (Indicates if tokens can be burned by holders or the issuer)
    *   `scope_id`: Optional String (e.g., Federation ID, Cooperative ID, Project ID, restricting where the token is primarily recognized or used)
    *   `expiration_timestamp`: Optional u64 (Unix timestamp for time-limited tokens)
*   **Key Host ABI Expectations (to be detailed in ABI RFC):**
    *   `host_get_token_balance(owner_did, token_type_id_or_symbol)`
    *   `host_transfer_fungible(from_did, to_did, token_type_id_or_symbol, amount)`
    *   `host_mint_fungible(recipient_did, token_type_id_or_symbol, amount)` (privileged)
    *   `host_burn_fungible(owner_did, token_type_id_or_symbol, amount)` (privileged or holder-initiated if `burnable`)
    *   `host_get_fungible_token_metadata(token_type_id_or_symbol)`

### 3.2. ICN-Reputation (ICN-R) - Potentially Soul-Bound

*   **Purpose:** Represents non-transferable (or highly restricted transferability) attestations, skills, roles, contributions, or social capital within the ICN. These are often "soul-bound" to a specific DID.
*   **Core Use Cases:**
    *   Verifiable credentials for skills or certifications.
    *   Proof of attendance or contribution.
    *   Role assignments within a cooperative (e.g., "Treasurer Role Token").
    *   Building trust scores and reputation systems.
    *   Access control based on proven characteristics rather than ownership of a fungible asset.
*   **Required Metadata Fields:**
    *   `standard_id`: String (e.g., "ICN-R-1.0")
    *   `name`: String (e.g., "Certified CCL Developer", "Project Alpha Contributor")
    *   `issuer_did`: String (DID of the issuing entity)
    *   `subject_did`: String (DID of the entity to whom the reputation is bound)
    *   `issuance_timestamp`: u64
    *   `description_cid`: String (CID pointing to detailed description, evidence, or criteria)
    *   `reputation_data_cid`: Optional String (CID pointing to specific data backing the reputation, e.g., a signed VC or a hash of achievements)
*   **Transferability Rules:** Non-transferable by default. Mechanisms for revocation or expiry should be considered. Transfer might be allowed only to the issuer (e.g., "returning" a role).
*   **Optional Extensions:**
    *   `expiration_timestamp`: Optional u64
    *   `scope_id`: Optional String (Context where this reputation is primarily valid)
    *   `revocable`: Boolean (Can the issuer revoke this token?)
    *   `claims_schema_cid`: Optional String (CID of a schema defining the structure of `reputation_data_cid`)
*   **Key Host ABI Expectations:**
    *   `host_issue_reputation_token(subject_did, token_details_cid)` (privileged)
    *   `host_revoke_reputation_token(subject_did, token_id_or_name)` (privileged)
    *   `host_get_reputation_tokens_for_did(subject_did, issuer_did_filter_optional)`
    *   `host_verify_reputation_token(subject_did, token_id_or_name)` (checks validity, non-revocation, expiry)

### 3.3. ICN-Governance (ICN-G)

*   **Purpose:** Represents voting power or rights to participate in specific governance processes. Can be fungible (like ICN-F) or non-fungible (representing a unique voting seat or right).
*   **Core Use Cases:**
    *   Voting on proposals.
    *   Electing roles or representatives.
    *   Signaling preference in community decisions.
*   **Required Metadata Fields (if fungible, inherits from ICN-F; if non-fungible, new structure):**
    *   `standard_id`: String (e.g., "ICN-G-F-1.0" for fungible, "ICN-G-NF-1.0" for non-fungible)
    *   `name`: String (e.g., "Federation Alpha Voting Token", "Coop Beta Council Seat")
    *   `symbol`: String (e.g., "FAVT", "CBS")
    *   `issuer_did`: String
    *   `governance_scope_id`: String (ID of the governance process or body this token applies to)
    *   (If Fungible): `decimals`, `total_supply`
    *   (If Non-Fungible): `token_id`: Unique u64 or String
*   **Transferability Rules:** Depends on the governance model. Some governance tokens might be freely transferable, others might be locked upon staking or tied to reputation.
*   **Optional Extensions:**
    *   `delegation_allowed`: Boolean (Can voting power be delegated?)
    *   `lockable_for_voting`: Boolean
    *   `weight_multiplier_cid`: Optional String (CID to a formula or rule for how this token's weight is calculated in voting, e.g., based on stake duration)
*   **Key Host ABI Expectations:**
    *   (Leverages ICN-F ABI if fungible)
    *   `host_get_governance_vote_weight(owner_did, governance_scope_id, proposal_id_optional)`
    *   `host_delegate_governance_tokens(...)` (if `delegation_allowed`)
    *   `host_lock_governance_tokens_for_vote(...)` (if `lockable`)

### 3.4. ICN-AccessKey (ICN-AK) - Often Non-Fungible

*   **Purpose:** Represents a unique, often non-fungible, right to access a specific service, resource, piece of data, or dApp functionality.
*   **Core Use Cases:**
    *   Software licenses or subscriptions.
    *   Access to private data channels.
    *   Permission to execute specific high-value mesh jobs.
    *   Membership passes for exclusive cooperative services.
*   **Required Metadata Fields:**
    *   `standard_id`: String (e.g., "ICN-AK-NF-1.0")
    *   `name`: String (e.g., "Premium Data Feed Access Key", "Mesh Job Executor License")
    *   `issuer_did`: String
    *   `token_id`: Unique u64 or String
    *   `service_endpoint_or_resource_id`: String (Identifier for the service/resource this key unlocks)
    *   `issuance_timestamp`: u64
*   **Transferability Rules:** Can be transferable (e.g., selling a license) or non-transferable, defined by the issuer.
*   **Optional Extensions:**
    *   `expiration_timestamp`: Optional u64
    *   `max_uses`: Optional u64 (for metered access)
    *   `scope_id`: Optional String
    *   `data_payload_cid`: Optional String (CID for additional data/configuration associated with the key)
*   **Key Host ABI Expectations:**
    *   `host_issue_access_key(owner_did, key_details_cid)` (privileged)
    *   `host_verify_access_key(owner_did, token_id, service_id)` (checks validity, expiry, uses)
    *   `host_consume_access_key_use(owner_did, token_id)` (if `max_uses` is set)
    *   `host_transfer_access_key(...)` (if transferable)

### 3.5. ICN-NonFungibleReceipt (ICN-NFR)

*   **Purpose:** Standardizes the structure of receipt tokens generated by significant on-chain or mesh actions (e.g., proposal approval, budget allocation, mesh job completion, escrow release). These are proofs of a completed event.
*   **Core Use Cases:**
    *   Audit trails.
    *   Triggering subsequent actions in a workflow.
    *   Proof of task completion for payment.
    *   Verifiable record of governance decisions.
*   **Required Metadata Fields:**
    *   `standard_id`: String (e.g., "ICN-NFR-1.0")
    *   `name`: String (e.g., "Budget Proposal #123 Approval Receipt", "Mesh Job XYZ Completion Receipt")
    *   `issuer_did`: String (Typically the DID of the host runtime, federation, or contract that processed the action)
    *   `token_id`: Unique u64 or String (often related to the original action ID)
    *   `subject_action_cid_or_id`: String (CID or ID of the proposal, job, or event this receipt pertains to)
    *   `timestamp`: u64 (Timestamp of the action's completion/receipt issuance)
    *   `outcome_status`: String (e.g., "Success", "Failure", "Approved", "Rejected")
    *   `receipt_data_cid`: String (CID pointing to detailed data of the receipt, e.g., `ExecutionReceipt` structure for a mesh job, final vote tally for a proposal)
*   **Transferability Rules:** Generally non-transferable, as they represent a specific historical event tied to an actor or action. However, the `receipt_data_cid` might be public.
*   **Optional Extensions:**
    *   `related_receipt_ids`: Optional Array<String> (IDs of other related receipts, for workflow tracking)
    *   `scope_id`: Optional String
*   **Key Host ABI Expectations:**
    *   `host_issue_receipt_token(receipt_details_cid)` (privileged, often called by other host functions like `host_anchor_receipt`)
    *   `host_get_receipt_details(token_id_or_action_id)`
    *   `host_query_receipts_by_action(action_cid_or_id, status_filter_optional)`

## 4. Design Considerations

*   **Versioning:** Each `standard_id` will include a version number (e.g., "-1.0") to allow for future evolution of these standards.
*   **Composability:** Standards are designed to be composable. For example, an `ICN-Governance` token might also be an `ICN-Fungible` token. The `standard_id` could eventually become an array if a token implements multiple standards, or a primary standard can be declared with extensions.
*   **CCL Integration:** The CCL `token_def` block will allow defining tokens that adhere to these standards, potentially with CCL-defined custom logic for hooks or extensions.
*   **Host ABI Granularity:** The Host ABI will need functions specific to these standards for creation, querying, and common operations, while also allowing for generic token interaction where appropriate.

## 5. Unresolved Questions

*   How to handle tokens that might conform to multiple standards simultaneously (e.g., a fungible governance token)? A primary standard with traits/extensions, or an array of implemented standard IDs?
*   Precise mechanism for defining and enforcing transferability restrictions beyond simple non-transferability (e.g., transferable only to whitelisted DIDs, transferable only by issuer).
*   Details of how CCL `on_event` hooks for tokens would interact with host-level operations and ensure determinism.

## 6. Future Possibilities

*   Standards for more complex financial instruments (e.g., ICN-Bond, ICN-Option).
*   Formal verification of token contracts generated from CCL `token_def` against these standards.

---

This draft lays the groundwork. I'm ready for your feedback and to refine this further! 