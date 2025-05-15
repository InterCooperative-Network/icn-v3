# RFC 0003: CCL Context and Scope Model

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0000 (Process), RFC 0001 (Structure), RFC 0030 (Syntax), RFC 0040 (DID and ScopeKey)

---

## 0. Abstract

This RFC defines the context model and scope resolution system for CCL (Cooperative Contract Language), the domain-specific language powering ICN’s programmable governance and resource policies. It describes how identity, organizational hierarchy, and contextual execution data are resolved during contract execution, and how this information is exposed to CCL contracts during runtime.

---

## 1. Introduction

CCL contracts are executed within a decentralized runtime that must resolve organizational scope and identity relationships in order to enforce policies, access control, and reputation impact. To support this, each execution must be bound to a deterministic `ExecutionContext` that is aware of:

* The submitting identity (DID)
* Its associated cooperative, community, and federation
* The organizational scope of execution (ScopeKey)
* The runtime node and validator scope
* Relevant roles, permissions, and policies attached to any of the above

This RFC describes the formal structure of the CCL execution context and how it interacts with identity resolution and scope enforcement in the runtime.

---

## 2. Terminology

* **DID** – Decentralized Identifier
* **ScopeKey** – A string or struct that uniquely identifies a cooperative, community, federation, or DID-level scope
* **ExecutionContext** – The resolved runtime context that provides scoped access to policy and identity data
* **CCL Host ABI** – The set of functions exposed to CCL WASM contracts for querying context, reading/writing data, and managing receipts

---

## 3. Context Model

### 3.1 Identity Resolution

During contract submission and runtime execution, the following identities are resolved:

* **Originator DID** – The submitting user or service
* **Runtime Node DID** – The executing validator or mesh node
* **Scoped Relationships** – Using the Identity Index, resolve:

  * Originator’s Cooperative DID
  * Cooperative’s Community DID
  * Community’s Federation DID

### 3.2 ScopeKey Hierarchy

Each execution is assigned a `ScopeKey`, used for mana accounting, reputation tracking, and policy enforcement. The hierarchy is as follows:

```
ScopeKey::Federation(FederationDID)
ScopeKey::Community(CommunityDID)
ScopeKey::Cooperative(CooperativeDID)
ScopeKey::Individual(DID)
```

Scopes are resolved using the most specific match available to the originator. Contracts may reference the current `ScopeKey` for gating logic or enforcement.

### 3.3 ExecutionContext Struct

```rust
pub struct ExecutionContext {
    pub originator_did: Did,
    pub runtime_node_did: Did,
    pub scope_key: ScopeKey,
    pub cooperative_did: Option<Did>,
    pub community_did: Option<Did>,
    pub federation_did: Option<Did>,
    pub policies: Vec<ResourcePolicy>,
    pub roles: Vec<AssignedRole>,
}
```

This context is initialized prior to execution and accessible via host ABI queries.

---

## 4. Host ABI Exposure

CCL contracts may query execution context through the following functions:

```rust
extern "C" {
    fn host_get_scope_key(buf_ptr: u32, buf_len: u32) -> i32;
    fn host_get_originator_did(buf_ptr: u32, buf_len: u32) -> i32;
    fn host_get_roles(buf_ptr: u32, buf_len: u32) -> i32;
    fn host_check_permission(policy_key_ptr: u32, len: u32) -> i32;
}
```

These functions allow contracts to:

* Verify access control based on DID and ScopeKey
* Adjust execution logic depending on cooperative or community context
* Emit receipts that are scoped and attributable

---

## 5. Rationale and Alternatives

CCL’s expressiveness depends on clear, auditable, and deterministic context resolution. The ScopeKey hierarchy reflects ICN’s federated governance structure and supports localized enforcement.

Alternatives such as flat DID-only scoping were rejected due to insufficient support for cooperative and federation-level policy abstraction.

---

## 6. Backward Compatibility

This model codifies behavior already implemented in the runtime and used implicitly in execution receipts and mana enforcement. Existing contracts are compatible if they use the current host ABI.

---

## 7. Security Considerations

* All context resolution must be verifiable and deterministic.
* Scope impersonation is prevented by cryptographic validation of identity and signed receipts.
* Malicious host ABI responses are mitigated by signature verification and DAG anchoring.

---

## 8. Privacy Considerations

* The execution context may expose scoped relationships (e.g., a DID’s cooperative). This data is not private and is required for governance logic.
* No PII is stored or transmitted.

---

## 9. Economic Impact

* Mana is tracked per `ScopeKey`, meaning cooperative or community quotas are enforceable.
* Receipt impact and reputation updates are scoped appropriately.
* This enables policy-driven throttling or role-based discounts.

---

## 10. Open Questions and Future Work

* Should CCL support custom scope definitions beyond the core hierarchy?
* Should cooperative/community metadata be accessible in contracts?
* Formalize the policy query model exposed to WASM.

---

## 11. Acknowledgements

Thanks to the developers of `icn-runtime`, `icn-identity`, and `icn-ccl-dsl` for prototyping these ideas in code prior to formal documentation.

---

## 12. References

* \[RFC 0000: RFC Process and Structure]
* \[RFC 0001: Project Structure and Directory Layout]
* \[RFC 0040: DID and ScopeKey Definitions (planned)]
* \[RFC 0010: Mana Accounting and Regeneration (planned)]

---

**Filename:** `0003-ccl-context-and-scope-model.md`
