# RFC 0015: Resource Policy Enforcer Design

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0010 (Mana), RFC 0013 (Economics Engine), RFC 0011 (Host ABI)

---

## 0. Abstract

This RFC formalizes the design of the `ResourcePolicyEnforcer`, a core component of the ICN runtime that governs whether scoped identities may access or consume limited resources (e.g., mana, tokens, compute). It provides quota, rate limit, and permission enforcement across federated, cooperative, and individual scopes.

---

## 1. Introduction

ICN enables scoped economic systems where execution resources (mana), tokens, or data access are governed by cooperative-defined rules. The `ResourcePolicyEnforcer` allows runtime enforcement of these rules in a modular, composable way.

It acts as a bridge between `ScopedResourceToken`s and the underlying economics repositories. This enables federations to define policies per scope, per resource, and per operation.

---

## 2. Terminology

* **ScopedResourceToken** – Describes an amount and type of resource scoped to a DID, coop, or federation
* **ResourceRepository** – A storage backend for resource balances (e.g., mana ledger)
* **PolicyConfig** – The quota or rate-limiting policy configuration

---

## 3. PolicyEnforcer Architecture

### 3.1 Enforcer Structure

```rust
pub struct ResourcePolicyEnforcer<R: ResourceRepository> {
    pub repository: Arc<R>,
    pub default_policy: PolicyConfig,
    pub override_policies: HashMap<LedgerKey, PolicyConfig>,
}
```

### 3.2 Enforcer Responsibilities

* Validate that `ScopedResourceToken` usage is permitted
* Track and record consumption
* Delegate balance storage to the `ResourceRepository`

---

## 4. Policy Configuration

### 4.1 PolicyConfig

```rust
pub struct PolicyConfig {
    pub max_quota: u64,
    pub max_rate_per_sec: Option<u64>,
    pub enforce_roles: Option<Vec<String>>,
}
```

This allows cooperatives to:

* Limit how much of a resource may be used
* Throttle usage rate over time
* Gate access to resource usage by role (e.g., "compute\_provider")

### 4.2 Policy Matching

* Exact match on `LedgerKey` → override
* No match → apply `default_policy`

---

## 5. Enforcer API

### Enforcement

```rust
fn check_authorization(token: &ScopedResourceToken) -> Result<()>;
```

### Usage Tracking

```rust
fn record_usage(token: &ScopedResourceToken) -> Result<()>;
```

These functions are called in tandem during runtime execution.

---

## 6. Runtime Integration

The `ConcreteHostEnvironment` initializes the `ResourcePolicyEnforcer` as part of `RuntimeContext`. Calls to `host_account_spend_mana()` pass through this enforcer. Future ABI calls for other resource types will use the same path.

This enables enforcement of per-scope quotas during job execution and prevents abuse before it occurs.

---

## 7. Observability

All enforcement actions should emit metrics:

* `resource_denied_total{reason="quota_exceeded"}`
* `resource_usage_total{resource="mana"}`

Log messages should also indicate:

* Which ScopeKey was checked
* Policy that was matched
* Result of enforcement decision

---

## 8. Rationale and Alternatives

This abstraction cleanly separates enforcement (enforcer) from persistence (repository), enabling:

* Pluggable backends (Sled, memory, federation-defined)
* Uniform logic across all scoped resource types
* Extensibility for role gating and rate limits

Alternatives like hardcoded enforcement in each resource system were rejected as inflexible and unscalable.

---

## 9. Backward Compatibility

This enforcer wraps existing mana logic (e.g., `ManaRepositoryAdapter`) and is already deployed in the runtime. Existing host ABI and receipt formats are unaffected.

---

## 10. Open Questions and Future Work

* Should the enforcer return soft-deny warnings instead of hard errors?
* Support for sliding-window or exponential rate limits?
* Federation-configurable policy overrides via governance?

---

## 11. Acknowledgements

Thanks to the developers of `icn-economics`, `icn-runtime`, and the Sled ledger integration team for making quota enforcement a core platform primitive.

---

## 12. References

* \[RFC 0010: Mana Accounting and Regeneration]
* \[RFC 0013: Economics Engine and Resource Types]
* \[RFC 0017: Token Flow and Treasury Interaction (planned)]

---

**Filename:** `0015-resource-policy-enforcer-design.md`
