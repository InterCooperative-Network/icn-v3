---
RFC: 0010
Title: Regenerative Mana Modeling and Curves
Status: Draft
Author: Matt Faherty, ChatGPT
Created: 2025-05-12
Updated: 2025-05-12
---

## Summary

This RFC formalizes the mathematical and behavioral modeling of **regenerative mana curves** in the ICN economy. Mana regeneration must be adaptive, reputation-aware, and discourage hoarding or overconsumption. The shape of the curve determines the felt incentives of participation.

## Motivation

Rather than relying on rigid scarcity or arbitrary quotas, ICN’s economy should:
- **Encourage steady, diverse participation**.
- **Reward positive contributions** with increased regeneration.
- **Discourage abuse** through soft throttling, not punitive burns.

This requires careful modeling of the mana **regeneration function** and **decay behavior**.

## Regeneration Function

Let `R(t)` be the mana regeneration rate at time `t`. We define:
```text
R(t) = B * f(rep_score) * g(activity) * h(context)
```
Where:
- `B` = base regen rate (per identity scope)
- `f(rep_score)` = reputation modifier (e.g. 0.1 to 2.0)
- `g(activity)` = participation modifier (e.g. exponential decay if idle)
- `h(context)` = policy-based modifier (e.g. emergency mode, co-op boost)

### Suggested f(rep_score) Curve (Piecewise Linear):
```text
0.0 ≤ score ≤ 0.2 → regen = 0.1x
0.2 < score ≤ 0.5 → regen = linear(0.1x → 1.0x)
0.5 < score ≤ 0.8 → regen = linear(1.0x → 1.5x)
0.8 < score ≤ 1.0 → regen = 2.0x
```

## Decay Behavior

When mana is overspent or burst rapidly:
- Apply a **cooldown curve**:
```text
decay(t) = M * exp(-k * t)
```
Where `M` is the overspent amount and `k` is a system-wide cooldown constant.
- Cooldown also pauses regeneration for a period.

## Visualization Examples
Graphs should be included to show curve shapes and typical regeneration scenarios for:
- High vs. low reputation actors
- Active vs. idle cooperatives
- Participants with burst-heavy behavior

## Implementation Plan

1. Add curve computation to `ManaManager`.
2. Load curve parameters from config or CCL.
3. Add Prometheus metric: `icn_mana_regen_rate{did=..., scope=...}`
4. Add dashboard panels for regen analysis.
5. Add unit tests for curve shape correctness.

## Related RFCs
- RFC 0009: Reputation-Governed Mana Economy
- RFC 0011: Anti-Extractive Economic Defaults

## Copyright
Copyright 2025 the ICN Contributors. Licensed under CC-BY-4.0.
