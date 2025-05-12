---
RFC: 0011
Title: Anti-Extractive Defaults in the ICN Economy
Status: Draft
Author: Matt Faherty, ChatGPT
Created: 2025-05-12
Updated: 2025-05-12
---

## Summary

This RFC introduces default mechanisms within the ICN economic engine that actively resist extractive behavior—such as hoarding, freeloading, and centralization of resource access. By designing the economy to be *regenerative*, *reciprocal*, and *mutualistic* by default, we foster long-term resilience and cooperative incentive alignment.

## Motivation

In traditional systems, wealth accumulation leads to **power consolidation** and **access monopolies**. To prevent this within ICN:
- Mana accumulation must yield **diminishing utility**.
- Access should favor **reciprocity and rotation**, not perpetual consumption.
- Participants must be **encouraged to reinvest, not extract**.

These constraints must be enforced not through punitive rules, but through *graceful defaults* embedded in the mana economy.

## Anti-Extractive Mechanisms

### 1. Diminishing Returns on Mana Hoarding
- As a DID’s mana pool approaches its cap, **regeneration slows nonlinearly**.
- Hoarded mana contributes **less** to compute access weight over time.

### 2. Reciprocal Participation Requirement
- Introduce **reciprocity scoring**: track outgoing vs. incoming job flow.
- Nodes with poor reciprocity get **deprioritized** in bid evaluation and mana regen.

### 3. Burst Penalty
- Excessive consumption over a short time window triggers **decay throttle** and cooldown.
- Prevents resource spike abuse.

### 4. Role-Based Decay Tuning
- Allow scoped override of decay logic per role (e.g. validators, mentors, stewards).
- Community policy can **bias in favor of high-contribution roles**.

### 5. Fair Rotation Enforcement
- Extend bid evaluation to bias toward **underutilized nodes** when reputation is similar.
- Helps prevent resource monopolization.

## Implementation Plan

1. Add **reciprocity tracker** to `ManaManager`.
2. Integrate diminishing returns curve into regeneration logic.
3. Extend bid evaluation with `recent_usage_score`.
4. Define role-tunable decay parameters in CCL.
5. Add dashboard view: “Resource Fairness Index” per community.

## Open Questions
- Should reciprocity metrics be scoped to jobs, tokens, or bandwidth?
- Should freeloaders trigger coop/community-level alerts?

## Related RFCs
- RFC 0009: Reputation-Governed Mana Economy
- RFC 0010: Regenerative Mana Modeling
- RFC 0012: Recognition of Diverse Contributions

## Copyright
Copyright 2025 the ICN Contributors. Licensed under CC-BY-4.0.
