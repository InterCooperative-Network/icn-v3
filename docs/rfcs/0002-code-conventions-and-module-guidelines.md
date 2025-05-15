# RFC 0002: Code Conventions and Module Guidelines

**Status:** Proposed
**Author(s):** Matt Faherty, ICN Technical Core Team
**Date:** 2025-05-14
**Version:** 1.0
**Replaces:** None
**Replaced By:** —
**Related To:** RFC 0000 (RFC Process), RFC 0001 (Project Structure)

---

## 0. Abstract

This RFC defines code conventions and module structure guidelines for the InterCooperative Network (ICN) codebase. It aims to promote consistency, modularity, and readability across all Rust crates, services, and frontend projects. These conventions are intended to support both internal developers and external contributors, ensuring ICN code remains maintainable and navigable as it evolves.

---

## 1. Introduction

As a federated and modular platform, ICN includes dozens of crates and services—each of which may be developed and extended by different cooperative actors. This document provides a unified set of code and module structure conventions to:

* Encourage consistency and reduce cognitive overhead.
* Simplify onboarding and cross-team collaboration.
* Improve discoverability of types, traits, and APIs.
* Support future automation in documentation, testing, and validation pipelines.

These guidelines apply to all Rust components of ICN, including `crates/`, `services/`, and supporting test and tool directories.

---

## 2. Terminology

* **Module** – A Rust file or folder containing related logic under a crate namespace.
* **Crate** – A named Rust library or binary with its own `Cargo.toml`.
* **Service** – A deployable binary crate that runs as a daemon or API server.
* **Component** – Any reusable unit of functionality or interface.

---

## 3. Code Style Conventions

### 3.1 General Style

* Follow [`rustfmt`](https://rust-lang.github.io/rustfmt/) with project defaults.
* Use `clippy` with `#![deny(warnings)]` in CI; fix all lint violations.
* Prefer `snake_case` for variables and functions.
* Prefer `CamelCase` for types, structs, traits, and enums.
* Use `SCREAMING_SNAKE_CASE` for constants.

### 3.2 Naming

* Use descriptive, scoped names: `MeshJobParams`, `ExecutionReceipt`, `DAGAnchorId`.
* Modules should reflect their domain: `receipt_dag.rs`, `mana.rs`, `identity_index.rs`.
* Crate names should be hyphenated and prefixed by domain: `icn-runtime`, `icn-crypto`, `icn-ccl-dsl`.

### 3.3 Comments

* Use `///` for documentation and module-level APIs.
* Use `//` for inline notes; keep them concise.
* Avoid commented-out code in committed branches.

### 3.4 Error Handling

* Prefer structured error enums (`enum AppError`) over strings.
* Use `anyhow::Result` in binaries and `thiserror::Error` in libraries.
* Include context (`.with_context(...)`) on error propagation.

---

## 4. Module Structure Guidelines

### 4.1 Crate Organization

Each crate should have:

```
src/
├── lib.rs          # Top-level entry point (or main.rs for binaries)
├── mod_a.rs        # Feature module A
├── mod_b/          # Feature module B as a folder
│   ├── mod.rs
│   └── subfeature.rs
├── types.rs        # Shared types (if applicable)
├── errors.rs       # Shared error types
├── config.rs       # Config loading/validation
├── tests/          # Optional module-local integration tests
```

### 4.2 Re-exports

* Only re-export submodules at crate root if public API exposure is needed.
* Avoid wildcard imports (`use foo::*`) in production code.
* Use qualified paths when possible for clarity (`crate::types::ScopeKey`).

### 4.3 Feature Flags

* Use named features in `Cargo.toml` to modularize optional components.
* Examples: `sled`, `full_host_abi`, `metrics`, `integration-tests`.
* Keep default features minimal.

### 4.4 Crate Dependencies

* Prefer internal interfaces over exposing entire external crates.
* Avoid deep coupling to unmaintained or unstable crates.
* Document rationale for non-obvious dependencies.

---

## 5. Frontend & Dashboard Conventions

While this RFC focuses on Rust systems, dashboard and wallet code should follow:

* [Next.js](https://nextjs.org/) best practices
* TypeScript + ESLint enforcement
* `src/components/`, `src/pages/`, `src/lib/` structure
* Tailwind for styling; ShadCN UI for base components
* Recharts for time-series and pie chart visualizations

---

## 6. Tooling and CI Expectations

* All crates must pass `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`.
* `cargo deny` should be used to vet licenses and vulnerabilities.
* Docs should build cleanly with `cargo doc`.
* Recommended: use `just` or `make` targets for common tasks.

---

## 7. Rationale and Alternatives

Consistency enables better reviews, less churn, and smoother collaboration across federated teams. These conventions are modeled on Rust ecosystem norms (e.g., Tokio, Serde, Axum) with adjustments for ICN-specific domains.

Alternatives such as looser guidelines or crate-specific conventions were rejected in favor of uniformity and ease of automation.

---

## 8. Backward Compatibility

This RFC codifies conventions already in use across much of the ICN repository. New code should comply. Older code should be refactored over time as needed.

---

## 9. Open Questions and Future Work

* Should we enforce module visibility conventions (e.g., `pub(crate)` by default)?
* Should we standardize crate-level doc comments?
* Add naming conventions for CLI commands and REST endpoints?

---

## 10. Acknowledgements

Thanks to contributors across ICN crates for shaping these conventions through practice. Special thanks to Rust community projects whose patterns were adopted here.

---

## 11. References

* \[RFC 0000: RFC Process and Structure]
* \[RFC 0001: Project Structure and Directory Layout]
* [Tokio Style Guide](https://github.com/tokio-rs/tokio/blob/master/CONTRIBUTING.md)
* [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

---

**Filename:** `0002-code-conventions-and-module-guidelines.md`
