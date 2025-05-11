# ICN CCL DSL (`icn-ccl-dsl`)

This crate defines the Domain Specific Language (DSL) Abstract Syntax Tree (AST) for the InterCooperative Network (ICN) Contract Chain Language (CCL).

## Purpose

The CCL is a high-level language designed for specifying governance rules, organizational bylaws, operational policies, and other constitutional documents for decentralized autonomous organizations (DAOs) and cooperatives operating on the ICN.

This `icn-ccl-dsl` crate provides the core, `serde`-friendly Rust structs that represent the parsed and lowered form of CCL. It serves as the intermediate representation (IR) that the `icn-ccl-compiler` produces from raw CCL text. This AST is then used by `icn-ccl-wasm-codegen` to generate executable WASM bytecode.

## Key Structures

*   `DslModule`: An enum representing various top-level CCL constructs (e.g., `Proposal`, `Vote`, `Role`, `ActionHandler`, `Section`).
*   `Rule`: A generic key-value pair structure for defining specific rules and parameters within modules.
*   `RuleValue`: An enum for the various types a rule's value can take (String, Number, Boolean, List, Map, Range, If-expression).
*   `ActionStep`: Defines atomic operations within an `ActionHandler` (e.g., `MeteredAction`, `Anchor` data).

This crate is a foundational piece of the CCL toolchain, enabling the transformation of human-readable contracts into a structured, machine-processable format. 