# RFC 0001: ICN Project Structure and Directory Layout

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0000 (RFC Process), RFC 0002 (Code Conventions), RFC 0003 (CCL Context Model)

---

## 0. Abstract

This RFC defines the standardized project structure for the InterCooperative Network (ICN) monorepo. It documents the purpose and scope of each top-level directory, clarifies conventions for crate placement, service boundaries, configuration, and test suites. This structure is designed to support modularity, reproducibility, and developer onboarding.

Repository: [https://github.com/InterCooperative-Network/icn-v3](https://github.com/InterCooperative-Network/icn-v3)

---

## 1. Introduction

The ICN is implemented as a unified multi-crate Rust monorepo, with supporting services, frontend components, documentation, and tooling. A clear, consistent directory structure ensures long-term maintainability, efficient collaboration, and simplified deployment.

This document formalizes the structure currently in use across the ICN repository ([icn-v3](https://github.com/InterCooperative-Network/icn-v3)) and provides rationale for its design.

---

## 2. Terminology

* **Crate** – A Rust module compiled into a library or binary.
* **Service** – A standalone backend component exposing an API or runtime behavior.
* **Workspace** – A logical grouping of crates defined in a shared `Cargo.toml`.
* **Devnet** – A local or containerized test network simulating federation behavior.

---

## 3. Project Structure

```
/                      # Root workspace
├── Cargo.toml         # Workspace manifest
├── crates/            # Core libraries and runtime components
│   ├── ccl/           # Cooperative Contract Language toolchain
│   ├── common/        # Shared logic: types, crypto, identity, economics
│   ├── p2p/           # Mesh protocol and peer networking
│   └── runtime/       # WASM runtime, core VM, and host ABI
├── services/          # Independently deployable backend services
│   ├── icn-agoranet/  # Proposal deliberation and governance API
│   ├── icn-reputation/ # Reputation computation and scoring API
│   └── icn-mesh-jobs/ # Job management, bidding, and lifecycle control
├── dashboard/         # Next.js frontend dashboard
├── wallet/            # Progressive Web App (PWA) for user key management
├── devnet/            # Local federation simulation
│   ├── configs/       # TOML files for federation, coop, and node setup
│   └── scripts/       # Bootstrap scripts and helper tooling
├── scripts/           # Generic CLI or shell helpers
├── tests/             # Integration and system-level tests
├── monitoring/        # Prometheus, Grafana, and alerting configuration
├── content/           # Markdown for documentation, pages, blog
├── docs/              # Technical design documents, diagrams, and RFCs
│   ├── rfcs/          # Official Request for Comments (RFCs)
│   └── architecture/  # Static diagrams and architectural notes
└── .github/           # CI/CD workflows, PR templates, contribution guides
```

---

## 4. Crates Breakdown

### `crates/ccl/`

Contains:

* `icn-ccl-parser`: Pest-based grammar for CCL DSL.
* `icn-ccl-dsl`: Typed AST for cooperative logic.
* `icn-ccl-compiler`: Compiles DSL to WASM.
* `icn-ccl-wasm-codegen`: Final WASM emitter.
* `ccl_std_env`: ABI utilities accessible within CCL contracts.

### `crates/common/`

Contains foundational modules:

* `icn-types`: Core data models and enums.
* `icn-crypto`: Signature utilities and hashing.
* `icn-identity`: DID-based trust model and scope resolution.
* `icn-economics`: Mana regeneration, enforcement, and quotas.
* `icn-mesh-protocol`: Libp2p protocol definitions.
* `icn-mesh-receipts`: ExecutionReceipt anchoring and DAG logic.

### `crates/p2p/`

* `planetary-mesh`: Libp2p-based mesh communication.

### `crates/runtime/`

* `icn-runtime`: Executes mesh jobs, enforces policy, emits receipts.
* `icn-core-vm`: WASM sandbox and syscall integration.
* `host-abi`: Defines the interface between host and guest.

---

## 5. Services Breakdown

### `services/icn-agoranet`

Provides a governance API:

* Proposal submission
* Voting
* WebSocket-based event channels

### `services/icn-mesh-jobs`

Manages the job lifecycle:

* Job creation and bidding
* Assignment protocol
* Receipt validation and metrics

### `services/icn-reputation`

Manages:

* Execution scoring
* Profile aggregation
* Mana update submission

---

## 6. Frontend and Wallet

### `dashboard/`

* React + Next.js app
* Visualizations for DAG, jobs, tokens, and reputation
* Uses Recharts and REST/WebSocket integration

### `wallet/icn-wallet-pwa/`

* Secure browser wallet
* Local key management and signing
* Connects to AgoraNet and runtime endpoints

---

## 7. Devnet and CI Tooling

### `devnet/`

* Federation-wide simulation environment
* Bootstrap templates for:

  * Federations
  * Cooperatives
  * Communities
  * Runtime nodes

### `.github/`

* GitHub Actions for testing, linting, deploy
* Contribution guidelines and PR templates

---

## 8. Rationale and Alternatives

This structure promotes:

* **Modularity:** Distinct crates for core, services, and extensions.
* **Reusability:** Shared crates prevent duplication.
* **Developer Experience:** Clear entry points for CLI, wallet, and UI.
* **Observability:** Integration of monitoring and metrics at all levels.

Alternatives like multiple repositories were considered but rejected due to cohesion, coordination cost, and the tight coupling between federation components.

---

## 9. Backward Compatibility

This document formalizes existing conventions. Deviations may be refactored in future RFCs but should maintain consistency with this layout until then.

---

## 10. Open Questions and Future Work

* Should services be versioned independently or together?
* Should DAG replication or proposal metadata move into dedicated crates?
* Can `devnet/` evolve into a stable testnet deployment tool?

---

## 11. Acknowledgements

Thanks to the ICN engineering team, contributors, and community members who helped evolve this structure through practical development.

---

## 12. References

* \[RFC 0000: RFC Process and Structure]
* \[RFC 0003: CCL Context Model (planned)]
* \[RFC 0016: Mesh Execution Pipeline (planned)]
* [ICN GitHub Repository](https://github.com/InterCooperative-Network/icn-v3)

---

**Filename:** `0001-project-structure.md`
