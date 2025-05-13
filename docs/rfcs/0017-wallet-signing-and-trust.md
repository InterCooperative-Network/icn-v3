---

RFC: 0017
Title: Wallet Signing and Trust Boundaries in ICN
Author: Matt Faherty
Date: 2025-05-12
Status: Draft

# Summary

This RFC defines the signing model and trust boundaries for the ICN Wallet. It specifies how identities interact with ICN components using signed payloads, how signing context is scoped, and what guarantees are assumed across local, web, and P2P boundaries.

# Motivation

To enable verifiable, user-driven participation in ICN governance and execution, signatures must be:

* Authenticated by the correct key material.
* Scoped to relevant DIDs and actions.
* Validated consistently across runtime, services, and browser environments.

This RFC ensures:

* Secure operation of the ICN Wallet (PWA or native).
* Valid cross-component signature verification.
* Predictable boundaries for capability delegation.

# Key Concepts

* **DID (Decentralized Identifier)**: All actors are identified by a DID.
* **Scope Key**: Defines operational authority (e.g., individual, coop, community).
* **Wallet Environment**: The software handling key material and signing (PWA, CLI, native).

# Signing Model

## Message Format

All signed messages conform to JWS compact serialization, with payloads JSON-encoded.

## Signing Payload

The payload MUST contain:

```json
{
  "did": "did:icn:alice",
  "scope": "coop:buildersguild",
  "timestamp": 1715522047,
  "action": {
    "type": "submit_proposal",
    "data": { ... }
  }
}
```

## Signature Flow

1. ICN Wallet prepares the payload.
2. Local key is used to produce JWS.
3. Signed message is submitted to:

   * Runtime (via host call)
   * ICN service API (e.g., Agoranet, Mesh Jobs)
   * P2P announcement (optional)

## Scope Enforcement

* Wallets MUST include `scope` in every payload.
* Verifiers (runtime/services) MUST resolve `ScopeKey` and ensure action aligns with declared scope.

# Trust Boundaries

## Wallet ➝ Browser

* Local-only storage of key material (WebCrypto, IndexedDB).
* No external transmission of private keys.
* Signature performed inside PWA or browser extension sandbox.

## Wallet ➝ Runtime

* Host ABI calls accept signed inputs (e.g., `submit_mesh_job`).
* Runtime validates signature, extracts DID + scope.
* Receipt anchors include signer metadata.

## Wallet ➝ Services

* HTTP headers or JSON bodies include signed payloads.
* Services validate signature and replay-protect via timestamp or nonce.
* Trusted timestamp windows configurable per service.

# Replay Protection

* Every signed action must include a timestamp.
* Services SHOULD enforce max drift (e.g., 30 seconds).
* Nonce or derived hash can be used to prevent replay across contexts.

# Multi-Key Support

* Wallets MAY support multiple DIDs (e.g., personal, coop, node).
* Active identity must be explicitly selected.
* Signature delegation (future) will use capability tokens.

# Testing and Validation

* Unit tests MUST validate:

  * Signature verification from WebCrypto and native clients.
  * ScopeKey resolution and rejection of mismatched scopes.
* Integration tests MUST cover:

  * Proposal submission.
  * Mesh job submission.
  * Reputation update anchor verification.

# Future Work

* Capability-based delegation tokens (ZCAP-LD).
* Encrypted key export/import between devices.
* In-wallet receipt verification and local DAG inspection.

---
