---
RFC: 0015
Title: Scoped Mana Accounting and Enforcement in ICN Runtime
Status: Draft
Author: Matt Faherty, ChatGPT
Created: 2025-05-12
Updated: 2025-05-12
---

## Summary

This RFC defines how **mana accounting and enforcement** should function within the ICN runtime to support **scoped resource policies**. Each compute action must be tied to a subject scope (DID, cooperative, or community) and metered accordingly. This enables decentralized, policy-aligned governance over execution behavior.

## Motivation

ICN allows multiple types of actors to initiate and execute jobs: individual identities, cooperatives, communities, or federations. Therefore:
- Mana must be accounted **per initiating scope**.
- Execution must be **blocked or throttled** if mana is insufficient.
- **Delegation and permissioning** should be supported explicitly.

## Scope Hierarchy
Mana accounting is layered as follows:
- **Primary**: `did:icn:...` (individual)
- **Optional Scopes**: `coop_id`, `community_id`

Scopes are declared explicitly in the runtime environment context via:
```rust
VmContext {
    caller_did: Did,
    coop_id: Option<Did>,
    community_id: Option<Did>,
    ...
}
```

## Enforcement Logic

When a WASM module executes:
1. Extract the metering subject (e.g. `caller_did`, fallback to `coop_id`)
2. Lookup mana balance via `ManaManager`
3. Attempt deduction via:
```rust
host_account_spend_mana(did_ptr, did_len, amount)
```
4. Trap execution if insufficient mana

### Policy-Aware Metering
- If the execution is **system-critical**, allow policy override.
- If `coop_id` has an active mana subsidy policy, allow fallback charging.
- Federation-level exemptions must be explicitly granted via governance.

## Implementation Plan

1. Extend `ConcreteHostEnvironment` to select correct scope subject
2. Modify `host_account_spend_mana` to accept layered fallback logic
3. Register scope-aware functions in `wasm/linker_legacy_impl.rs`
4. Integrate with `ManaManager` for multi-scope lookup and deduction
5. Add unit tests for scope enforcement edge cases

## Security Considerations
- Prevent scope impersonation (e.g. claiming another coop's DID)
- Ensure scope rules are cryptographically enforced and anchored

## Related RFCs
- RFC 0009: Mana Economy
- RFC 0010: Regeneration Modeling
- RFC 0013: Federated Economic Parameter Governance

## Copyright
Copyright 2025 the ICN Contributors. Licensed under CC-BY-4.0.
