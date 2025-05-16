pub mod bindings;

// Export all bindings at the crate root for easy access
pub use bindings::*;

pub mod error;
pub use error::HostAbiError;

// pub const ICN_HOST_ABI_VERSION: u32 = 8; // bump from 7 → 8 for mesh job submission ABI change

// InterCooperative Network (ICN) - Host ABI Definitions
// This crate defines the Application Binary Interface (ABI) that WASM modules (e.g., CCL contracts)
// use to interact with the ICN host runtime environment. It specifies the functions,
// data structures, and error codes for this interaction.

// Using core::ffi::c_void for potential opaque handles in the future, though not strictly used by current function signatures.
// use core::ffi::c_void;
// use async_trait::async_trait; // Already removed by user
use serde::Serialize;
// No wasmer imports needed

// Need Display for Trap::new
// use std::fmt;
// use wasmtime::Trap; // Already commented, confirmed unused by new compiler output
// use thiserror::Error; // HostAbiError is now in error.rs
// use wasmtime::{Caller, Linker};
// use anyhow::Error as AnyhowError; // Seems unused by MeshHostAbi
use async_trait::async_trait;
use wasmtime::Caller;

// Corrected import: only include types that exist in icn_types::mesh
use icn_types::mesh::{
    JobStatus, MeshJobParams, // OrgScopeIdentifier, StageInputSource, WorkflowType, // These were unused
};
// use wasmtime::Memory; // Commenting out as per new compiler warning
// use std::sync::Arc; // This seems to be unused now.
// use wasmtime::AsContextMut; // Commenting out as per new compiler warning
// use tracing::{error}; // Assuming tracing is not used directly in this file

use std::collections::HashMap;
// use std::convert::TryInto; // Unused
use std::ffi::CStr; // CString was unused
use std::os::raw::{c_char}; // c_int, c_void were unused
use std::ptr;
use std::slice;
use std::str;

// --- Helper Enums & Structs for ABI Communication (other than HostAbiError) ---

/// Placeholder for JobPermissions if not defined in icn_types::mesh
#[derive(Debug, Clone, Default)]
pub struct JobPermissions {} // Defined a placeholder

/// Specifies the type of data contained in a `ReceivedInputInfo` structure,
/// indicating whether interactive input is provided inline or as a CID.
#[repr(u32)] // Ensures stable representation across the ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ReceivedInputType {
    InlineData = 0, // The data is provided directly after ReceivedInputInfo.
    Cid = 1, // The data provided after ReceivedInputInfo is a CID string pointing to the actual input.
}

/// Information about received interactive input.
/// This struct is written by `host_interactive_receive_input` into the WASM module's buffer.
/// The actual payload data (if inline) or the CID string (if by CID)
/// immediately follows this struct in the same buffer.
#[repr(C)] // Ensures C-compatible memory layout for predictable ABI interaction.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ReceivedInputInfo {
    /// Type of the received input (InlineData or Cid).
    pub input_type: ReceivedInputType, // Effectively u32 due to #[repr(u32)] on ReceivedInputType.
    /// Length (in bytes) of the actual data or CID string that follows this struct in the buffer.
    pub data_len: u32,
}

/// Defines log levels for messages sent via `host_log_message`.
#[repr(u32)] // Ensures stable representation across the ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error = 0, // Critical errors that might terminate or corrupt the job.
    Warn = 1,  // Warnings about potential issues that don't necessarily halt execution.
    Info = 2,  // Informational messages about normal operation.
    Debug = 3, // Detailed debugging information for developers.
    Trace = 4, // Highly verbose trace information, for deep debugging.
}

/// Represents the status of a job execution.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum P2PJobStatus {
    Pending = 0,
    InProgress = 1,
    Completed = 2,
    Failed = 3,
    Cancelled = 4,
    Unknown = 5, // Should not happen
}

impl From<JobStatus> for P2PJobStatus {
    fn from(status: JobStatus) -> Self {
        match status {
            JobStatus::InProgress => P2PJobStatus::InProgress,
            JobStatus::Completed => P2PJobStatus::Completed,
            JobStatus::Failed => P2PJobStatus::Failed,
            JobStatus::Cancelled => P2PJobStatus::Cancelled,
            // Assuming JobStatus has other variants that might map to Pending or Unknown
            _ => P2PJobStatus::Unknown, // Or handle exhaustively
        }
    }
}

/// Represents a CID (Content Identifier) for use across the ABI.
/// CIDs are passed as null-terminated strings.
pub type AbiCid = *const c_char;

/// Represents generic binary data passed across the ABI.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AbiBytes {
    pub ptr: *const u8,
    pub len: u32,
}

// This is a simplified version. The actual one might be more complex and involve Arcs/Mutexes.
pub struct MinimalJobContext {
    pub job_id: String,
    pub originator_did: String,      // Assuming DID is a string here
    pub permissions: JobPermissions, // Using the local placeholder
    pub workflow_params: Option<MeshJobParams>, // Changed from WorkflowDefinition to MeshJobParams
    pub current_stage_index: Option<usize>,
    pub stage_outputs: HashMap<String, String>, // Stage ID to output CID
    pub interactive_input_buffer: Option<Vec<u8>>,
    pub interactive_output_buffer: Option<Vec<u8>>,
}

/// Maximum size of data that can be directly inlined in a P2P message payload.
pub const INLINE_PAYLOAD_MAX_SIZE: usize = 1024; // 1KB, example value

/// Max number of bytes that can be peeked from interactive input buffer
pub const MAX_INTERACTIVE_INPUT_BUFFER_PEEK: usize = 256;

/// Trait defining the Host ABI functions callable from WASM modules.
///
/// # Error Handling
/// Most functions return an `i32` status code. `0` generally means success.
/// Negative values correspond to `HostAbiError` variants.
///
/// # String/CID Handling
/// CIDs and other string-like data are passed as `*const c_char` (null-terminated C strings).
/// Buffers provided by WASM for host functions to write into should be of adequate size.
/// The host will write a null terminator if the buffer is large enough.
/// Functions returning string data will indicate the required buffer size if the provided one is too small.
///
/// # Binary Data Handling
/// Binary data is passed using `AbiBytes` (pointer and length).
#[async_trait::async_trait]
pub trait HostEnvironment: Send + Sync + Clone + 'static {
    // --- Job Context & Info ---
    fn get_job_id(&self, job_id_buf_ptr: *mut c_char, job_id_buf_len: u32) -> i32;
    fn get_originator_did(&self, did_buf_ptr: *mut c_char, did_buf_len: u32) -> i32;
    fn get_current_epoch(&self, epoch_buf_ptr: *mut c_char, epoch_buf_len: u32) -> i32;
    fn get_current_timestamp(&self) -> i64; // Unix timestamp in seconds

    // --- Workflow & Stage Info ---
    fn get_workflow_type(&self) -> i32; // Returns WorkflowType variant as i32, or -1 if no workflow context
    fn get_current_stage_index(&self) -> i32; // Returns stage index, or -1 if not in a multi-stage workflow
    fn get_current_stage_id(&self, stage_id_buf_ptr: *mut c_char, stage_id_buf_len: u32) -> i32;
    fn get_stage_input_cid(&self, cid_buf_ptr: *mut c_char, cid_buf_len: u32) -> i32;

    // --- Logging & Diagnostics ---
    fn log_msg(&self, level: i32, msg_ptr: *const c_char, msg_len: u32) -> i32;

    // --- Data Storage & Anchoring ---
    fn read_cid_data(
        &self,
        cid_ptr: *const c_char,
        offset: u64,
        buffer_ptr: *mut u8,
        buffer_len: u32,
    ) -> i32;
    fn write_data_and_get_cid(
        &self,
        data_ptr: *const u8,
        data_len: u32,
        cid_buf_ptr: *mut c_char,
        cid_buf_len: u32,
    ) -> i32;
    fn anchor_cid(
        &self,
        cid_ptr: *const c_char,
        metadata_ptr: *const c_char,
        metadata_len: u32,
    ) -> i32;

    // --- Cryptography & Identity ---
    fn verify_signature(
        &self,
        did_ptr: *const c_char,
        data_ptr: *const u8,
        data_len: u32,
        sig_ptr: *const u8,
        sig_len: u32,
    ) -> i32;
    fn sign_data(
        &self,
        data_ptr: *const u8,
        data_len: u32,
        sig_buf_ptr: *mut u8,
        sig_buf_len: u32,
    ) -> i32;

    // --- Resource Management ---
    fn consume_resource(&self, rt_type_val: i32, amt: u64) -> i32;
    fn remaining_resource(&self, rt_type_val: i32) -> i64;

    // --- Interactive Job Support --- (Optional, host may not support)
    fn send_interactive_output(&self, data_ptr: *const u8, data_len: u32) -> i32;
    fn receive_interactive_input(
        &self,
        buffer_ptr: *mut u8,
        buffer_len: u32,
        timeout_ms: u32,
    ) -> i32;
    fn peek_interactive_input_buffer_size(&self) -> i32;
    fn clear_interactive_input_buffer(&self) -> i32;

    // --- Network & P2P --- (Optional)
    async fn p2p_send_message(
        &self,
        peer_did_ptr: *const c_char,
        data_ptr: *const u8,
        data_len: u32,
    ) -> i32;
    async fn p2p_receive_message(
        &self,
        buffer_ptr: *mut u8,
        buffer_len: u32,
        timeout_ms: u32,
    ) -> i32;

    // --- Dynamic Linking / Capability Invocation --- (Optional)
    async fn call_capability(
        &self,
        capability_cid_ptr: *const c_char,
        input_ptr: *const u8,
        input_len: u32,
        output_buf_ptr: *mut u8,
        output_buf_len: u32,
    ) -> i32;
}

/// Helper to safely copy a Rust string into a C buffer provided by WASM.
/// Returns the number of bytes written (excluding null terminator) or a HostAbiError code.
pub fn copy_string_to_c_buf(rust_str: &str, c_buf: *mut c_char, c_buf_len: u32) -> i32 {
    if c_buf.is_null() || c_buf_len == 0 {
        return HostAbiError::InvalidArguments as i32;
    }
    let bytes = rust_str.as_bytes();
    let len_to_write = bytes.len();

    if (len_to_write + 1) > c_buf_len as usize {
        // +1 for null terminator
        return HostAbiError::BufferTooSmall as i32;
    }

    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), c_buf as *mut u8, len_to_write);
        // Write null terminator
        *(c_buf as *mut u8).add(len_to_write) = 0;
    }
    len_to_write as i32
}

/// Helper for converting a C string (UTF-8 assumed) from WASM memory to a Rust String.
/// The caller must ensure `c_str_ptr` is valid and null-terminated.
///
/// # Safety
///
/// - `c_str_ptr` must be a valid pointer to a null-terminated C string.
/// - The memory pointed to by `c_str_ptr` must be valid for reads up to the null terminator.
/// - The string data must be valid UTF-8.
pub unsafe fn string_from_c_str(c_str_ptr: *const c_char) -> Result<String, HostAbiError> {
    if c_str_ptr.is_null() {
        return Err(HostAbiError::InvalidArguments);
    }
    CStr::from_ptr(c_str_ptr)
        .to_str()
        .map(|s| s.to_owned())
        .map_err(|_| HostAbiError::InvalidArguments) // UTF-8 conversion error
}

/// Helper to safely create a Rust Vec<u8> from AbiBytes provided by WASM.
pub fn vec_from_abi_bytes(abi_bytes: AbiBytes) -> Result<Vec<u8>, HostAbiError> {
    if abi_bytes.ptr.is_null() {
        if abi_bytes.len == 0 {
            return Ok(Vec::new());
        } else {
            return Err(HostAbiError::InvalidArguments);
        }
    }
    unsafe { Ok(slice::from_raw_parts(abi_bytes.ptr, abi_bytes.len as usize).to_vec()) }
}

/// Defines the interface for host functions callable from WASM modules
/// that implement the ICN Mesh ABI.
///
/// S is the StoreData type that will be used with Wasmtime.
#[async_trait::async_trait]
pub trait MeshHostAbi<S: Send + Sync + 'static>: Send + Sync {
    // Section Management
    async fn host_begin_section(
        &self,
        mut caller: Caller<'_, S>,
        kind_ptr: u32,
        kind_len: u32,
        title_ptr: u32,
        title_len: u32,
    ) -> Result<i32, HostAbiError>;

    async fn host_end_section(&self, mut caller: Caller<'_, S>) -> Result<i32, HostAbiError>;

    // Property Management (within sections)
    async fn host_set_property(
        &self,
        mut caller: Caller<'_, S>,
        key_ptr: u32,
        key_len: u32,
        value_json_ptr: u32,
        value_json_len: u32,
    ) -> Result<i32, HostAbiError>;

    // Data Anchoring
    async fn host_anchor_data(
        &self,
        mut caller: Caller<'_, S>,
        path_ptr: u32,
        path_len: u32,
        data_ref_ptr: u32, // CID of data to anchor, as string
        data_ref_len: u32,
    ) -> Result<i32, HostAbiError>;

    // Generic Host Call (for extensibility, e.g., specific service interactions)
    async fn host_generic_call(
        &self,
        mut caller: Caller<'_, S>,
        fn_name_ptr: u32,
        fn_name_len: u32,
        args_payload_ptr: u32, // CBOR encoded arguments
        args_payload_len: u32,
    ) -> Result<i32, HostAbiError>; // Returns CBOR encoded result or error

    // Proposal and Workflow Management (Conceptual)
    async fn host_create_proposal(
        &self,
        mut caller: Caller<'_, S>,
        id_ptr: u32, // String: DID or unique ID for the proposal context
        id_len: u32,
        title_ptr: u32, // String
        title_len: u32,
        version_ptr: u32, // String (e.g., "1.0", "git-sha")
        version_len: u32,
        // Further params might define proposal type, target coop, etc.
    ) -> Result<i32, HostAbiError>; // Returns proposal_handle or error

    // Token/Resource Management
    async fn host_mint_token( // Or a more generic "manage_resource"
        &self,
        mut caller: Caller<'_, S>,
        resource_type_ptr: u32, // String: e.g., "ccl_credits", "storage_gb_hours"
        resource_type_len: u32,
        amount: u64, // Can be positive (mint/allocate) or negative (burn/revoke) - design choice
        recipient_did_ptr: u32, // String: DID of recipient
        recipient_did_len: u32,
        data_json_ptr: u32, // Optional JSON metadata for the transaction/ledger entry
        data_json_len: u32,
    ) -> Result<i32, HostAbiError>;

    // Conditional Logic Support
    async fn host_if_condition_eval(
        &self,
        mut caller: Caller<'_, S>,
        condition_str_ptr: u32, // String: human-readable condition or scriptlet
        condition_str_len: u32,
        // Potentially args_payload for the condition evaluation context
    ) -> Result<i32, HostAbiError>; // 0 for false, 1 for true, <0 for error

    async fn host_else_handler(&self, mut caller: Caller<'_, S>) -> Result<i32, HostAbiError>; // Marks an 'else' block start

    async fn host_endif_handler(&self, mut caller: Caller<'_, S>) -> Result<i32, HostAbiError>; // Marks an 'endif' block end

    // Logging and Diagnostics
    async fn host_log_todo( // For "TODO: implement this logic" style messages
        &self,
        mut caller: Caller<'_, S>,
        message_ptr: u32,
        message_len: u32,
    ) -> Result<i32, HostAbiError>;

    // Event Handling (Conceptual)
    async fn host_on_event( // Register a handler for a specific event type
        &self,
        mut caller: Caller<'_, S>,
        event_name_ptr: u32, // String: e.g., "proposal_approved", "job_completed"
        event_name_len: u32,
        // callback_idx: u32, // Index in WASM table for the callback function
    ) -> Result<i32, HostAbiError>;

    // Deprecated log for job status, use specific event or section properties
    async fn host_log_debug_deprecated(
        &self,
        mut caller: Caller<'_, S>,
        message_ptr: u32,
        message_len: u32,
    ) -> Result<i32, HostAbiError>;


    // Resource Accounting / Range Checks
    async fn host_range_check( // Simple numeric range check
        &self,
        mut caller: Caller<'_, S>,
        value: i64,
        min_val: i64,
        max_val: i64,
    ) -> Result<i32, HostAbiError>; // 0 if in range, error code otherwise


    async fn host_use_resource( // Explicitly request/use a named resource
        &self,
        mut caller: Caller<'_, S>,
        resource_type_ptr: u32,
        resource_type_len: u32,
        amount: u64,
    ) -> Result<i32, HostAbiError>;


    async fn host_transfer_token(
        &self,
        mut caller: Caller<'_, S>,
        token_type_ptr: u32,
        token_type_len: u32,
        amount: u64,
        sender_did_ptr: u32,
        sender_did_len: u32,
        recipient_did_ptr: u32,
        recipient_did_len: u32,
    ) -> Result<i32, HostAbiError>;

    async fn host_submit_mesh_job(
        &self,
        mut caller: Caller<'_, S>,
        cbor_payload_ptr: u32,
        cbor_payload_len: u32,
        job_id_buffer_ptr: u32,   // Pointer to a buffer to write the job ID string
        job_id_buffer_len: u32, // Length of the job ID buffer
    ) -> Result<i32, HostAbiError>; // Returns length of written job ID or error
}