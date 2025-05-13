use serde::{Deserialize, Serialize};
use crate::resource::ResourceType;
use icn_identity::Did; // Correct source for Did
use crate::org::{CooperativeId, CommunityId}; // Assuming these are in icn_types::org
use crate::jobs::policy::ExecutionPolicy; // New import
// use crate::identity::Did; // Removed erroneous/duplicate import
// use crate::runtime_receipt::RuntimeExecutionReceipt; // Removed, as it does not appear to be used in this file
use std::collections::HashMap;

// Potential unused imports to be checked by compiler, remove if confirmed unused by later build.
// Based on previous compiler output, these were unused:
// use crate::error::MeshError;
// use crate::trust::TrustBundleId;

/// Quality of Service profile for a Mesh Job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QoSProfile {
    BestEffort,
    LowLatency,
    CostOptimized,
    GuaranteedCompletion,
}

/// NEW Enum: Defines the source of input for a workflow stage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StageInputSource {
    /// Input is taken from the initial job parameters, identified by a key.
    JobInput(String),
    /// Input is taken from the output of a previous stage, identified by stage ID and output key.
    PreviousStageOutput(String, String), // (previous_stage_id, output_key_from_that_stage)
    /// Stage requires no explicit input.
    NoInput,
}

/// NEW Enum: Defines the type of workflow for a mesh job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkflowType {
    /// A single WASM module execution, similar to the current behavior.
    SingleWasmModule,
    /// A sequence of stages that execute in a defined order.
    SequentialPipeline,
    // Potentially in the future:
    // DirectedAcyclicGraph, // For more complex dependencies between stages
}

impl Default for WorkflowType {
    fn default() -> Self {
        WorkflowType::SingleWasmModule
    }
}

/// NEW Struct: Defines a single stage within a multi-stage workflow
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageDefinition {
    /// Unique identifier for this stage within the workflow.
    pub stage_id: String,
    /// Human-readable description of the stage.
    pub description: String,
    /// CID of the WASM module to execute for this stage.
    pub wasm_cid: String, // Made non-optional as per discussion
    /// Defines where this stage gets its input from.
    pub input_source: StageInputSource,
    /// Optional list of specific resources required for this stage.
    /// If None, job-level `resources_required` might apply, or stage might use defaults.
    pub resources_required: Option<Vec<(ResourceType, u64)>>,
    /// Optional deadline for this specific stage completion, as a Unix timestamp (seconds since epoch).
    pub deadline: Option<u64>,
    // pub expected_output_keys: Option<Vec<String>>, // Future consideration
}

/// Parameters for submitting a new mesh computation job.
/// This structure is expected to be serialized (e.g., to CBOR) and passed
/// as the payload to the `host_submit_mesh_job` ABI function.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshJobParams {
    /// CID of the WASM module to execute. For `SingleWasmModule` workflow type, this is the primary module.
    /// This field is primarily for `WorkflowType::SingleWasmModule`.
    pub wasm_cid: String,
    /// Human-readable description of the job.
    pub description: String,
    /// List of resources required for the job, specifying the type and amount.
    /// e.g., [(ResourceType::Cpu, 1000), (ResourceType::Memory, 2048)]
    pub resources_required: Vec<(ResourceType, u64)>,
    /// Desired Quality of Service profile for the job execution.
    pub qos_profile: QoSProfile,
    /// Optional deadline for job completion, as a Unix timestamp (seconds since epoch).
    pub deadline: Option<u64>,
    /// Optional CID of input data required for the job.
    pub input_data_cid: Option<String>,
    pub max_acceptable_bid_tokens: Option<u64>,

    // --- NEW Fields for refactoring ---
    /// Defines the type of workflow for this job. Defaults to `SingleWasmModule`.
    #[serde(default)]
    pub workflow_type: WorkflowType,

    /// List of stage definitions, used if `workflow_type` is not `SingleWasmModule`.
    /// `None` if it's a `SingleWasmModule` job. Each `StageDefinition` must specify its own `wasm_cid`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stages: Option<Vec<StageDefinition>>,

    /// Flag indicating if the job supports/requires real-time interaction. Defaults to `false`.
    #[serde(default)]
    pub is_interactive: bool,

    /// Optional CID of a schema describing the expected structure of the final job output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_output_schema_cid: Option<String>,

    /// Optional execution policy for this job.
    #[serde(skip_serializing_if = "Option::is_none")] // Add skip_serializing_if for consistency
    pub execution_policy: Option<ExecutionPolicy>, // ✅ New field
}

impl Default for MeshJobParams {
    fn default() -> Self {
        MeshJobParams {
            wasm_cid: String::new(), // Should be set meaningfully for SingleWasmModule jobs
            description: String::new(),
            resources_required: Vec::new(),
            qos_profile: QoSProfile::BestEffort, // Ensure QoSProfile::BestEffort exists or use an appropriate default
            deadline: None,
            input_data_cid: None,
            max_acceptable_bid_tokens: None,
            workflow_type: WorkflowType::default(),
            stages: None,
            is_interactive: false,
            expected_output_schema_cid: None,
            execution_policy: None, // Add to default
        }
    }
}

/// Represents an organizational scope for a job or receipt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OrgScopeIdentifier {
    pub coop_id: Option<CooperativeId>,
    pub community_id: Option<CommunityId>,
}

/// Represents a mesh computation job within the ICN system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeshJob {
    /// Unique identifier for the job.
    /// e.g., "job_uuid" or a CID derived from parameters + nonce.
    pub job_id: String,
    /// The parameters defining the job.
    pub params: MeshJobParams,
    /// DID of the entity that originated/submitted the job.
    pub originator_did: Did,
    /// Optional organizational scope associated with the job's originator or context.
    pub originator_org_scope: Option<OrgScopeIdentifier>,
    /// Timestamp of when the job was submitted to the ICN, as a Unix timestamp (seconds since epoch).
    pub submission_timestamp: u64,
}

/// Status of a Mesh Job execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobStatus {
    InProgress,
    Completed,
    Failed,
    Cancelled, // Adding this as it was in planetary-mesh JobStatus
} 