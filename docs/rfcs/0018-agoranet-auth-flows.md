---

RFC: 0018
Title: Agoranet Authentication and Role Assertion Flows
Author: Matt Faherty
Date: 2025-05-12
Status: Draft

# Summary

This RFC defines the authentication, identity binding, and role assertion flows within Agoranet, the deliberation and proposal coordination layer of ICN. It formalizes how users authenticate via signed payloads, how cooperative/community roles are asserted, and how policies gate access to proposal types or governance actions.

# Motivation

Agoranet enables democratic coordination of proposals, discussion, and voting. To protect this process, we must:

* Bind actions to cryptographically verified identities.
* Ensure roles (e.g., steward, treasurer) are properly scoped.
* Enforce access and decision policies across federated contexts.

# Auth Flow Overview

1. **Login Challenge**: Agoranet issues a `nonce` to the user.
2. **Signed Response**: The wallet signs a payload containing the DID, nonce, and timestamp.
3. **Session Issued**: On success, Agoranet establishes a temporary session and resolves DID roles.

## Auth Request Example

```json
{
  "did": "did:icn:alice",
  "nonce": "xyz123",
  "timestamp": 1715528888,
  "scope": "coop:buildersguild"
}
```

# Role Resolution

Upon successful auth, Agoranet performs:

* Trust bundle verification of the presented DID.
* Role lookup within the cooperative or community scope.
* Assignment of local session privileges (e.g., can propose, can vote).

## Session State

```json
{
  "did": "did:icn:alice",
  "roles": ["steward", "member"],
  "scope": "coop:buildersguild",
  "expires_at": 1715532488
}
```

# Role Assertion

Roles are not blindly trusted from the client. Agoranet:

* Validates active role credentials against on-chain or anchored proofs.
* Caches verified roles for the session duration.
* Invalidates sessions when role credentials are revoked or expire.

# Policy Enforcement

Each proposal type (e.g., election, bylaw change) may define:

* Required role(s) to submit.
* Quorum thresholds by role composition.
* Notification or delay periods gated by role assertions.

# WebSocket & Real-Time Integration

* Proposal rooms enforce role-gated participation.
* Users join as verified identities.
* Live events (votes, amendments) are broadcast with role metadata.

# Audit and Replay Protection

* Every proposal, comment, and vote is signed with the originator DID.
* All actions are anchored into the cooperative DAG.
* Timestamps and nonces prevent replay across sessions.

# Federation Context

* Agoranet instances MAY federate to coordinate cross-coop votes.
* Role verification is always scoped — no global admin exists.
* Federation-wide proposals must resolve roles in multiple bundles.

# Future Work

* Anonymous participation with credential-based selective disclosure.
* Offline proposal staging and later anchoring.
* Session recovery via encrypted re-auth tokens.

---
