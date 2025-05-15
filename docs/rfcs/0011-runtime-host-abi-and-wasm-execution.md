# RFC 0011: Runtime Host ABI and WASM Execution

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0003 (Context Model), RFC 0010 (Mana Accounting), RFC 0030 (CCL Syntax), RFC 0042 (Credential Types)

---

## 0. Abstract

This RFC defines the Application Binary Interface (ABI) exposed to WebAssembly (WASM) modules executing in the ICN runtime. The host ABI acts as the secure and auditable bridge between user-defined cooperative logic (CCL) and the execution environment. This includes access to identity, context, storage, reputation, and economic primitives.

---

## 1. Introduction

ICN executes CCL contracts compiled to WASM. To ensure deterministic behavior, access control, and observability, the runtime must tightly control what system functions are exposed to these modules.

The Host ABI provides a structured, versioned, and sandboxed interface between the guest WASM module and the ICN runtime. This ABI must:

* Support scoped access to identity and ledger
* Enforce resource usage via mana and policies
* Enable structured output via ExecutionReceipts
* Remain stable and backwards-compatible across versions

---

## 2. ABI Principles

* **Minimal surface area**: Only expose explicitly permitted functionality.
* **Scope-aware**: All operations are contextualized by a ScopeKey.
* **Auditable**: Calls should produce traceable, observable outputs.
* **Safe**: WASM cannot access arbitrary memory or unsafe syscalls.
* **Versioned**: ABI versions can be tracked and enforced at load-time.

---

## 3. ABI Function Categories

### 3.1 Identity and Context

```rust
fn host_get_scope_key(ptr: u32, len: u32) -> i32;
fn host_get_originator_did(ptr: u32, len: u32) -> i32;
fn host_get_roles(ptr: u32, len: u32) -> i32;
```

### 3.2 Economic Functions

```rust
fn host_account_get_mana(did_ptr: u32, len: u32) -> i32;
fn host_account_spend_mana(did_ptr: u32, len: u32, amount: u64) -> i32;
```

### 3.3 Storage Access

```rust
fn host_kv_read(key_ptr: u32, key_len: u32, out_ptr: u32) -> i32;
fn host_kv_write(key_ptr: u32, key_len: u32, val_ptr: u32, val_len: u32) -> i32;
```

### 3.4 Receipt Output

```rust
fn host_anchor_receipt(receipt_ptr: u32, receipt_len: u32) -> i32;
```

### 3.5 Proposal and Governance (planned)

```rust
fn host_submit_proposal(...);
fn host_vote_on_proposal(...);
```

---

## 4. Execution Environment

The WASM guest module is executed using `icn-core-vm` (a wasmtime-based engine) and instrumented by `icn-runtime`. During execution:

* A `ConcreteHostEnvironment` is instantiated and bound to the guest.
* The guest invokes host functions via ABI.
* All calls are routed through scoped access layers (e.g. mana enforcement, policy checks).
* Outputs (e.g. receipts, state changes) are recorded.

---

## 5. ABI Versioning and Compatibility

Each contract must declare an ABI version in its preamble or WASM metadata. The runtime validates this against its supported ABI set before loading.

Breaking changes (e.g., function signatures, semantics) require a version bump. New functions may be added to the existing version as long as they don’t alter previous behavior.

---

## 6. Observability

All ABI calls should:

* Emit Prometheus metrics where relevant (e.g., mana spent)
* Be loggable for audit/replay purposes
* Optionally appear in the `ExecutionReceipt`

---

## 7. Rationale and Alternatives

The ABI design draws inspiration from smart contract platforms like WASI, EVM, and Solana’s BPF interface, but is tailored for:

* Cooperative scope resolution
* Reputation-aware execution
* Resource fairness via mana

Alternatives such as direct syscall exposure or capability injection were rejected for safety and auditability reasons.

---

## 8. Security Considerations

* WASM isolation protects the host environment from untrusted guest logic.
* All host operations are pre-authorized and bounded.
* Memory sharing is controlled via Wasmtime's linear memory system.
* Inputs and outputs are bounded by length limits and serialization validation.

---

## 9. Privacy Considerations

* DIDs and scope keys exposed to the guest are not considered private.
* No raw personally identifying data is exposed.
* Hosts may redact or mask sensitive fields in policy-controlled environments.

---

## 10. Future Work

* Expose contract-local logging and error codes
* Support dynamic function discovery (`host_list_functions()`)
* Extend storage model to enable append-only event logs
* Define governance-specific ABI subset

---

## 11. Acknowledgements

Thanks to the ICN runtime and wasm tooling contributors who validated this interface through active job execution and testbed deployments.

---

## 12. References

* \[RFC 0003: CCL Context and Scope Model]
* \[RFC 0010: Mana Accounting and Regeneration]
* [Wasmtime ABI documentation](https://docs.wasmtime.dev/)

---

**Filename:** `0011-runtime-host-abi-and-wasm-execution.md`
