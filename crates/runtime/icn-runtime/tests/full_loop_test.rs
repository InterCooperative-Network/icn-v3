use icn_runtime::{
    config::RuntimeConfig,
    Runtime,
    RuntimeContextBuilder,
    MemStorage, // Use MemStorage directly
    Proposal, ProposalState, QuorumStatus,
    RuntimeStorage, // Add trait
    reputation_integration::{HttpReputationUpdater, NoopReputationUpdater},
};
use icn_types::{
    mesh::{MeshJob, MeshJobParams, JobStatus as IcnJobStatus, QoSProfile, WorkflowType, OrgScopeIdentifier},
    resource::ResourceType,
    runtime_receipt::RuntimeExecutionMetrics,
};
use icn_identity::{Did, KeyPair as IcnKeyPair};

// Multihash and CID related imports
use cid::Cid as IcnCid;
use multihash::{Multihash, Code};
use sha2::{Sha256, Digest};

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;
use tracing_subscriber;
use std::time::Duration;

// Helper to initialize tracing for tests, if not already done globally
fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("icn_runtime=debug".parse().unwrap()))
        .try_init();
}

// Helper to create a dummy WASM module bytes (e.g., a simple no-op or add function)
fn dummy_wasm_bytes() -> Vec<u8> {
    // A minimal valid WASM module (wat: (module)) - no exports, no start function
    wat::parse_str("(module)").unwrap()
}

#[tokio::test]
async fn full_runtime_loop_executes_and_anchors_job() -> anyhow::Result<()> {
    init_test_tracing();

    // 1. Set up temp storage and config
    let temp_dir = tempdir()?;
    let wasm_file_path = temp_dir.path().join("test.wasm");

    // Write dummy WASM content (simple no-op module)
    let wat_content = r#"(module (func (export "run")))"#;
    std::fs::write(&wasm_file_path, wat::parse_str(wat_content)?)?;

    let wasm_bytes = std::fs::read(&wasm_file_path)?;
    // Corrected multihash generation
    let mut hasher = Sha256::new();
    hasher.update(&wasm_bytes);
    let hash_result = hasher.finalize();
    let wasm_multihash = Multihash::wrap(Code::Sha2_256.into(), &hash_result).expect("Failed to wrap hash");
    let wasm_cid = IcnCid::new_v1(0x55, wasm_multihash).to_string();

    // 2. Build config - Ensure node has an identity (KeyPair)
    let node_keypair = IcnKeyPair::generate();
    let node_did_str = node_keypair.did.to_string();

    let config = RuntimeConfig { // Config is needed for Runtime::from_config or setting context details
        node_did: node_did_str.clone(),
        storage_path: temp_dir.path().to_path_buf(),
        key_path: None,
        reputation_service_url: None,
        mesh_job_service_url: None,
        metrics_port: None,
        log_level: Some("debug".into()),
    };

    // 3. Build runtime
    // Direct context setup for test clarity:
    let storage_for_runtime: Arc<dyn RuntimeStorage> = Arc::new(MemStorage::new());
    storage_for_runtime.store_wasm(&wasm_cid, &wasm_bytes).await?;

    let mut context_builder = RuntimeContextBuilder::new(); // Use public builder
    context_builder = context_builder.with_executor_id(node_did_str.clone());
    context_builder = context_builder.with_identity(node_keypair.clone());
    let runtime_context = Arc::new(context_builder.build());
    
    // Initialize Runtime with context
    let mut runtime = Runtime::with_context(storage_for_runtime.clone(), runtime_context);
    // If Runtime needs config data not in context, alternative setup might be needed.
    // Assuming context holds enough for this test.
    // Ensure NoopReputationUpdater if needed
    if runtime.context().reputation_service_url().is_none() { 
        runtime = runtime.with_reputation_updater(Arc::new(NoopReputationUpdater));
    }

    // 4. Create job and inject into queue
    let job_originator_did = Did::from_str("did:key:z6MkpTHR8VNsESGeQGSwQy1VBCLeP2g2rM86Zbf3pt12345")?;
    
    let job_params = MeshJobParams {
        wasm_cid: wasm_cid.clone(), 
        description: "Test job for full loop".to_string(),
        resources_required: vec![(ResourceType::Cpu, 1)], 
        qos_profile: QoSProfile::BestEffort,
        deadline: None,
        input_data_cid: None,
        max_acceptable_bid_tokens: None, 
        workflow_type: WorkflowType::SingleWasmModule,
        stages: None,
        is_interactive: false,
        expected_output_schema_cid: None,
        execution_policy: None, 
    };

    let job = MeshJob {
        job_id: Uuid::new_v4().to_string(),
        originator_did: job_originator_did.clone(), // Use clone
        params: job_params,
        originator_org_scope: Some(OrgScopeIdentifier { 
            coop_id: None,
            community_id: None,
        }), 
        submission_timestamp: chrono::Utc::now().timestamp_millis() as u64, // Cast to u64
    };

    {
        let mut queue = runtime.context().pending_mesh_jobs.lock().unwrap();
        queue.push_back(job.clone());
        println!("Job {} pushed to queue. Queue size: {}", job.job_id, queue.len());
    }

    // 5. Spawn runtime in background
    let runtime_clone_for_task = runtime.clone();
    let handle = tokio::spawn(async move {
        println!("Runtime loop starting...");
        if let Err(e) = runtime_clone_for_task.run_forever().await {
            eprintln!("Runtime loop exited with error: {:?}", e);
        }
        println!("Runtime loop finished.");
    });

    // 6. Wait for job to be processed
    println!("Test: Sleeping to allow job processing...");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await; // Adjusted sleep
    println!("Test: Woke up.");

    // 7. Assertions (Example - check if receipt exists)
    let potential_receipt_id = format!("mock-receipt-{}", job.job_id); // Guess based on MemStorage impl
    match storage_for_runtime.load_receipt(&potential_receipt_id).await { // Use correct storage var
        Ok(receipt) => {
             tracing::info!(receipt_id = %receipt.id, "Successfully loaded receipt for job.");
             assert_eq!(receipt.proposal_id, job.job_id, "Receipt proposal ID should match job ID");
        },
        Err(e) => {
            // Fail if receipt *should* have been created
            panic!("Failed to load receipt for job {}: {}. This might indicate the job failed or receipt IDing is different.", job.job_id, e);
        }
    }

    handle.abort();
    println!("Test finished.");

    Ok(())
}

#[tokio::test]
async fn test_full_runtime_loop_with_mem_storage() -> anyhow::Result<()> {
    tracing_subscriber::fmt::try_init().ok();

    let wasm_bytes = dummy_wasm_bytes();
    // --- Corrected multihash generation ---
    let mut hasher = Sha256::new();
    hasher.update(&wasm_bytes);
    let hash_digest = hasher.finalize();
    let wasm_multihash = Multihash::wrap(Code::Sha2_256.into(), &hash_digest)?;
    let wasm_cid = IcnCid::new_v1(0x55, wasm_multihash);
    let wasm_cid_str = wasm_cid.to_string();
    // -------------------------------------

    let storage: Arc<dyn RuntimeStorage> = Arc::new(MemStorage::new());
    storage.store_wasm(&wasm_cid_str, &wasm_bytes).await?;

    let keypair = IcnKeyPair::generate();
    let executor_did = keypair.did.clone();
    let job_originator_did = keypair.did.clone(); // Use same DID

    // --- Use public RuntimeContextBuilder ---
    let mut context_builder = RuntimeContextBuilder::new();
    context_builder = context_builder
        .with_identity(keypair)
        .with_executor_id(executor_did.to_string());
    let runtime_context = Arc::new(context_builder.build());
    // ---------------------------------------

    // --- Use with_context and ensure updater ---
    let mut runtime = Runtime::with_context(storage.clone(), runtime_context.clone());
    if runtime.context().reputation_service_url().is_none() { // Check context directly
        runtime = runtime.with_reputation_updater(Arc::new(NoopReputationUpdater));
    }
    // ----------------------------------------

    let job_id = "test-job-123".to_string();
    let params = MeshJobParams {
        wasm_cid: wasm_cid_str.clone(),
        description: "Test job for mem storage loop".to_string(),
        resources_required: vec![(ResourceType::Cpu, 1)],
        qos_profile: QoSProfile::BestEffort,
        deadline: None,
        input_data_cid: None,
        max_acceptable_bid_tokens: None,
        workflow_type: WorkflowType::SingleWasmModule,
        stages: None,
        is_interactive: false,
        expected_output_schema_cid: None,
        execution_policy: None,
    };

    // --- Corrected MeshJob initialization ---
    let job = MeshJob {
        job_id: job_id.clone(),
        params,
        originator_did: job_originator_did.clone(),
        originator_org_scope: Some(OrgScopeIdentifier {
            coop_id: None,
            community_id: None,
        }),
        submission_timestamp: chrono::Utc::now().timestamp_millis() as u64,
    };
    // --------------------------------------

    runtime_context.pending_mesh_jobs.lock().unwrap().push_back(job.clone());
    tracing::info!(job_id = %job.job_id, "Pushed job to runtime queue");

    // Run the runtime for a short duration to process the job
    let runtime_handle = tokio::spawn(async move {
        // Use a timeout to prevent hanging if the loop has issues
        match tokio::time::timeout(Duration::from_secs(5), runtime.run_forever()).await {
            Ok(Err(e)) => tracing::error!("Runtime loop exited with error: {:?}", e),
            Err(_) => tracing::warn!("Runtime loop timed out"),
            Ok(Ok(_)) => tracing::info!("Runtime loop finished cleanly (unexpected in test)"),
        }
    });

    // Give time for processing
    tokio::time::sleep(Duration::from_secs(2)).await;

    tracing::info!("Test checking for job processing completion...");

    // Check receipt
    let potential_receipt_id = format!("mock-receipt-{}", job_id);
    match storage.load_receipt(&potential_receipt_id).await { // storage is Arc<dyn RuntimeStorage>
        Ok(receipt) => {
             tracing::info!(receipt_id = %receipt.id, "Successfully loaded receipt for job.");
             assert_eq!(receipt.proposal_id, job_id, "Receipt proposal ID should match job ID");
        },
        Err(e) => {
            // Fail if receipt should exist
             panic!("Failed to load receipt for job {}: {}. This might indicate the job failed or receipt IDing is different.", job_id, e);
        }
    }

    runtime_handle.abort();
    Ok(())
} 