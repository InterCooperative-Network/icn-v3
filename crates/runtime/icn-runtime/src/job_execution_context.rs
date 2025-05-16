// InterCooperative Network (ICN) - Job Execution Context
// This module defines the data structure used by the `icn-runtime` to hold the state
// and manage the execution of a single Mesh Job.

use host_abi::LogLevel;
use icn_identity::Did;
use icn_mesh_protocol::{JobInteractiveInputV1, P2PJobStatus};
use icn_types::mesh::MeshJobParams;
use std::collections::VecDeque;
use host_abi::HostAbiError;
use std::collections::HashMap;
use std::str::FromStr;

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

#[derive(Debug, Clone)]
pub struct SectionContext {
    pub kind: String,
    pub title: Option<String>,
    pub properties: HashMap<String, String>,
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

    // For ABI tests
    pub section_stack: Vec<SectionContext>,
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
            permissions.max_log_level_allowed = LogLevel::Debug;
        }
        // TODO: Derive more permissions based on job_params, originator, or capability tokens.

        JobExecutionContext {
            job_id,
            originator_did,
            current_status: P2PJobStatus::Running {
                node_id: host_node_did,
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
            current_stage_index: None,
            current_stage_id: None,
            interactive_input_queue: VecDeque::new(),
            interactive_output_sequence_num: 0,
            mana_consumed: 0,
            cpu_time_us_consumed: 0,
            memory_mb_peak_usage: 0,
            permissions,
            execution_start_time_ms: current_time_ms,
            section_stack: Vec::new(),
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

    // --- ABI Method Stubs ---
    pub fn begin_section(&mut self, kind: String, title: Option<String>) -> Result<(), HostAbiError> {
        if self.section_stack.is_empty() {
            self.section_stack.push(SectionContext {
                kind,
                title,
                properties: HashMap::new(),
            });
            Ok(())
        } else {
            Err(HostAbiError::InvalidState("Cannot begin a new section while another is active (nested sections not supported).".to_string()))
        }
    }

    pub fn end_section(&mut self) -> Result<(), HostAbiError> {
        if self.section_stack.pop().is_none() {
            eprintln!("[JEC WARN] end_section called with empty section_stack");
        }
        Ok(())
    }

    pub fn set_property(&mut self, key: String, value_json: String) -> Result<(), HostAbiError> {
        if let Some(current_section) = self.section_stack.last_mut() {
            current_section.properties.insert(key, value_json);
            Ok(())
        } else {
            Err(HostAbiError::InvalidState("No active section to set property on.".to_string()))
        }
    }

    pub fn anchor_data(&mut self, path: String, data_ref: String) -> Result<(), HostAbiError> {
        println!("[JEC STUB] anchor_data: path={}, data_ref={}", path, data_ref);
        // TODO: Implement actual logic (e.g., record anchor)
        Ok(())
    }

    pub fn generic_call(&mut self, fn_name: String, args_payload: String) -> Result<(), HostAbiError> {
        println!("[JEC STUB] generic_call: fn_name={}, args_payload={}", fn_name, args_payload);
        // TODO: Implement actual logic
        Ok(())
    }

    pub fn create_proposal(&mut self, id: String, title: String, version: String) -> Result<(), HostAbiError> {
        if self.section_stack.is_empty() {
            Ok(())
        } else {
            Err(HostAbiError::InvalidState("Proposal creation requires an active section.".to_string()))
        }
    }

    pub fn mint_token(&mut self, res_type: String, amount: i64, recipient: Option<String>, data_json: Option<String>) -> Result<(), HostAbiError> {
        if self.section_stack.is_empty() {
            Ok(())
        } else {
            Err(HostAbiError::InvalidState("Mint token operation is not valid in the current context.".to_string()))
        }
    }

    pub fn if_condition_eval(&mut self, condition_str: String) -> Result<(), HostAbiError> {
        println!("[JEC STUB] if_condition_eval: {}", condition_str);
        // TODO: Implement actual logic (e.g., evaluate condition, manage conditional stack)
        Ok(())
    }

    pub fn else_handler(&mut self) -> Result<(), HostAbiError> {
        println!("[JEC STUB] else_handler");
        // TODO: Implement actual logic (e.g., manage conditional stack)
        Ok(())
    }

    pub fn endif_handler(&mut self) -> Result<(), HostAbiError> {
        println!("[JEC STUB] endif_handler");
        // TODO: Implement actual logic (e.g., manage conditional stack)
        Ok(())
    }

    pub fn on_event(&mut self, event_name: String) -> Result<(), HostAbiError> {
        println!("[JEC STUB] on_event: {}", event_name);
        // TODO: Implement actual logic (e.g., register event handler context)
        Ok(())
    }

    pub fn range_check(&mut self, start_val: f64, end_val: f64) -> Result<(), HostAbiError> {
        println!("[JEC STUB] range_check: start={}, end={}", start_val, end_val);
        // TODO: Implement actual logic (e.g., manage range check context)
        Ok(())
    }

    pub fn use_resource(&mut self, resource_type: String, amount: i64) -> Result<(), HostAbiError> {
        println!("[JEC STUB] use_resource: type={}, amount={}", resource_type, amount);
        // TODO: Implement actual logic (e.g., record resource usage)
        Ok(())
    }

    pub fn transfer_token(&mut self, token_type: String, amount: i64, sender: Option<String>, recipient: String) -> Result<(), HostAbiError> {
        println!("[JEC STUB] transfer_token: type={}, amount={}, sender={:?}, recipient={}", token_type, amount, sender, recipient);
        // TODO: Implement actual logic
        Ok(())
    }

    pub fn submit_mesh_job(&mut self, cbor_payload: Vec<u8>, write_back_fn: impl FnOnce(&str) -> Result<i32, HostAbiError>) -> Result<i32, HostAbiError> {
        println!("[JEC STUB] submit_mesh_job: payload_len={}", cbor_payload.len());
        let dummy_job_id = "dummy_mesh_job_123";
        write_back_fn(dummy_job_id)
    }
}

// Default implementation for JobExecutionContext for testing
impl Default for JobExecutionContext {
    fn default() -> Self {
        let dummy_did = Did::from_str("did:icn:test_originator").unwrap_or_else(|_| {
            panic!("Failed to create dummy DID for JobExecutionContext::default");
        });
        let dummy_host_did = Did::from_str("did:icn:test_host").unwrap_or_else(|_| {
            panic!("Failed to create dummy host DID for JobExecutionContext::default");
        });

        JobExecutionContext {
            job_id: "test_job_id".to_string(),
            originator_did: dummy_did,
            job_params: MeshJobParams::default(),
            current_status: P2PJobStatus::Running {
                node_id: dummy_host_did,
                current_stage_index: None,
                current_stage_id: None,
                progress_percent: Some(0),
                status_message: Some("Default JEC initialized".to_string()),
            },
            current_stage_index: None,
            current_stage_id: None,
            interactive_input_queue: VecDeque::new(),
            interactive_output_sequence_num: 0,
            mana_consumed: 0,
            cpu_time_us_consumed: 0,
            memory_mb_peak_usage: 0,
            permissions: JobPermissions::default(),
            execution_start_time_ms: 0,
            section_stack: Vec::new(),
        }
    }
}
