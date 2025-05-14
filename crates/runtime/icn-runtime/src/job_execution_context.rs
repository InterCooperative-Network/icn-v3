// InterCooperative Network (ICN) - Job Execution Context
// This module defines the data structure used by the `icn-runtime` to hold the state
// and manage the execution of a single Mesh Job.

use host_abi::LogLevel;
use icn_identity::Did;
use icn_mesh_protocol::{JobInteractiveInputV1, P2PJobStatus};
use icn_types::mesh::MeshJobParams;
use std::collections::VecDeque;

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
    pub job_id: String,
    pub originator_did: Did,
    pub job_params: MeshJobParams, // The full parameters defining the job.

    pub current_status: P2PJobStatus, // The rich status from planetary_mesh::protocol::P2PJobStatus
    pub current_stage_index: Option<u32>, // Current stage for workflow jobs.
    pub current_stage_id: Option<String>, // User-defined ID of the current stage.

    // Interactivity State
    pub interactive_input_queue: VecDeque<JobInteractiveInputV1>,
    pub interactive_output_sequence_num: u64, // Sequence for messages sent by this job.

    // Resource Tracking (placeholders - would integrate with a metering service)
    pub mana_consumed: u128,
    pub cpu_time_us_consumed: u64,
    pub memory_mb_peak_usage: u32,

    // Permissions for this job instance.
    pub permissions: JobPermissions,

    // Timestamp for when the job/stage started execution, for timeouts etc.
    pub execution_start_time_ms: u64,
}

impl JobExecutionContext {
    pub fn new(
        job_id: String,
        originator_did: Did,
        job_params: MeshJobParams,
        host_node_did: Did,
        current_time_ms: u64,
    ) -> Self {
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
            current_status: P2PJobStatus::Running {
                // Initial status when execution context is created
                node_id: host_node_did, // The DID of the current executor node
                current_stage_index: if job_params.workflow_type
                    != icn_types::mesh::WorkflowType::SingleWasmModule
                {
                    Some(0)
                } else {
                    None
                },
                current_stage_id: job_params
                    .stages
                    .as_ref()
                    .and_then(|s| s.get(0).map(|sd| sd.stage_id.clone())),
                progress_percent: Some(0),
                status_message: Some("Job initializing".to_string()),
            },
            job_params,
            current_stage_index: None, // Will be set properly by workflow logic if applicable
            current_stage_id: None,    // Will be set properly by workflow logic if applicable
            interactive_input_queue: VecDeque::new(),
            interactive_output_sequence_num: 0,
            mana_consumed: 0,
            cpu_time_us_consumed: 0,
            memory_mb_peak_usage: 0,
            permissions,
            execution_start_time_ms: current_time_ms,
        }
    }

    // Example method to update status and potentially notify (simplified)
    pub fn update_status(&mut self, new_status: P2PJobStatus) {
        self.current_status = new_status;
        println!(
            "Job {} status updated to: {:?}",
            self.job_id, self.current_status
        );
    }
}
