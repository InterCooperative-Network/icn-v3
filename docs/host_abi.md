# WASM Host ABI

This document describes the host functions available to WebAssembly modules running in the ICN Runtime.

**ABI Version: 1.0**

## Function Reference

### DAG Operations

#### `host_anchor_cid(cid: *const u8, cid_len: usize) -> i32`

Anchors a CID to the DAG, making it part of the immutable record.

- **Parameters:**
  - `cid`: Pointer to UTF-8 encoded string of the CID to anchor
  - `cid_len`: Length of the CID string
- **Returns:** 0 on success, error code on failure
- **Example:**
  ```wat
  (call $host_anchor_cid (i32.const 0) (i32.const 64))
  ```

### Trust Bundle Operations

#### `host_get_trust_bundle(cid: *const u8, cid_len: usize) -> i32`

Retrieves and verifies a trust bundle from the given CID.

- **Parameters:**
  - `cid`: Pointer to UTF-8 encoded string of the trust bundle CID to retrieve
  - `cid_len`: Length of the CID string
- **Returns:** 1 if the trust bundle is valid, 0 if not found, error code on failure
- **Example:**
  ```wat
  (call $host_get_trust_bundle (i32.const 0) (i32.const 64))
  ```

### Memory Operations

#### `host_memory_allocate(size: usize) -> *mut u8`

Allocates memory in the host environment.

- **Parameters:**
  - `size`: Number of bytes to allocate
- **Returns:** Pointer to the allocated memory, or null on failure
- **Example:**
  ```wat
  (call $host_memory_allocate (i32.const 1024))
  ```

#### `host_memory_free(ptr: *mut u8, size: usize) -> i32`

Frees memory previously allocated in the host environment.

- **Parameters:**
  - `ptr`: Pointer to the memory to free
  - `size`: Size of the memory region
- **Returns:** 0 on success, error code on failure
- **Example:**
  ```wat
  (call $host_memory_free (local.get $ptr) (i32.const 1024))
  ```

## Error Codes

- **0**: Success
- **-1**: Generic error
- **-2**: Memory allocation error
- **-3**: Invalid CID
- **-4**: DAG operation error
- **-5**: Trust validation error 