# RFC 0010: Mana Accounting and Regeneration

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0003 (CCL Context), RFC 0012 (Reputation Scoring), RFC 0013 (Economics Engine), RFC 0015 (Policy Enforcer)

---

## 0. Abstract

This RFC defines the mana accounting model used in the ICN runtime and reputation systems. Mana is a regenerating resource that gates execution privileges for scoped identities. It ensures fair and enforceable access to shared compute infrastructure, supports policy enforcement across organizational hierarchies, and incentivizes efficient use of federation resources.

---

## 1. Introduction

Mana acts as the primary execution resource token in ICN, replacing gas or fuel-like models with a regenerative, cooperative-aligned mechanism. Each scoped identity (DID, cooperative, community, federation) has an associated mana pool. Actions in the runtime consume mana, and regeneration policies restore it over time.

This system aligns economic fairness with sustainability and trust: high-reputation actors gain increased regeneration or quota ceilings, while abusive or untrusted ones face throttling.

---

## 2. Terminology

* **Mana** – A regenerative unit representing execution capacity.
* **ScopeKey** – Identifier representing the identity level (individual, cooperative, etc.) tied to a mana ledger.
* **ManaLedger** – Tracks balances and regeneration metadata for each ScopeKey.
* **ManaCost** – Total cost of a job execution, recorded in the receipt.
* **RegenerationPolicy** – Policy defining mana restoration behavior over time.

---

## 3. Accounting Model

### 3.1 Mana Cost Evaluation

Each WASM job execution consumes a fixed or policy-adjusted mana cost:

* Defined by CCL contract metadata or execution class
* Recorded in the `ExecutionMetrics.mana_cost` field of the `ExecutionReceipt`
* Subtracted from the originator's ScopeKey via `ManaLedger::spend()`

If the originator has insufficient mana, execution is rejected with `InsufficientBalance`.

### 3.2 Regeneration

Mana regenerates over time according to a `RegenerationPolicy`:

```rust
pub struct RegenerationPolicy {
    pub rate_per_second: u64,
    pub max_mana: u64,
    pub min_regeneration_threshold: Option<u64>,
}
```

Regeneration occurs during:

* Periodic runtime ticks (`Runtime::tick_mana()`)
* On-demand (e.g., before execution) when `mana_ledger.get()` is called

---

## 4. Ledger Implementation

The mana subsystem is backed by implementations of the `ManaLedger` trait:

* **InMemoryManaLedger**: For tests and ephemeral runtimes
* **SledManaLedger**: Persistent ledger using `sled`, supports federation-scale workloads

Each implementation must support:

```rust
pub trait ManaLedger {
    fn get_mana_state(&self, scope: &ScopeKey) -> Result<ManaState>;
    fn update_mana_state(&self, scope: &ScopeKey, new: ManaState) -> Result<()>;
    fn all_dids(&self) -> Result<Vec<Did>>;
}
```

---

## 5. Policy Enforcement

Mana is enforced through the `ResourcePolicyEnforcer` layer. Each action may be:

* **Allowed**: if sufficient mana is available
* **Denied**: if below threshold
* **Rate-limited**: by quota settings

All runtime calls consuming resources must be preceded by `host_account_spend_mana(...)` ABI enforcement.

---

## 6. Runtime Integration

### 6.1 Execution Flow

1. Resolve `ScopeKey` from `ExecutionContext`
2. Check mana balance
3. Execute job
4. Record mana cost
5. Emit `ExecutionReceipt`
6. Deduct mana from `ManaLedger`
7. Submit cost to `icn-reputation`

### 6.2 Host ABI Exposure

```rust
extern "C" {
    fn host_account_get_mana(did_ptr: u32, len: u32) -> i32;
    fn host_account_spend_mana(did_ptr: u32, len: u32, amount: u64) -> i32;
}
```

---

## 7. Reputation Effects

Mana costs are submitted to the reputation service as part of the execution receipt. Higher mana usage increases:

* Weight of successful job completions
* Penalty magnitude on job failures

This creates alignment between computation value and trust scoring.

---

## 8. Rationale and Alternatives

Mana provides a regenerative, scope-aware system more appropriate for cooperative networks than deflationary gas models. Unlike token-based fuel, it avoids wealth-based privilege and enables time-based fairness.

Alternatives considered:

* Static quotas: too rigid and hard to adapt dynamically
* Token-burning: disincentivizes usage and introduces market risk

---

## 9. Backward Compatibility

Mana was introduced in ICN v3. This RFC formalizes the design implemented in `icn-economics`, `icn-runtime`, and `host-abi`. No breaking changes are introduced.

---

## 10. Open Questions and Future Work

* Dynamic regeneration rates based on reputation or role?
* Governance-defined regeneration overrides?
* Visualization and forecast tools for mana usage?

---

## 11. Acknowledgements

Thanks to contributors in runtime, reputation, and economics subsystems who modeled and implemented mana regeneration in the core.

---

## 12. References

* \[RFC 0003: CCL Context and Scope Model]
* \[RFC 0012: Reputation Scoring and Profile Structure]
* \[RFC 0015: Resource Policy Enforcer Design (planned)]
* \[ManaRegenerator & SledManaLedger (source)]

---

**Filename:** `0010-mana-accounting-and-regeneration.md`
