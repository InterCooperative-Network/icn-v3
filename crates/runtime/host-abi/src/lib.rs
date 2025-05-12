pub mod bindings;

// Export all bindings at the crate root for easy access
pub use bindings::*;

pub const ICN_HOST_ABI_VERSION: u32 = 8; // bump from 7 → 8 for mesh job submission ABI change 

// InterCooperative Network (ICN) - Host ABI Definitions
// This crate defines the Application Binary Interface (ABI) that WASM modules (e.g., CCL contracts)
// use to interact with the ICN host runtime environment. It specifies the functions,
// data structures, and error codes for this interaction.

// Using core::ffi::c_void for potential opaque handles in the future, though not strictly used by current function signatures.
use core::ffi::c_void;
use serde::Serialize;
// No wasmer imports needed

// Need Display for Trap::new
use std::fmt;
use wasmtime::Trap;
use thiserror::Error;
use wasmtime::{Caller, Linker}; // Keep Trap for now, might be needed elsewhere or remove later if truly unused.
use anyhow::Error as AnyhowError;

// --- Helper Enums & Structs for ABI Communication ---

/// Specifies the type of data contained in a `ReceivedInputInfo` structure,
/// indicating whether interactive input is provided inline or as a CID.
#[repr(u32)] // Ensures stable representation across the ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ReceivedInputType {
    InlineData = 0, // The data is provided directly after ReceivedInputInfo.
    Cid = 1,        // The data provided after ReceivedInputInfo is a CID string pointing to the actual input.
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

/// Errors returned by Host ABI functions.
#[derive(Error, Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostAbiError {
    #[error("Unknown error")]
    UnknownError,
    #[error("Memory access error")]
    MemoryAccessError,
    #[error("Buffer too small")]
    BufferTooSmall,
    #[error("Invalid arguments")]
    InvalidArguments,
    #[error("Not found")]
    NotFound,
    #[error("Timeout")]
    Timeout,
    #[error("Not permitted")]
    NotPermitted,
    #[error("Not supported")]
    NotSupported,
    #[error("Resource limit exceeded")]
    ResourceLimitExceeded,
    #[error("Data encoding error (UTF8/CBOR)")]
    DataEncodingError,
    #[error("Invalid state")]
    InvalidState,
    #[error("Network error")]
    NetworkError,
    #[error("Storage error")]
    StorageError,
    #[error("Serialization error")]
    SerializationError,
    #[error("Invalid DID format")]
    InvalidDIDFormat,
    #[error("Invalid CID format")]
    InvalidCIDFormat,
    #[error("Queue full")]
    QueueFull,
    #[error("Channel closed")]
    ChannelClosed,
}

// Add Send + Sync + 'static bounds to HostAbiError if necessary
// (Assuming it's implicitly Send + Sync + 'static based on its fields)

// --- The Host ABI Trait (Using Wasmtime concepts) ---
// Functions will be called with a Caller<'a, T> where T is the host state
// Memory access uses Caller::get_export("memory").and_then(|mem| mem.into_memory())
// Caller provides access to host state via caller.data() or caller.data_mut()

pub trait MeshHostAbi<T: Sized> { // Generic over Host State T
    // **I. Job & Workflow Information **

    /// Gets the unique ID of the current job.
    /// The Job ID is written as a UTF-8 string into the buffer specified by `job_id_buf_ptr`.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `job_id_buf_ptr` (u32): Pointer to the buffer in WASM memory to write the Job ID.
    /// * `job_id_buf_len` (u32): Length of the provided buffer.
    /// # Returns
    /// * `i32`: Number of bytes written for the Job ID string if successful.
    ///            Returns `HostAbiError::BufferTooSmall` if the buffer is insufficient.
    ///            Other negative `HostAbiError` codes on other failures.
    fn host_job_get_id(&self, caller: wasmtime::Caller<T>, job_id_buf_ptr: u32, job_id_buf_len: u32) -> Result<i32, AnyhowError>;

    /// Gets the CID of the initial input data specified when the job was submitted (from `MeshJobParams.input_data_cid`).
    /// The CID is written as a UTF-8 string into `cid_buf_ptr`.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `cid_buf_ptr` (u32): Pointer to the buffer in WASM memory for the CID string.
    /// * `cid_buf_len` (u32): Length of the buffer.
    /// # Returns
    /// * `i32`: Number of bytes written for the CID string.
    ///            Returns 0 if `input_data_cid` was `None` for the job.
    ///            Returns `HostAbiError::BufferTooSmall` if the buffer is insufficient.
    ///            Other negative `HostAbiError` codes on other failures.
    fn host_job_get_initial_input_cid(&self, caller: wasmtime::Caller<T>, cid_buf_ptr: u32, cid_buf_len: u32) -> Result<i32, AnyhowError>;

    /// Checks if the current job has been marked as interactive (from `MeshJobParams.is_interactive`).
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// # Returns
    /// * `i32`: 1 if the job is interactive.
    ///            0 if the job is not interactive.
    ///            Negative `HostAbiError` codes on failure (e.g., job context not found).
    fn host_job_is_interactive(&self, caller: wasmtime::Caller<T>) -> Result<i32, AnyhowError>;

    /// Gets the current stage index (0-based) if the job is part of a multi-stage workflow.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// # Returns
    /// * `i32`: The current stage index if the job is in a workflow.
    ///            Returns -1 if the job is a `SingleWasmModule` type (not a multi-stage workflow).
    ///            Other negative `HostAbiError` codes on failures.
    fn host_workflow_get_current_stage_index(&self, caller: wasmtime::Caller<T>) -> Result<i32, AnyhowError>;

    /// Gets the user-defined ID of the current stage, if available and if the job is in a multi-stage workflow.
    /// The stage ID is written as a UTF-8 string into `stage_id_buf_ptr`.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `stage_id_buf_ptr` (u32): Pointer to the buffer in WASM memory for the stage ID.
    /// * `stage_id_buf_len` (u32): Length of the buffer.
    /// # Returns
    /// * `i32`: Number of bytes written for the stage ID string.
    ///            Returns 0 if no stage ID is defined for the current stage, or if not in a multi-stage workflow.
    ///            Returns `HostAbiError::BufferTooSmall` if the buffer is insufficient.
    ///            Other negative `HostAbiError` codes on other failures.
    fn host_workflow_get_current_stage_id(
        &self,
        caller: wasmtime::Caller<T>,
        stage_id_buf_ptr: u32,
        stage_id_buf_len: u32,
    ) -> Result<i32, AnyhowError>;

    /// Gets the resolved input CID for the current stage of a workflow.
    /// This function interprets the `StageInputSource` for the current stage.
    /// If the source requires an `input_key` (e.g., from `StageInputSource::JobInput(key)` or
    /// `StageInputSource::PreviousStageOutput(prev_stage_id, key)`), that key must be provided.
    /// The resolved CID is written as a UTF-8 string into `cid_buf_ptr`.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `input_key_ptr` (u32): Pointer to a UTF-8 string in WASM memory representing the input key. Can be 0 if no key is applicable for the stage's input source.
    /// * `input_key_len` (u32): Length of the input key string. Can be 0 if no key.
    /// * `cid_buf_ptr` (u32): Pointer to the buffer in WASM memory for the resolved CID string.
    /// * `cid_buf_len` (u32): Length of the CID buffer.
    /// # Returns
    /// * `i32`: Number of bytes written for the CID string.
    ///            Returns 0 if the current stage has no defined input (`StageInputSource::NoInput`) or if input resolution yields no CID.
    ///            Returns `HostAbiError::BufferTooSmall` if `cid_buf_ptr` is insufficient.
    ///            Returns `HostAbiError::NotFound` if a referenced previous stage output or job input key is not found.
    ///            Other negative `HostAbiError` codes on other failures.
    fn host_workflow_get_current_stage_input_cid(
        &self,
        caller: wasmtime::Caller<T>,
        input_key_ptr: u32,
        input_key_len: u32,
        cid_buf_ptr: u32,
        cid_buf_len: u32,
    ) -> Result<i32, AnyhowError>;

    // **II. Status & Progress Reporting **

    /// Reports the current progress of the job or stage to the host.
    /// The host may use this to update the job's status (e.g., `JobStatus::Running` fields)
    /// and trigger a `JobStatusUpdateV1` P2P message.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `percentage` (u8): Progress percentage (0-100).
    /// * `status_message_ptr` (u32): Pointer to a UTF-8 encoded status message string in WASM memory.
    /// * `status_message_len` (u32): Length of the status message string.
    /// # Returns
    /// * `i32`: `HostAbiError::Success` (0) if the report was accepted.
    ///            Negative `HostAbiError` codes on failure (e.g., `InvalidArguments` for bad message string).
    fn host_job_report_progress(
        &self,
        caller: wasmtime::Caller<T>,
        percentage: u8,
        status_message_ptr: u32,
        status_message_len: u32,
    ) -> Result<i32, AnyhowError>;

    /// Signals that the current stage of a multi-stage workflow has completed successfully.
    /// The host will typically update the job's status (e.g., to `AwaitingNextStage` or `Completed`)
    /// and may trigger a `JobStatusUpdateV1` P2P message.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `output_cid_ptr` (u32): Optional pointer to a UTF-8 string in WASM memory representing the primary output CID for this stage. Can be 0 if no primary output CID.
    /// * `output_cid_len` (u32): Length of the output CID string. Can be 0 if no primary output CID.
    /// # Returns
    /// * `i32`: `HostAbiError::Success` (0) on successful completion reporting.
    ///            Returns `HostAbiError::InvalidState` if not in a multi-stage workflow or not in an active stage.
    ///            Other negative `HostAbiError` codes on failure.
    /// # Note
    /// For stages producing multiple named outputs, the contract should currently aggregate them
    /// into a single structure, store that structure using `host_data_write_buffer` to get a CID,
    /// and pass that single CID as the `output_cid_ptr` here.
    fn host_workflow_complete_current_stage(
        &self,
        caller: wasmtime::Caller<T>,
        output_cid_ptr: u32,
        output_cid_len: u32,
    ) -> Result<i32, AnyhowError>;

    // **III. Interactivity **

    /// Sends interactive output data from the WASM job to the job originator/client.
    /// The host will construct and send a `JobInteractiveOutputV1` P2P message.
    /// The host determines if the payload is sent inline or as a CID based on its size.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `payload_ptr` (u32): Pointer to the raw payload data in WASM memory.
    /// * `payload_len` (u32): Length of the payload data.
    /// * `output_key_ptr` (u32): Optional pointer to a UTF-8 string in WASM memory, serving as a key or identifier for this output. Can be 0 if not applicable.
    /// * `output_key_len` (u32): Length of the output key string. Can be 0 if not applicable.
    /// * `is_final_chunk` (i32): 1 if this is the final chunk of a (potentially streamed) response for this interaction or output key, 0 otherwise.
    /// # Returns
    /// * `i32`: `HostAbiError::Success` (0) if the output was accepted for sending.
    ///            Returns `HostAbiError::NotPermitted` if the job is not interactive or not allowed to send output.
    ///            Returns `HostAbiError::ResourceLimitExceeded` if payload is too large for host to handle (e.g. create CID for).
    ///            Other negative `HostAbiError` codes on failure.
    fn host_interactive_send_output(
        &self,
        caller: wasmtime::Caller<T>,
        payload_ptr: u32,
        payload_len: u32,
        output_key_ptr: u32,
        output_key_len: u32,
        is_final_chunk: i32, // 1 for true, 0 for false
    ) -> Result<i32, AnyhowError>;

    /// Attempts to receive interactive input data sent to the WASM job.
    /// The host checks an internal queue populated by incoming `JobInteractiveInputV1` P2P messages.
    /// If input is available, `ReceivedInputInfo` struct followed by the actual payload (or CID string)
    /// is written into the WASM buffer specified by `buffer_ptr`.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `buffer_ptr` (u32): Pointer to the buffer in WASM memory to write the `ReceivedInputInfo` and subsequent data/CID.
    /// * `buffer_len` (u32): Length of the provided WASM buffer.
    /// * `timeout_ms` (u32): Maximum time to wait for input in milliseconds.
    ///                        0 indicates a non-blocking check. `u32::MAX` suggests indefinite blocking
    ///                        (though the host may impose its own maximum timeout).
    /// # Returns
    /// * `i32`: Total number of bytes written to the WASM buffer (for `ReceivedInputInfo` + data/CID) if input is received.
    ///            Returns 0 if no input is available (for non-blocking call) or if the timeout elapses.
    ///            Returns `HostAbiError::BufferTooSmall` if `buffer_len` is insufficient for `ReceivedInputInfo` + data/CID. The input message remains queued.
    ///            Returns `HostAbiError::NotPermitted` if the job is not interactive or not in a state to receive input.
    ///            Other negative `HostAbiError` codes on failure.
    fn host_interactive_receive_input(
        &self,
        caller: wasmtime::Caller<T>,
        buffer_ptr: u32,
        buffer_len: u32,
        timeout_ms: u32,
    ) -> Result<i32, AnyhowError>;

    /// Gets the total size (in bytes) required to store the next available interactive input message
    /// (i.e., `sizeof(ReceivedInputInfo)` + length of its associated data/CID payload).
    /// This allows the WASM module to allocate an appropriately sized buffer before calling `host_interactive_receive_input`.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// # Returns
    /// * `i32`: Required size in bytes if input is available.
    ///            Returns 0 if no input is currently available in the queue.
    ///            Negative `HostAbiError` codes on failure.
    fn host_interactive_peek_input_len(&self, caller: wasmtime::Caller<T>) -> Result<i32, AnyhowError>;

    /// Signals to the host that the job is now expecting user input and may pause or yield execution.
    /// The host typically uses this to transition the job's status to `JobStatus::PendingUserInput`
    /// and inform the job originator/client.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `prompt_cid_ptr` (u32): Optional pointer to a UTF-8 string in WASM memory, representing a CID for data that describes the needed input (e.g., a schema, a detailed prompt). Can be 0 if not applicable.
    /// * `prompt_cid_len` (u32): Length of the prompt CID string. Can be 0 if not applicable.
    /// # Returns
    /// * `i32`: `HostAbiError::Success` (0) if the prompt was accepted.
    ///            Returns `HostAbiError::NotPermitted` if the job is not interactive.
    ///            Other negative `HostAbiError` codes on failure.
    fn host_interactive_prompt_for_input(
        &self,
        caller: wasmtime::Caller<T>,
        prompt_cid_ptr: u32,
        prompt_cid_len: u32,
    ) -> Result<i32, AnyhowError>;

    // **IV. Data Handling & Storage (Interacting with Host's IPFS-like Storage) **

    /// Reads data from a resource identified by a CID from the host's storage layer.
    /// Data is read into the WASM buffer specified by `buffer_ptr`.
    /// The job must have permission to read the specified CID.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `cid_ptr` (u32): Pointer to the UTF-8 string in WASM memory representing the CID to read.
    /// * `cid_len` (u32): Length of the CID string.
    /// * `offset` (u64): Byte offset within the data (identified by CID) from which to start reading.
    /// * `buffer_ptr` (u32): Pointer to the buffer in WASM memory where the read data will be written.
    /// * `buffer_len` (u32): Length of the WASM buffer (maximum bytes to read).
    /// # Returns
    /// * `i32`: Number of bytes actually read and written to the WASM buffer. This might be less than `buffer_len` if the end of the data is reached.
    ///            Returns `HostAbiError::NotFound` if the CID does not exist.
    ///            Returns `HostAbiError::NotPermitted` if the job is not allowed to read this CID.
    ///            Returns `HostAbiError::InvalidArguments` for issues like bad offset or buffer parameters.
    ///            Other negative `HostAbiError` codes on other failures.
    fn host_data_read_cid(
        &self,
        caller: wasmtime::Caller<T>,
        cid_ptr: u32,
        cid_len: u32,
        offset: u64,
        buffer_ptr: u32,
        buffer_len: u32,
    ) -> Result<i32, AnyhowError>;

    /// Writes data from a WASM buffer to the host's storage layer, resulting in a new CID.
    /// The newly created CID (UTF-8 string) is written into the WASM buffer specified by `cid_buf_ptr`.
    /// The job must have permission to write data.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `data_ptr` (u32): Pointer to the raw data in WASM memory to be written.
    /// * `data_len` (u32): Length of the data to write.
    /// * `cid_buf_ptr` (u32): Pointer to the buffer in WASM memory where the resulting CID string will be written.
    /// * `cid_buf_len` (u32): Length of the CID buffer.
    /// # Returns
    /// * `i32`: Number of bytes written for the CID string.
    ///            Returns `HostAbiError::BufferTooSmall` if `cid_buf_len` is insufficient for the CID.
    ///            Returns `HostAbiError::NotPermitted` if the job is not allowed to write data.
    ///            Returns `HostAbiError::ResourceLimitExceeded` if `data_len` is too large or storage quota is hit.
    ///            Other negative `HostAbiError` codes on other failures.
    fn host_data_write_buffer(
        &self,
        caller: wasmtime::Caller<T>,
        data_ptr: u32,
        data_len: u32,
        cid_buf_ptr: u32,
        cid_buf_len: u32,
    ) -> Result<i32, AnyhowError>;

    // **V. Logging **

    /// Logs a message from the WASM module to the host's logging system.
    /// The host may choose to filter messages based on the log level and its own configuration.
    ///
    /// # Arguments
    /// * `caller` (Caller<'a, T>): Reference to the WASM caller environment.
    /// * `level` (LogLevel): The severity level of the log message (passed as u32 from WASM).
    /// * `message_ptr` (u32): Pointer to a UTF-8 encoded message string in WASM memory.
    /// * `message_len` (u32): Length of the message string.
    /// # Returns
    /// * `i32`: `HostAbiError::Success` (0) if the log message was accepted by the host.
    ///            Negative `HostAbiError` codes on failure (e.g., `InvalidArguments` for bad message or level).
    fn host_log_message(
        &self,
        caller: wasmtime::Caller<T>,
        level: LogLevel,
        message_ptr: u32,
        message_len: u32,
    ) -> Result<i32, AnyhowError>;
} 