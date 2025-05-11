// InterCooperative Network (ICN) - Job Execution Context
// This module defines the data structure used by the `icn-runtime` to hold the state
// and manage the execution of a single Mesh Job.

use icn_types::mesh::{MeshJobParams, JobId};
use icn_identity::Did;
use planetary_mesh::protocol::JobInteractiveInputV1; // Assuming this is the correct path
use planetary_mesh::JobStatus; // Assuming this is the correct path for the enhanced JobStatus
use host_abi::{LogLevel, ReceivedInputInfo, ReceivedInputType, HostAbiError}; // Assuming host-abi is a sibling crate or correctly pathed

use std::collections::VecDeque;
use std::sync::{Arc, Mutex}; // For potential async wakers or shared state within context
// use std::task::Waker; // If we were to store Wakers directly for async yielding

// Conceptual internal representation of job permissions/capabilities.
// This would be more complex in a real system, potentially derived from tokens or policies.
#[derive(Debug, Clone)]
pub struct JobPermissions {
    pub can_read_all_cids: bool,
    pub allowed_cid_prefixes: Option<Vec<String>>,
    pub can_write_data: bool,
    pub can_send_interactive_output: bool,
    pub max_log_level_allowed: LogLevel,
    // Add more granular permissions as needed, e.g., network access controls for WASM.
}

impl Default for JobPermissions {
    fn default() -> Self {
        JobPermissions {
            can_read_all_cids: false, // Default to restrictive
            allowed_cid_prefixes: None,
            can_write_data: false,
            can_send_interactive_output: false, // Must be explicitly enabled by job_params.is_interactive
            max_log_level_allowed: LogLevel::Info, // Default reasonable log level
        }
    }
}

/// Holds the runtime state and context for a single executing Mesh Job.
#[derive(Debug)] // Clone might be complex due to internal state like Wakers if used.
pub struct JobExecutionContext {
    pub job_id: JobId,
    pub originator_did: Did,
    pub job_params: MeshJobParams, // The full parameters defining the job.

    pub current_status: JobStatus, // The rich status from planetary_mesh::JobStatus.
    pub current_stage_index: Option<u32>, // Current stage for workflow jobs.
    pub current_stage_id: Option<String>,   // User-defined ID of the current stage.

    // Interactivity State
    // pub is_interactive_session_active: bool, // Can be inferred from current_status == PendingUserInput or a dedicated flag
    pub interactive_input_queue: VecDeque<JobInteractiveInputV1>,
    pub interactive_output_sequence_num: u64, // Sequence for messages sent by this job.

    // Resource Tracking (placeholders - would integrate with a metering service)
    pub mana_consumed: u128,
    pub cpu_time_us_consumed: u64,
    pub memory_mb_peak_usage: u32,

    // Permissions for this job instance.
    pub permissions: JobPermissions,

    // For managing async yielding of WASM execution when awaiting input.
    // This is highly dependent on the async executor and WASM VM integration.
    // For now, this is a conceptual placeholder. A real implementation might use
    // a channel sender to signal the WASM task, or a shared Condvar.
    // pub input_waker: Option<Waker>,

    // Timestamp for when the job/stage started execution, for timeouts etc.
    pub execution_start_time_ms: u64, 
}

impl JobExecutionContext {
    pub fn new(job_id: JobId, originator_did: Did, job_params: MeshJobParams, host_node_did: Did, current_time_ms: u64) -> Self {
        let mut permissions = JobPermissions::default();
        if job_params.is_interactive {
            permissions.can_send_interactive_output = true;
            // Potentially set a higher default log level for interactive debugging
            permissions.max_log_level_allowed = LogLevel::Debug;
        }
        // TODO: Derive more permissions based on job_params, originator, or capability tokens.

        JobExecutionContext {
            job_id,
            originator_did,
            current_status: JobStatus::Running { // Initial status when execution context is created
                node_id: host_node_did, // The DID of the current executor node
                current_stage_index: if job_params.workflow_type != icn_types::mesh::WorkflowType::SingleWasmModule { Some(0) } else { None },
                current_stage_id: job_params.stages.as_ref().and_then(|s| s.get(0).map(|sd| sd.stage_id.clone())),
                progress_percent: Some(0),
                status_message: Some("Job initializing".to_string()),
            },
            job_params, // job_params moved here
            current_stage_index: None, // Will be set properly by workflow logic if applicable
            current_stage_id: None,    // Will be set properly by workflow logic if applicable
            interactive_input_queue: VecDeque::new(),
            interactive_output_sequence_num: 0,
            mana_consumed: 0,
            cpu_time_us_consumed: 0,
            memory_mb_peak_usage: 0,
            permissions,
            // input_waker: None,
            execution_start_time_ms: current_time_ms,
        }
    }

    // Example method to update status and potentially notify (simplified)
    pub fn update_status(&mut self, new_status: JobStatus /*, p2p_service: &P2pService */) {
        self.current_status = new_status;
        // In a real implementation, this might also trigger sending a JobStatusUpdateV1 P2P message.
        // e.g., p2p_service.send_job_status_update(self.job_id.clone(), self.originator_did.clone(), self.current_status.clone());
        println!("Job {} status updated to: {:?}", self.job_id, self.current_status);
    }
} 