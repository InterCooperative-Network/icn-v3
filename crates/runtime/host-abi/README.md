# ICN Host ABI (`host-abi`)

This crate defines the Application Binary Interface (ABI) for communication between WASM modules (typically compiled from CCL - Contract Chain Language) and the ICN host runtime environment.

## Components

1.  **`MeshHostAbi` Trait (`src/lib.rs`)**:
    *   This Rust trait defines the high-level, idiomatic Rust interface that the host environment (e.g., `icn-runtime`) implements.
    *   It uses standard Rust types, `Result` for error handling, and clear method signatures.
    *   It represents the **complete conceptual ABI** that could be exposed to WASM modules.

2.  **FFI Bindings (`src/bindings.rs`)**:
    *   This file contains `extern "C"` function declarations that constitute the actual Foreign Function Interface (FFI) layer.
    *   These functions are what WASM modules will declare as imports.
    *   Signatures use C-compatible types (raw pointers, integers for results/handles).
    *   This represents the **currently implemented FFI subset** of the `MeshHostAbi`.

3.  **Data Structures (`src/lib.rs`)**:
    *   Shared enums and structs (e.g., `AbiQueryType`, `AbiResourceType`, `FFIError`) used for passing complex data across the FFI boundary, often serialized or passed by reference.

## ABI Function Exposure

The `icn-runtime` crate is responsible for linking implementations of these FFI functions to the WASM modules it executes.

### Currently Implemented and Linked FFI Functions (via `bindings.rs` and `icn-runtime/src/wasm/linker.rs`):

*   `host_log_message(ptr: u32, len: u32) -> i32`
*   `host_anchor_to_dag(cid_ptr: u32, cid_len: u32, data_ptr: u32, data_len: u32) -> i32`
*   `host_query_dag_node(query_type: i32, query_ptr: u32, query_len: u32, result_buf_ptr: u32, result_buf_len: u32) -> i32` (Length of actual result, or error code)
*   `host_check_resource_authorization(resource_type: i32, amount: u64) -> i32` (1 for authorized, 0 for not, <0 for error)
*   `host_record_resource_usage(resource_type: i32, amount: u64) -> i32`
*   `host_get_identity_did(result_buf_ptr: u32, result_buf_len: u32) -> i32` (Length of DID string, or error code)
*   `host_is_governance_context() -> i32` (1 for true, 0 for false)
*   `host_mint_token(token_type_ptr: u32, token_type_len: u32, amount: u64, recipient_did_ptr: u32, recipient_did_len: u32) -> i32`
*   `host_transfer_token(token_type_ptr: u32, token_type_len: u32, amount: u64, sender_did_ptr: u32, sender_did_len: u32, recipient_did_ptr: u32, recipient_did_len: u32) -> i32`
*   `host_submit_job(wasm_cid_ptr: u32, wasm_cid_len: u32, params_ptr: u32, params_len: u32, job_id_buf_ptr: u32, job_id_buf_len: u32) -> i32` (Length of Job ID, or error code)
*   `host_get_job_result(job_id_ptr: u32, job_id_len: u32, result_buf_ptr: u32, result_buf_len: u32) -> i32` (Length of result, or error code)

### Conceptual/Planned ABI Functions (in `MeshHostAbi` or `ccl_std_env` but not yet in `bindings.rs` or linked):

These functions are part of the broader vision for the Host ABI but are not yet exposed to WASM modules:

*   `get_current_time_unix_epoch()`: To allow WASM modules to get a sense of current time.
*   `resolve_did_document(did_ptr: u32, did_len: u32, result_buf_ptr: u32, result_buf_len: u32)`: For more complex DID interactions.
*   Other potential cryptographic helper functions.

The `ccl_std_env/src/abi_helpers.rs` file also provides conceptual Rust traits that mirror some of these host functions from the perspective of a standard library a CCL-compiled WASM module might use. 