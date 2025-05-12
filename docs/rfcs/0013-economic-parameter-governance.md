---
RFC: 0013
Title: Federated Governance of Economic Parameters via CCL
Status: Draft
Author: Matt Faherty, ChatGPT
Created: 2025-05-12
Updated: 2025-05-12
---

## Summary

This RFC proposes a formal mechanism for governing economic parameters across the ICN platform using **CCL proposals scoped to federations, cooperatives, or communities**. This ensures that key economic behaviors—like mana regeneration, token distribution, and bid weighting—are democratically tunable and traceable.

## Motivation

Hard-coded or centralized economic rules replicate the problems of fiat systems. ICN’s legitimacy depends on:
- Transparent, localized governance over economic rules
- Verifiable, receipt-backed policy enforcement
- The ability for federations and co-ops to adapt their economies to their needs

## Governable Parameters
Parameters to be exposed to governance include:
- `base_mana_pool`, `base_regen_rate`, `mana_cooldown_period`
- `reputation_weight_multiplier`
- `reciprocity_curve_factor`
- `bid_evaluation.weights` (reputation, price, locality)
- Token allocation rules (for ICN-F, ICN-NFR)
- Token decay / expiration rules (if enabled)

## Proposal Format
CCL proposals must:
- Specify the scope (`federation_id`, `coop_id`, `community_id`)
- Include one or more economic parameters to modify
- Pass a configurable **approval threshold**
- Result in an `ExecutionReceipt` with economic effect metadata

### Example
```yaml
proposal_type: ECONOMIC_POLICY_UPDATE
scope:
  coop_id: did:coop:harvest_union
parameters:
  base_mana_pool: 8000
  regen_rate: 0.015
  reciprocity_curve_factor: 1.3
```

## Runtime Enforcement
Upon successful passage:
- `EconomicPolicyManager` updates scoped config entries
- Changes are reflected in mana calculations, bid evaluation, and token flows
- All changes are logged and queryable

## Implementation Plan
1. Extend CCL AST + validator to support `ECONOMIC_POLICY_UPDATE`
2. Add `EconomicPolicyManager` with scoped overrides and history tracking
3. Update `ManaManager`, `BidEvaluator`, and token modules to read from scoped policy
4. Anchor receipts with diff metadata and expose in dashboards

## Open Questions
- Should conflicting scoped policies have resolution priority (e.g. coop vs community)?
- Should economic policy changes be time-delayed for observability?

## Related RFCs
- RFC 0009: Mana Economy Reform
- RFC 0011: Anti-Extractive Defaults
- RFC 0012: Social Minting + Contribution Recognition

## Copyright
Copyright 2025 the ICN Contributors. Licensed under CC-BY-4.0.
