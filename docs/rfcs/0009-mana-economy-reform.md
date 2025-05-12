---
RFC: 0009
Title: Reputation-Governed Mana Economy
Status: Draft
Author: Matt Faherty, ChatGPT
Created: 2025-05-12
Updated: 2025-05-12
---

## Summary

This RFC proposes a shift in ICN’s resource accounting model from a fixed-pool fuel-based approach to a **reputation-governed, regenerating mana system**. The goal is to align resource usage with contribution, discourage extractive behavior, and enable cooperative economic feedback loops.

## Motivation

Current economic systems—on and off-chain—are often based on static balances and artificial scarcity. In ICN, we seek to replace this with an *adaptive*, *trust-based* model where:

- Access to compute and participation is tied to **reputation and role**.
- Mana **regenerates over time**, simulating personal and organizational vitality.
- Abuse is throttled not by fees, but by **reputation-driven decay** and automatic cooldowns.

This creates an economy grounded in **regenerative contribution**, not accumulation.

## Goals

- Establish **mana as a regenerating resource** per identity, coop, or community.
- Link **regeneration rate** to reputation scores and contribution metrics.
- Enable **runtime metering** and enforcement via host ABI.
- Support policy-driven overrides and scoped economic tuning via CCL.

## Design

### Mana Scope
Mana is tracked at the following levels:
- Individual DID
- Cooperative ID
- Community ID

### Regeneration Model
- Each entity has a **base mana pool** (configurable).
- Regeneration occurs per epoch (e.g., hourly), governed by:
  ```text
  regen_rate = base_rate * reputation_modifier * activity_modifier
  ```
- Excessive consumption causes cooldown decay. Low-reputation actors regenerate more slowly.

### Runtime Integration
- `host_account_get_mana(did_ptr, did_len) -> i64`
- `host_account_spend_mana(did_ptr, did_len, amount) -> i32`
- Error codes distinguish between insufficient mana vs. unauthorized spend.

### Governance Hooks
- CCL policy modules can:
  - Adjust base pool and regen rates.
  - Penalize or boost specific behaviors.
  - Propose global or scoped overrides.

## Implementation Plan

1. **Model Regeneration Curve** in `ManaManager`
2. **Cache Reputation Weights** in `icn-reputation`
3. **Runtime ABI Enforcement**
4. **Expose Regen State** via Prometheus + Dashboard
5. **Define CCL Integration API** for policy overrides
6. **RFCs 0010–0012** will define:
   - Regenerative modeling (0010)
   - Anti-extractive defaults (0011)
   - Cooperative recognition + social minting (0012)

## Open Questions
- Should regeneration be paused for sanctioned identities?
- How to weight reputation dimensions (accuracy, volume, diversity)?
- Should regeneration pool scale with cooperative size/activity?

## Prior Art
- Mana in IOTA
- Soulbound token research
- Timebanking economies
- Proof-of-Reputation consensus variants

## Copyright
Copyright 2025 the ICN Contributors. Licensed under CC-BY-4.0.
---
RFC: 0009
Title: Reputation-Governed Mana Economy
Status: Draft
Author: Matt Faherty, ChatGPT
Created: 2025-05-12
Updated: 2025-05-12
---

## Summary

This RFC proposes a shift in ICN’s resource accounting model from a fixed-pool fuel-based approach to a **reputation-governed, regenerating mana system**. The goal is to align resource usage with contribution, discourage extractive behavior, and enable cooperative economic feedback loops.

## Motivation

Current economic systems—on and off-chain—are often based on static balances and artificial scarcity. In ICN, we seek to replace this with an *adaptive*, *trust-based* model where:

- Access to compute and participation is tied to **reputation and role**.
- Mana **regenerates over time**, simulating personal and organizational vitality.
- Abuse is throttled not by fees, but by **reputation-driven decay** and automatic cooldowns.

This creates an economy grounded in **regenerative contribution**, not accumulation.

## Goals

- Establish **mana as a regenerating resource** per identity, coop, or community.
- Link **regeneration rate** to reputation scores and contribution metrics.
- Enable **runtime metering** and enforcement via host ABI.
- Support policy-driven overrides and scoped economic tuning via CCL.

## Design

### Mana Scope
Mana is tracked at the following levels:
- Individual DID
- Cooperative ID
- Community ID

### Regeneration Model
- Each entity has a **base mana pool** (configurable).
- Regeneration occurs per epoch (e.g., hourly), governed by:
  ```text
  regen_rate = base_rate * reputation_modifier * activity_modifier
  ```
- Excessive consumption causes cooldown decay. Low-reputation actors regenerate more slowly.

### Runtime Integration
- `host_account_get_mana(did_ptr, did_len) -> i64`
- `host_account_spend_mana(did_ptr, did_len, amount) -> i32`
- Error codes distinguish between insufficient mana vs. unauthorized spend.

### Governance Hooks
- CCL policy modules can:
  - Adjust base pool and regen rates.
  - Penalize or boost specific behaviors.
  - Propose global or scoped overrides.

## Implementation Plan

1. **Model Regeneration Curve** in `ManaManager`
2. **Cache Reputation Weights** in `icn-reputation`
3. **Runtime ABI Enforcement**
4. **Expose Regen State** via Prometheus + Dashboard
5. **Define CCL Integration API** for policy overrides
6. **RFCs 0010–0012** will define:
   - Regenerative modeling (0010)
   - Anti-extractive defaults (0011)
   - Cooperative recognition + social minting (0012)

## Open Questions
- Should regeneration be paused for sanctioned identities?
- How to weight reputation dimensions (accuracy, volume, diversity)?
- Should regeneration pool scale with cooperative size/activity?

## Prior Art
- Mana in IOTA
- Soulbound token research
- Timebanking economies
- Proof-of-Reputation consensus variants

## Copyright
Copyright 2025 the ICN Contributors. Licensed under CC-BY-4.0.
