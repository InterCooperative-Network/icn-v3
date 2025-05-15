# RFC 0013: Economics Engine and Token Resource Types

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0010 (Mana), RFC 0012 (Reputation), RFC 0015 (Policy Enforcer), RFC 0017 (Token Flow)

---

## 0. Abstract

This RFC defines the economic system underlying the ICN runtime and service architecture. It introduces the `EconomicsEngine`, `ScopedResourceToken`, and `LedgerKey` abstractions, enabling modular accounting and quota enforcement across diverse resource types such as compute (mana), tokens, and access rights.

---

## 1. Introduction

The ICN platform supports multiple layers of economics:

* **Mana:** Internal, regenerating compute quota
* **Tokenized resources:** External assets (e.g., ICN-F, ICN-R, cooperative credits)
* **Reputation incentives:** Indirect economic influence

This document focuses on the architecture of the Economics Engine and its core types, designed for extensibility, federation-specific policies, and integration with scoped execution.

---

## 2. Terminology

* **LedgerKey** – A key combining ScopeKey and resource ID for accounting
* **ScopedResourceToken** – A policy-tagged unit of a resource (e.g., mana, credits)
* **EconomicsEngine** – Trait managing balances, enforcement, and usage tracking
* **ResourceRepository** – Backend used to store and retrieve balances

---

## 3. Economics Engine Interface

The `EconomicsEngine` is a trait abstraction:

```rust
pub trait EconomicsEngine {
    fn check_authorization(&self, token: &ScopedResourceToken) -> Result<()>;
    fn record_usage(&self, token: &ScopedResourceToken) -> Result<()>;
    fn get_balance(&self, token: &ScopedResourceToken) -> Result<u64>;
}
```

This design allows:

* Mana to be enforced like any token
* Policy-aware authorization
* Pluggable backends

---

## 4. LedgerKey and Scoped Tokens

### 4.1 LedgerKey

A composite identifier:

```rust
pub struct LedgerKey {
    pub scope: ScopeKey,
    pub resource: String, // e.g., "mana", "icn-f", "compute_hours"
}
```

### 4.2 ScopedResourceToken

Represents a resource request:

```rust
pub struct ScopedResourceToken {
    pub key: LedgerKey,
    pub quantity: u64,
    pub policy: Option<ResourcePolicy>,
}
```

---

## 5. ResourceRepository Backends

Examples include:

* `ManaRepositoryAdapter`: Wraps `ManaLedger` for compute enforcement
* `SledTokenLedger`: General-purpose, persistent store using `sled`
* `InMemoryLedger`: Fast tests and local simulation

---

## 6. Resource Policies

Policies attached to `ScopedResourceToken` may enforce:

* Maximum quota
* Time-windowed rate limits
* Role-based permissions
* Token burn/mint logic (future extensions)

Policies are defined and enforced using:

```rust
pub struct ResourcePolicyEnforcer {
    pub repository: Arc<dyn ResourceRepository>,
    pub default_policy: PolicyConfig,
}
```

---

## 7. Runtime Integration

* Host ABI functions for `host_account_spend_mana()` and `get_mana()` now use `EconomicsEngine` internally
* The runtime injects an instance of `ManaRepositoryAdapter` or hybrid enforcer at execution time

---

## 8. Observability

Economics operations are instrumented via Prometheus:

* `resource_usage_total{resource="mana"}`
* `resource_denied_total{scope=..., reason=...}`
* `economics_ledger_balance{resource=...}`

These enable debugging and federation monitoring.

---

## 9. Rationale and Alternatives

This architecture allows federation-specific resource logic while maintaining a common host ABI. It separates the enforcement (PolicyEnforcer) from persistence (Repository), improving testability and reusability.

Alternative designs coupling all economic logic in runtime were rejected for lack of modularity.

---

## 10. Backward Compatibility

The design is compatible with existing `ManaLedger` and `RuntimeContext` structures. Only type aliases and wrappers are required to unify with the new trait-based system.

---

## 11. Open Questions and Future Work

* Should quotas be enforced pre- or post-job?
* Do federations define their own token types at runtime?
* Can CCL mint/burn scoped tokens?
* Should non-mana tokens have regeneration models?

---

## 12. Acknowledgements

Thanks to the authors of `icn-economics`, `ManaLedger`, and runtime integrators who tested this engine with receipts and scoped execution.

---

## 13. References

* \[RFC 0010: Mana Accounting and Regeneration]
* \[RFC 0015: Resource Policy Enforcer (planned)]
* \[RFC 0017: Token Flow and Treasury Interaction]

---

**Filename:** `0013-economics-engine-and-resource-tokens.md`
