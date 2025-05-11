# ICN CCL WASM Codegen (`icn-ccl-wasm-codegen`)

This crate is responsible for the final stage of the Contract Chain Language (CCL) compilation pipeline: generating executable WebAssembly (WASM) bytecode from the CCL's Domain Specific Language (DSL) Abstract Syntax Tree (AST).

## Purpose

After CCL source code is parsed by `icn-ccl-parser` and lowered into a structured AST by `icn-ccl-compiler` (resulting in `Vec<icn_ccl_dsl::DslModule>` objects from the `icn-ccl-dsl` crate), this `icn-ccl-wasm-codegen` crate takes that AST and transforms it into a compact, efficient WASM module.

This WASM module can then be executed by a WASM runtime, such as the one provided by `icn-core-vm` and orchestrated by `icn-runtime`, allowing the ICN to enforce the rules and logic defined in the original CCL contract.

## Process

1.  **AST Traversal (`WasmGenerator`)**: The `WasmGenerator` walks the input `Vec<DslModule>` AST.
2.  **Opcode Generation**: As it traverses the AST, it converts DSL constructs (like proposals, rules, actions) into a sequence of high-level, custom `Opcode`s defined within this crate (see `src/opcodes.rs`). These opcodes are specific to the ICN execution environment and represent conceptual operations rather than raw WASM instructions directly.
3.  **WASM Emission (`emit::emit_wasm`)**: The `emit_wasm` function takes the generated `Program` (a list of `Opcode`s) and translates it into a standard WASM byte stream. This involves:
    *   Defining WASM function types.
    *   Importing necessary host functions (defined by `host-abi`).
    *   Generating WASM function bodies that implement the logic of the custom opcodes by calling host functions or using WASM instructions.
    *   Exporting a main `run` function for the WASM module.

The output is a `Vec<u8>` containing the WASM bytecode. 