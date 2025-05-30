# ICN Core VM (`icn-core-vm`)

This crate provides the core WebAssembly (WASM) Virtual Machine, named `CoVm` (Cooperative Virtual Machine), for the InterCooperative Network (ICN).

## Purpose

The `CoVm` is responsible for executing WASM modules, particularly those generated by the CCL (Contract Chain Language) toolchain (e.g., from `icn-ccl-wasm-codegen`). These WASM modules contain the logic for governance rules, policies, and other constitutional contracts.

The VM provides a sandboxed execution environment for these WASM modules, interfacing with the broader ICN system through a defined Host ABI (Application Binary Interface), specified in the `host-abi` crate.

## Key Features

*   **WASM Execution**: Uses the `wasmtime` engine to load, validate, and run WASM bytecode.
*   **Host Function Linking**: Provides mechanisms to link host-defined functions (implementing the `MeshHostAbi` from the `host-abi` crate) into the WASM module's import set. This allows WASM modules to interact with the underlying ICN system (e.g., to log messages, anchor data to a DAG, interact with economic primitives).
*   **Resource Limiting**: Implements basic resource limiting (e.g., fuel/gas for execution steps) to prevent runaway computations and ensure fair resource usage. The `ExecutionMetrics` struct tracks resource consumption.
*   **Host Context**: Manages a `HostContext` that can store state or provide capabilities accessible to host functions during WASM execution.
*   **Error Handling**: Defines `CoVmError` for reporting issues during WASM loading, instantiation, or execution.

## Usage

The `icn-runtime` crate typically instantiates and uses the `CoVm` to execute specific jobs or contract calls, providing the necessary WASM bytecode and a `HostContext` configured with the appropriate host ABI implementation. 