---
RFC: 0014
Title: Economic Observability and Transparency in ICN
Status: Draft
Author: Matt Faherty, ChatGPT
Created: 2025-05-12
Updated: 2025-05-12
---

## Summary

This RFC proposes a complete observability layer for economic behavior within the ICN platform. It defines a standard for tracking, visualizing, and analyzing resource flows, token dynamics, and governance-driven parameter changes. This transparency is critical to trust, accountability, and adaptability.

## Motivation

In traditional systems, economic metrics are often hidden, delayed, or manipulated. In ICN, all economic behavior should be:
- **Real-time observable**
- **Scope-aware (per DID, coop, community)**
- **Linked to verifiable receipts and reputation impact**

This empowers both automated and human oversight.

## Observability Domains

### 1. Mana Metrics
- `icn_mana_balance{did=..., scope=...}`
- `icn_mana_regeneration_rate{did=...}`
- `icn_mana_cooldown_state{did=...}`

### 2. Token Metrics
- `icn_token_balance{token=ICN-F|ICN-NFR, did=...}`
- `icn_token_minted_total`, `icn_token_burned_total`
- `icn_token_decay_events_total`

### 3. Execution Receipts
- Indexed by type: `compute`, `contribution`, `economic-policy-change`
- Visible in dashboards with filtering per scope

### 4. Governance Change Log
- All parameter changes anchored and diffed
- Change history browsable per scope
- Alerts for major shifts (e.g. regen policy lowered)

### 5. Fairness and Reciprocity
- `icn_resource_fairness_index{coop_id=...}`
- `icn_bid_reciprocity_ratio{did=...}`
- Time series of access inequality vs. usage

## Dashboard Features
- Entity profile panels (DID, Coop, Community)
- Mana regeneration curve visualizations
- Token flow graphs
- Live proposal feed with economic diffs
- Reputation vs. access correlation plots

## Implementation Plan

1. Instrument economic code paths with `prometheus` metrics
2. Expose scoped economic telemetry via `/metrics` and internal WebSocket
3. Extend dashboard with scoped economic views
4. Add alerts for anomalies, hoarding, or sudden changes
5. Cross-link receipts and metrics in entity views

## Related RFCs
- RFC 0009: Mana Reform
- RFC 0011: Anti-Extractive Defaults
- RFC 0013: Parameter Governance

## Open Questions
- Should some metrics be privately scoped by default (opt-in transparency)?
- How should we define thresholds for fairness alerts?

## Copyright
Copyright 2025 the ICN Contributors. Licensed under CC-BY-4.0.
