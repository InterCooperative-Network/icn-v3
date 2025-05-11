# CCL (Contract Chain Language) Compilation Pipeline

This document outlines the compilation pipeline for the InterCooperative Network's Contract Chain Language (CCL). CCL is used to define governance rules, policies, and constitutional documents for DAOs and cooperatives on the ICN.

The pipeline transforms human-readable CCL text into executable WebAssembly (WASM) bytecode.

## Pipeline Stages

```mermaid
graph LR
    A[CCL Source Text (.ccl)] --> B{icn-ccl-parser};
    B -- Pest Grammar --> C{Pest Parse Tree};
    C --> D{icn-ccl-compiler (lower.rs)};
    D -- Lowers To --> E[CCL DSL AST (Vec<DslModule>)];
    E --> F{icn-ccl-wasm-codegen};
    F -- Generates Custom Opcodes --> G[ICN Program (Vec<Opcode>)];
    G -- Emits WASM --> H[WASM Bytecode (.wasm)];
    H --> I{icn-core-vm (via icn-runtime)};
    I -- Executes with Host ABI --> J[Runtime Behavior / State Changes];
```

**1. CCL Source Text (`.ccl` file)**
   - Input: Human-readable text file written in CCL syntax.
   - Example: `election.ccl`, `budget.ccl`.

**2. Parsing (`icn-ccl-parser`)**
   - Crate: `crates/ccl/icn-ccl-parser`
   - Action: Takes the raw CCL string and parses it based on a formal grammar defined in `ccl.pest` (using the Pest parser generator).
   - Output: A Concrete Syntax Tree (CST) or Parse Tree, specifically a `pest::iterators::Pairs<'_, Rule>`. This tree represents the grammatical structure of the input CCL.

**3. Lowering to DSL AST (`icn-ccl-compiler`)**
   - Crate: `crates/ccl/icn-ccl-compiler` (specifically the `lower.rs` module).
   - Action: Traverses the Pest parse tree and transforms (lowers) it into a more abstract and structured representation: the CCL Domain Specific Language (DSL) Abstract Syntax Tree (AST).
   - Output: A `Vec<icn_ccl_dsl::DslModule>`. The `DslModule` and related structs are defined in the `icn-ccl-dsl` crate. This AST is `serde`-friendly and easier to work with programmatically than the raw parse tree.

**4. WASM Codegen - Step 1: DSL AST to Custom Opcodes (`icn-ccl-wasm-codegen`)**
   - Crate: `crates/ccl/icn-ccl-wasm-codegen`
   - Action: The `WasmGenerator` walks the `Vec<DslModule>` AST.
   - As it traverses, it converts DSL constructs into a sequence of high-level, custom `Opcode`s (defined in `icn-ccl-wasm-codegen/src/opcodes.rs`). These are not raw WASM instructions but rather ICN-specific conceptual operations (e.g., `CreateProposal`, `MintToken`, `CallHost`).
   - Output: An `icn_ccl_wasm_codegen::opcodes::Program` object, which is essentially a list of these custom `Opcode`s.

**5. WASM Codegen - Step 2: Custom Opcodes to WASM Bytecode (`icn-ccl-wasm-codegen`)**
   - Crate: `crates/ccl/icn-ccl-wasm-codegen` (specifically the `emit.rs` module).
   - Action: The `emit_wasm` function takes the `Program` (list of custom `Opcode`s) and translates it into a standard WASM byte stream. This involves:
       - Defining WASM function types and sections.
       - Declaring imports for necessary host functions (defined by `host-abi`).
       - Generating WASM function bodies that implement the logic of the custom opcodes, often by calling the imported host functions or using basic WASM instructions for control flow.
       - Exporting a main `run` function for the WASM module.
   - Output: `Vec<u8>` containing the final WASM bytecode.

**6. Execution (`icn-runtime` with `icn-core-vm`)**
   - Crates: `crates/runtime/icn-runtime`, `crates/runtime/icn-core-vm`
   - Action: The `icn-runtime` orchestrates the execution. It uses the `CoVm` (Cooperative Virtual Machine) from `icn-core-vm` to load and run the compiled WASM module.
   - The `CoVm` links the WASM module's imported host functions to their actual implementations provided by the `icn-runtime` (which implements `MeshHostAbi` from `host-abi`).
   - Output: The WASM module executes, interacting with the host environment via the ABI, potentially leading to state changes in the ICN, resource accounting, etc.

## Key Crates Involved:

*   **`icn-ccl-parser`**: Handles parsing CCL text into a parse tree using Pest.
*   **`icn-ccl-dsl`**: Defines the Rust structs for the Domain Specific Language AST.
*   **`icn-ccl-compiler`**: Orchestrates the lowering from parse tree to DSL AST.
*   **`icn-ccl-wasm-codegen`**: Converts the DSL AST into executable WASM bytecode via custom opcodes.
*   **`host-abi`**: Defines the interface (Rust trait and FFI bindings) between WASM guest modules and the ICN host.
*   **`icn-core-vm`**: Provides the `CoVm` WASM execution engine (using `wasmtime`).
*   **`icn-runtime`**: Implements the host side of the ABI and uses `icn-core-vm` to run compiled CCL contracts. 