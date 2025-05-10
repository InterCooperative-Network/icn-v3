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