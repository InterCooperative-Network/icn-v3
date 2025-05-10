use anyhow::Result;
use chrono::Utc;
use icn_core_vm::{CoVm, ExecutionMetrics, ResourceLimits};
use icn_economics::ScopedResourceToken;
use icn_identity::Did;
use icn_runtime::Runtime;
use p2p::planetary_mesh::{
    ComputeRequirements, JobManifest, JobPriority, JobStatus, PlanetaryMeshNode, NodeCapability,
};
use std::path::Path;
use std::{fs, str::FromStr};
use uuid::Uuid;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use icn_types::mesh::{MeshJob, MeshJobParams, QoSProfile};
use icn_types::did::Did;
use icn_economics::{Economics, LedgerKey, ResourceAuthorizationPolicy};
use std::sync::RwLock;

/// Test mesh job submission and execution flow
#[tokio::test]
async fn test_mesh_compute_flow() -> Result<()> {
    // Create a test directory
    let test_dir = tempfile::tempdir()?;
    let wasm_path = test_dir.path().join("test_job.wasm");

    // Use the CCL to WASM compiler to generate a test WASM with a job submission
    let ccl_path = Path::new("examples/ccl/mesh_job.ccl");
    let ccl_content = fs::read_to_string(ccl_path)?;
    
    // Instead of compiling, we'll use a mock WASM for testing
    // This simulates what would happen if we compiled the CCL
    fs::write(&wasm_path, &[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00])?;
    
    // Create a test mesh node
    let node_did = Did::from_str("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")?;
    let node_capabilities = NodeCapability {
        node_id: "test-node-1".to_string(),
        node_did: node_did.to_string(),
        available_memory_mb: 4096,
        available_cpu_cores: 8,
        available_storage_mb: 102400,
        cpu_architecture: "x86_64".to_string(),
        features: vec!["avx".to_string(), "sse4".to_string()],
        location: Some("us-west".to_string()),
        bandwidth_mbps: 1000,
        supported_job_types: vec!["compute".to_string()],
        updated_at: Utc::now(),
    };
    let node = PlanetaryMeshNode::new(node_did.to_string(), node_capabilities)?;
    
    // Create a job manifest
    let job_id = Uuid::new_v4().to_string();
    let requirements = ComputeRequirements {
        min_memory_mb: 2048,
        min_cpu_cores: 4,
        min_storage_mb: 10240,
        max_execution_time_secs: 3600,
        required_features: vec![],
    };
    let token = ScopedResourceToken {
        resource_type: "compute".to_string(),
        amount: 1000,
        scope: "data-analysis".to_string(),
        expires_at: None,
        issuer: Some("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string()),
    };
    let manifest = JobManifest {
        id: job_id.clone(),
        submitter_did: "did:key:z6MktyAYM2rE5N2h9kYgqSMv9uCWeP9j9JapH5xJd9XwM7oP".to_string(),
        description: "Test mesh compute job".to_string(),
        created_at: Utc::now(),
        expires_at: None,
        wasm_cid: wasm_path.to_string_lossy().into_owned(),
        ccl_cid: Some(ccl_path.to_string_lossy().into_owned()),
        input_data_cid: None,
        output_location: None,
        requirements,
        priority: JobPriority::Medium,
        resource_token: token,
        trust_requirements: vec![],
        status: JobStatus::Created,
    };
    
    // Submit the job
    let job_id = node.submit_job(manifest).await?;
    
    // Check the job status
    let status = node.get_job_status(&job_id).await?;
    assert!(matches!(status, JobStatus::Submitted));
    
    // Create execution metrics
    let metrics = ExecutionMetrics {
        fuel_used: 5000,
        host_calls: 10,
        io_bytes: 1024,
        anchored_cids_count: 1,
        job_submissions_count: 1,
    };
    
    // Create a receipt
    let receipt = node.create_job_receipt(
        &job_id,
        metrics,
        vec![("compute".to_string(), 500)],
        Some("output-data-cid-123".to_string()),
        0, // success status
        Some("7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069".to_string()), // SHA-256 hash
        Some(r#"{"completion_time_ms": 1500, "memory_peak_mb": 1024}"#.to_string()),
        vec!["Started job".to_string(), "Processing data".to_string(), "Job completed".to_string()],
    ).await?;
    
    // Verify receipt contents
    assert_eq!(receipt.job_id, job_id);
    assert_eq!(receipt.executor_node_id, "test-node-1");
    assert_eq!(receipt.result_status, 0);
    assert_eq!(receipt.resource_usage, vec![("compute".to_string(), 500)]);
    assert_eq!(receipt.execution_logs.len(), 3);
    
    // Get the receipt from the node
    let retrieved_receipt = node.get_job_receipt(&job_id).await?;
    assert!(retrieved_receipt.is_some());
    let retrieved_receipt = retrieved_receipt.unwrap();
    assert_eq!(retrieved_receipt.job_id, job_id);
    
    Ok(())
}

// Helper function for setting up Runtime for mesh compute tests
fn setup_runtime_for_mesh_test(
    caller_did_str: &str,
    initial_token_balance: Option<u64>,
) -> (Runtime, Arc<Mutex<VecDeque<MeshJob>>>, Did) {
    let caller_did = Did::parse(caller_did_str).expect("Failed to parse caller_DID");
    let pending_mesh_jobs = Arc::new(Mutex::new(VecDeque::new()));

    // Setup Economics and initial ledger state
    let policy = ResourceAuthorizationPolicy::default(); // Or some specific test policy
    let economics = Arc::new(Economics::new(policy));
    
    if let Some(balance) = initial_token_balance {
        // Initialize token balance for the caller DID
        // This assumes Economics has a method like `set_balance` or `credit_balance`.
        // For simplicity, let's assume direct ledger manipulation if possible, or use an appropriate Economics API.
        // The `test_resource_economics` test implies interactions via `record_usage` and `get_usage`.
        // A direct balance setting method might be needed for tests or provided by `Economics` test utils.
        // Let's simulate this by setting an initial record that implies a balance.
        // This part is tricky without knowing the exact Economics API for setting initial balances.
        // For now, we'll assume `check_resource_authorization` in ConcreteHostEnvironment can be made to work
        // with a pre-populated ledger if Economics uses one, or a policy.
        // Alternative: If Economics is policy-based for authorization, we set a policy.
        // Given the linker calls `host_env.check_resource_authorization`, we need that method to reflect this balance.
        // The `Economics` struct holds a `resource_ledger: SharedResourceLedger` which is `Arc<RwLock<HashMap<LedgerKey, u64>>>`.
        
        // We need to populate this ledger if check_resource_authorization reads from it.
        let ledger_key = LedgerKey {
            actor_did: caller_did.to_string(), // Assuming LedgerKey uses String for DID
            coop_id: None, // Assuming no specific org scope for this basic test
            community_id: None,
            resource_type: ResourceType::Token,
        };
        economics.resource_ledger.write().unwrap().insert(ledger_key, balance);
    }

    let runtime_context = Arc::new(
        RuntimeContext::builder()
            .with_pending_mesh_jobs(pending_mesh_jobs.clone())
            .with_economics(economics.clone()) // Ensure economics is part of the context
            .build(),
    );

    let host_env = ConcreteHostEnvironment::new(
        caller_did.clone(),
        Arc::new(MockStorage::new()),
        runtime_context,
        Arc::new(NoopTrustValidator),
    );

    let runtime = Runtime::new_with_host_env(Arc::new(Mutex::new(host_env)));
    (runtime, pending_mesh_jobs, caller_did)
}

#[test]
fn test_submit_job_ccl_to_runtime_queue_sufficient_funds() {
    // 1. Define a CCL Test Script
    let ccl_script = r#"
        actions:
          - SubmitJob:
              wasm_cid: "test_wasm_cid_123"
              description: "A test mesh job with sufficient funds"
              input_data_cid: "test_input_cid_456"
              entry_function: "main"
              required_resources_json: "{"Cpu": 100, "Token": 10, "Memory": 256}"
              qos_profile_json: "{"type": "BestEffort"}" 
              max_acceptable_bid_tokens: 50 # Requires 50 tokens
              deadline_utc_ms: 1678886400000
              metadata_json: "{"custom_key": "custom_value"}"
    "#;

    // 2. Compile CCL to WASM
    let dsl_module_list: DslModuleList = parse_ccl(ccl_script).expect("CCL parsing failed");
    let program = compile_dsl_to_program(dsl_module_list).expect("CCL compilation to program failed");
    let wasm_bytes = program_to_wasm(&program);

    // 3. Set Up Runtime with sufficient funds
    let caller_did_str = "did:example:meshjobcaller_rich";
    let (runtime, pending_mesh_jobs, caller_did) = 
        setup_runtime_for_mesh_test(caller_did_str, Some(100)); // Has 100 tokens, needs 50

    // 4. Execute WASM
    let execution_result = runtime.execute_wasm(&wasm_bytes, None, vec![]);
    
    assert!(execution_result.is_ok(), "WASM execution failed: {:?}", execution_result.err());
    let opt_return_values = execution_result.unwrap();
    assert!(opt_return_values.is_some(), "_start function did not return any values");
    let return_values = opt_return_values.unwrap();
    assert_eq!(return_values.len(), 1, "_start function should return exactly one i32 value");
    
    let job_id_len_or_error = return_values[0].i32().expect("_start function return value is not an i32");
    assert!(job_id_len_or_error > 0, "host_submit_mesh_job (via _start) should return positive JobId length with sufficient funds: {}", job_id_len_or_error);

    // 5. Verify Job in Queue
    let jobs_guard = pending_mesh_jobs.lock().unwrap();
    assert_eq!(jobs_guard.len(), 1, "Expected 1 job in the queue with sufficient funds");

    let mesh_job = jobs_guard.front().expect("Job queue is empty after lock");

    assert_eq!(mesh_job.originator_did, caller_did.to_string());
    assert_eq!(mesh_job.params.wasm_cid, "test_wasm_cid_123");
    assert_eq!(mesh_job.params.max_acceptable_bid_tokens, Some(50));
    assert_eq!(mesh_job.params.description, "A test mesh job with sufficient funds", "Description mismatch");
    assert_eq!(mesh_job.params.input_data_cid, Some("test_input_cid_456".to_string()), "Input Data CID mismatch");
    
    let expected_resources: Vec<(ResourceType, u64)> = vec![
        (ResourceType::Cpu, 100),
        (ResourceType::Token, 10),
        (ResourceType::Memory, 256),
    ];
    let mut sorted_actual_resources = mesh_job.params.resources_required.clone();
    sorted_actual_resources.sort_by_key(|k| format!("{:?}", k.0)); 
    assert_eq!(sorted_actual_resources.len(), expected_resources.len(), "Resource count mismatch");
    for (actual, expected) in sorted_actual_resources.iter().zip(expected_resources.iter()) {
        assert_eq!(actual.0, expected.0, "Resource type mismatch");
        assert_eq!(actual.1, expected.1, "Resource amount mismatch for type {:?}", expected.0);
    }
    assert_eq!(mesh_job.params.qos_profile, QoSProfile::BestEffort, "QoS Profile mismatch");
    assert_eq!(mesh_job.params.deadline, Some(1678886400000), "Deadline mismatch");
    assert!(mesh_job.job_id.starts_with("job_"), "JobId format is incorrect");
    assert!(mesh_job.job_id.len() > 4, "JobId seems too short"); 
}

#[test]
fn test_submit_job_ccl_insufficient_funds() {
    // 1. Define a CCL Test Script that requires tokens
    let ccl_script = r#"
        actions:
          - SubmitJob:
              wasm_cid: "test_wasm_cid_insufficient"
              description: "A test mesh job with insufficient funds"
              max_acceptable_bid_tokens: 50 # Requires 50 tokens
              // Other fields can be minimal or None for this test
              required_resources_json: "{"Cpu": 1}" # Minimal resources
              qos_profile_json: "{"type": "BestEffort"}"
    "#;

    // 2. Compile CCL to WASM
    let dsl_module_list: DslModuleList = parse_ccl(ccl_script).expect("CCL parsing failed for insufficient funds test");
    let program = compile_dsl_to_program(dsl_module_list).expect("CCL compilation failed for insufficient funds test");
    let wasm_bytes = program_to_wasm(&program);

    // 3. Set Up Runtime with insufficient funds
    let caller_did_str = "did:example:meshjobcaller_poor";
    // Setup with 10 tokens, but job requires 50
    let (runtime, pending_mesh_jobs, _caller_did) = 
        setup_runtime_for_mesh_test(caller_did_str, Some(10)); 

    // 4. Execute WASM
    let execution_result = runtime.execute_wasm(&wasm_bytes, None, vec![]);
    
    assert!(execution_result.is_ok(), "WASM execution should logically succeed but return error code from host: {:?}", execution_result.err());
    let opt_return_values = execution_result.unwrap();
    assert!(opt_return_values.is_some(), "_start function did not return any values even on error path");
    let return_values = opt_return_values.unwrap();
    assert_eq!(return_values.len(), 1, "_start function should return exactly one i32 value (error code)");
    
    let job_id_len_or_error = return_values[0].i32().expect("_start function return value is not an i32");
    assert_eq!(job_id_len_or_error, -41, "Expected error code -41 (InsufficientFundsForJobBid) but got {}", job_id_len_or_error);

    // 5. Verify Job NOT in Queue
    let jobs_guard = pending_mesh_jobs.lock().unwrap();
    assert_eq!(jobs_guard.len(), 0, "Expected 0 jobs in the queue with insufficient funds");
} 