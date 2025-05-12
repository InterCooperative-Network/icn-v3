use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Status of a P2P job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PJobStatus {
    /// Job is currently running
    Running {
        /// DID of the node currently executing the job
        node_id: Did,
        /// Current stage index for workflow jobs
        current_stage_index: Option<u32>,
        /// User-defined ID of the current stage
        current_stage_id: Option<String>,
        /// Progress percentage (0-100)
        progress_percent: Option<u8>,
        /// Human-readable status message
        status_message: Option<String>,
    },
    /// Job is waiting for user input
    PendingUserInput {
        /// DID of the node waiting for input
        node_id: Did,
        /// Current stage index for workflow jobs
        current_stage_index: Option<u32>,
        /// User-defined ID of the current stage
        current_stage_id: Option<String>,
        /// Human-readable status message
        status_message: Option<String>,
    },
    /// Job has completed successfully
    Completed {
        /// DID of the node that completed the job
        node_id: Did,
        /// Final output CID
        output_cid: String,
    },
    /// Job has failed
    Failed {
        /// DID of the node that failed the job
        node_id: Did,
        /// Error message
        error_message: String,
    },
}

/// Interactive input message for a job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInteractiveInputV1 {
    /// Sequence number for ordering
    pub sequence_num: u64,
    /// Input data
    pub data: Vec<u8>,
    /// Input key for routing
    pub input_key: String,
    /// Whether this is the final chunk
    pub is_final_chunk: bool,
}

/// Interactive output message from a job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInteractiveOutputV1 {
    /// Sequence number for ordering
    pub sequence_num: u64,
    /// Output data
    pub data: Vec<u8>,
    /// Output key for routing
    pub output_key: String,
    /// Whether this is the final chunk
    pub is_final_chunk: bool,
}

/// Protocol message types for mesh networking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MeshProtocolMessage {
    /// Interactive output from a job
    JobInteractiveOutputV1(JobInteractiveOutputV1),
    /// Job status update
    JobStatusUpdateV1 {
        /// Job ID
        job_id: String,
        /// New status
        status: P2PJobStatus,
    },
}

/// Constants for interactive input/output
pub const INLINE_PAYLOAD_MAX_SIZE: usize = 1024 * 1024; // 1MB
pub const MAX_INTERACTIVE_INPUT_BUFFER_PEEK: usize = 1024 * 1024; // 1MB 