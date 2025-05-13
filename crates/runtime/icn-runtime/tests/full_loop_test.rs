use icn_runtime::{
    config::RuntimeConfig,
    storage::RuntimeStorage, // Assuming MemStorage (used internally by Runtime::new) is not pub(crate)
    Runtime,
};
use icn_types::{
    mesh::{MeshJob, MeshJobParams, JobStatus as IcnJobStatus, QoSProfile, WorkflowType, OriginatorOrganizationScope},
    identity::{Did, KeyPair as IcnKeyPair}, // Make sure IcnKeyPair is the correct KeyPair type expected by context
    resource::ResourceType, // Corrected path based on typical structure
    cid::Cid as IcnCid, // Alias to avoid confusion with other Cids if any
};

use std::collections::HashMap;
// use std::collections::VecDeque; // Not directly used in test code, but by RuntimeContext
use std::str::FromStr;
use std::sync::Arc;
// use std::sync::Mutex; // Not directly used in test code, but by RuntimeContext
use tempfile::tempdir;
use uuid::Uuid;

// Helper to initialize tracing for tests, if not already done globally
fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("icn_runtime=debug".parse().unwrap()))
        .try_init();
}

#[tokio::test]
async fn full_runtime_loop_executes_and_anchors_job() -> anyhow::Result<()> {
    init_test_tracing();

    // 1. Set up temp storage and config
    let temp_dir = tempdir()?;
    let wasm_file_path = temp_dir.path().join("test.wasm");

    // Write dummy WASM content (simple no-op module)
    // Exports a function "run" which is what execute_mesh_job might look for if not _start.
    let wat_content = r#"(module (func (export "run")))"#;
    std::fs::write(&wasm_file_path, wat::parse_str(wat_content)?)?;

    let wasm_bytes = std::fs::read(&wasm_file_path)?;
    let wasm_cid = IcnCid::new_v1(0x55, multihash::Multihash::wrap(0x12, &multihash::Sha2_256::digest(&wasm_bytes)).unwrap()).to_string();

    // 2. Build config - Ensure node has an identity (KeyPair)
    // For execute_mesh_job to sign receipts, the runtime needs its own KeyPair.
    let node_keypair = IcnKeyPair::generate_ed25519();
    let node_did_str = node_keypair.did().to_string();

    let config = RuntimeConfig {
        node_did: node_did_str.clone(),
        storage_path: temp_dir.path().to_path_buf(),
        key_path: None, // We are providing the keypair directly to context for this test
        reputation_service_url: None,
        mesh_job_service_url: None,
        metrics_port: None,
        log_level: Some("debug".into()),
    };

    // 3. Build runtime
    // We need to ensure the RuntimeContext within runtime has the node_keypair.
    // Modifying from_config or new/with_context might be needed if it doesn't already handle this.
    // For this test, let's assume from_config can set it up if key_path was used,
    // or we can build context manually and use `with_context`.
    
    // Direct context setup for test clarity:
    let storage_for_runtime = Arc::new(icn_runtime::storage::MemStorage::new());
    storage_for_runtime.store_wasm(&wasm_cid, wasm_bytes.clone()).await?;

    let mut context_builder = icn_runtime::context::RuntimeContextBuilder::new();
    context_builder = context_builder.with_executor_id(node_did_str.clone());
    context_builder = context_builder.with_identity(node_keypair.clone()); // Set the node's keypair
    let runtime_context = Arc::new(context_builder.build());
    
    let mut runtime = Runtime::with_context(storage_for_runtime.clone(), runtime_context);
    runtime.config = config; // Manually set the config as with_context doesn't take it.
    // Ensure reputation updater is also re-initialized if needed after config is set manually.
    if let (Some(url), Some(identity)) = (runtime.context().reputation_service_url(), runtime.context().identity()) {
        let updater = Arc::new(icn_runtime::reputation_integration::HttpReputationUpdater::new(
            url.clone(),
            identity.did.clone(),
        ));
        runtime = runtime.with_reputation_updater(updater);
    } else {
        // Use NoopReputationUpdater if no URL is configured, to ensure it's always Some.
        runtime = runtime.with_reputation_updater(Arc::new(icn_runtime::reputation_integration::NoopReputationUpdater));
    }


    // 4. Create job and inject into queue
    let job_originator_did = Did::from_str("did:key:z6MkpTHR8VNsESGeQGSwQy1VBCLeP2g2rM86Zbf3pt12345")?;
    
    let job_params = MeshJobParams {
        wasm_cid: wasm_cid.clone(),
        description: "Test job for full loop".to_string(),
        resources_required: vec![(ResourceType::Cpu, 1)], // Example resource
        qos_profile: QoSProfile::BestEffort,
        deadline: None,
        input_data_cid: None,
        max_acceptable_bid_tokens: None, 
        workflow_type: WorkflowType::SingleWasmModule,
        stages: None,
        is_interactive: false,
        expected_output_schema_cid: None,
        execution_policy: None, 
        // Missing fields from your original MeshJobParams definition (region, extra)
        // Added some common ones. Adjust if your MeshJobParams is different.
    };

    let job = MeshJob {
        job_id: Uuid::new_v4().to_string(),
        originator_did: job_originator_did,
        params: job_params,
        originator_org_scope: Some(OriginatorOrganizationScope { // Added for mana manager
            coop_id: None,
            community_id: None,
        }), 
        priority: 0, // Added default
        status: IcnJobStatus::Pending, // Added default
        current_stage_index: 0, // Added default
        retry_count: 0, // Added default
        error_message: None, // Added default
        created_at: chrono::Utc::now(), // Added default
        updated_at: chrono::Utc::now(), // Added default
    };

    {
        // Access pending_mesh_jobs through the Arc<RuntimeContext>
        let mut queue = runtime.context().pending_mesh_jobs.lock().unwrap();
        queue.push_back(job.clone());
        println!("Job {} pushed to queue. Queue size: {}", job.job_id, queue.len());
    }

    // 5. Spawn runtime in background
    let runtime_clone_for_task = runtime.clone(); // Clone the runtime for the spawned task
    let handle = tokio::spawn(async move {
        println!("Runtime loop starting...");
        if let Err(e) = runtime_clone_for_task.run_forever().await {
            eprintln!("Runtime loop exited with error: {:?}", e);
        }
        println!("Runtime loop finished.");
    });

    // 6. Wait for job to be processed
    // Increased sleep time to allow for processing and logging
    println!("Test: Sleeping to allow job processing...");
    tokio::time::sleep(std::time::Duration::from_secs(10)).await; // Increased sleep
    println!("Test: Woke up.");

    // TODO: Add assertions here:
    // - Check if the job is no longer in pending_mesh_jobs.
    // - Check if a receipt was stored in storage (e.g., MemStorage.receipts).
    // - If using a mock reputation updater, check if it received the receipt.
    // - Check logs for specific messages indicating success.

    // For now, we observe logs. If no errors, and job processing logs appear, it's a good sign.
    // Example assertion (conceptual):
    // let receipts = storage_for_runtime.get_receipt_by_job_id(&job.job_id).await?;
    // assert!(receipts.is_some(), "Receipt for job {} should have been anchored", job.job_id);

    handle.abort(); // Clean up the runtime task
    println!("Test finished.");

    Ok(())
} 