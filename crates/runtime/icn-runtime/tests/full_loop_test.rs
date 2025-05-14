use anyhow::{anyhow, Context, Result};
use cid::Cid as IcnCid;
use icn_economics::mana::{InMemoryManaLedger, ManaRegenerator, RegenerationPolicy, ManaState, ManaLedger};
use icn_economics::ResourceType;
use icn_identity::{Did, KeyPair as IcnKeyPair, ScopeKey, TrustBundle};
use icn_mesh_receipts::ExecutionReceipt as MeshExecutionReceipt;
// use icn_proposal_dag::ProposalDag; // User needs to add this dependency
use icn_runtime::{
    config::RuntimeConfig,
    reputation_integration::{NoopReputationUpdater, ReputationScoringConfig as ReputationConfig, ReputationRecord},
    MemStorage,
    Proposal,
    Runtime,
    RuntimeContextBuilder,
    RuntimeStorage,
    VmContext,
};
use icn_types::{
    dag_store::DagStore,
    mesh::{CommunityId, CooperativeId, MeshJob, MeshJobParams, OrgScopeIdentifier, QoSProfile, WorkflowType},
    runtime_receipt::RuntimeExecutionReceipt,
    VerifiableReceipt,
};
use multihash::{Code, Multihash};
use prometheus::Registry;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::info;
use tracing_subscriber::fmt::TestWriter;
use url::Url;
use uuid::Uuid;
use wat::parse_str;
use async_trait::async_trait;

// Helper to initialize tracing for tests, if not already done globally
fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("icn_runtime=debug".parse().unwrap()),
        )
        .try_init();
}

// Helper to create a dummy WASM module bytes (e.g., a simple no-op or add function)
fn dummy_wasm_bytes() -> Vec<u8> {
    // A minimal valid WASM module (wat: (module)) - no exports, no start function
    wat::parse_str("(module)").unwrap()
}

// --- Mock Reputation Updater for Mana Deduction (local to this test file) ---

#[derive(Debug, Clone)]
struct TestManaDeductionCall {
    executor_did: Did,
    amount: u64,
    coop_id: String,
    community_id: String,
}

#[derive(Clone, Debug, Default)]
struct TestMockReputationUpdater {
    mana_deductions: Arc<Mutex<Vec<TestManaDeductionCall>>>,
}

impl TestMockReputationUpdater {
    fn new() -> Self {
        Default::default()
    }

    fn get_mana_deductions(&self) -> Vec<TestManaDeductionCall> {
        self.mana_deductions.lock().unwrap().clone()
    }
}

#[async_trait]
impl icn_runtime::reputation_integration::ReputationUpdater for TestMockReputationUpdater {
    async fn submit_receipt_based_reputation(
        &self,
        _receipt: &icn_types::runtime_receipt::RuntimeExecutionReceipt,
        _is_successful: bool,
        _coop_id: &str,
        _community_id: &str,
    ) -> anyhow::Result<()> {
        Ok(()) // No-op for this part
    }

    async fn submit_mana_deduction(
        &self,
        executor_did: &Did,
        amount: u64,
        coop_id: &str,
        community_id: &str,
    ) -> anyhow::Result<()> {
        self.mana_deductions
            .lock()
            .unwrap()
            .push(TestManaDeductionCall {
                executor_did: executor_did.clone(),
                amount,
                coop_id: coop_id.to_string(),
                community_id: community_id.to_string(),
            });
        Ok(())
    }
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
    let wasm_multihash =
        Multihash::wrap(Code::Sha2_256.into(), &hash_result).expect("Failed to wrap hash");
    let wasm_cid = IcnCid::new_v1(0x55, wasm_multihash).to_string();

    // 2. Build config - Ensure node has an identity (KeyPair)
    let node_keypair = IcnKeyPair::generate();
    let node_did_str = node_keypair.did.to_string();

    let config = RuntimeConfig {
        node_did: node_did_str.clone(),
        storage_path: temp_dir.path().to_path_buf(),
        key_path: None,
        reputation_service_url: None,
        mesh_job_service_url: None,
        metrics_port: None,
        log_level: Some("debug".into()),
        reputation_scoring_config_path: None,
        mana_regeneration_policy: None,
        mana_tick_interval_seconds: Some(30),
    };

    // 3. Build runtime
    // Direct context setup for test clarity:
    let storage_for_runtime: Arc<dyn RuntimeStorage> = Arc::new(MemStorage::new());
    storage_for_runtime
        .store_wasm(&wasm_cid, &wasm_bytes)
        .await?;

    let mut context_builder = RuntimeContextBuilder::<InMemoryManaLedger>::new(); // Added generic type
    context_builder = context_builder.with_executor_id(node_did_str.clone());
    context_builder = context_builder.with_identity(node_keypair.clone());
    // Add a default ManaRegenerator as it's often required by build()
    let mana_ledger_for_job = Arc::new(InMemoryManaLedger::new());
    let policy_for_job = RegenerationPolicy::FixedRatePerTick(0); // Default no-op policy
    let regenerator_for_job = Arc::new(ManaRegenerator::new(mana_ledger_for_job, policy_for_job));
    context_builder = context_builder.with_mana_regenerator(regenerator_for_job);
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
    let job_originator_keypair = IcnKeyPair::generate();
    let job_originator_did = job_originator_keypair.did;

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
        explicit_mana_cost: None, // Added missing field
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
        println!(
            "Job {} pushed to queue. Queue size: {}",
            job.job_id,
            queue.len()
        );
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
    match storage_for_runtime
        .load_receipt(&potential_receipt_id)
        .await
    {
        // Use correct storage var
        Ok(receipt) => {
            tracing::info!(receipt_id = %receipt.id, "Successfully loaded receipt for job.");
            assert_eq!(
                receipt.proposal_id, job.job_id,
                "Receipt proposal ID should match job ID"
            );
        }
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
    let mut context_builder = RuntimeContextBuilder::<InMemoryManaLedger>::new(); // Added generic type
    context_builder = context_builder
        .with_identity(keypair)
        .with_executor_id(executor_did.to_string());
    // Add a default ManaRegenerator
    let mana_ledger_for_loop = Arc::new(InMemoryManaLedger::new());
    let policy_for_loop = RegenerationPolicy::FixedRatePerTick(0);
    let regenerator_for_loop = Arc::new(ManaRegenerator::new(mana_ledger_for_loop, policy_for_loop));
    context_builder = context_builder.with_mana_regenerator(regenerator_for_loop);
    let runtime_context = Arc::new(context_builder.build());
    // ---------------------------------------

    // --- Use with_context and ensure updater ---
    let mut runtime = Runtime::with_context(storage.clone(), runtime_context.clone());
    if runtime.context().reputation_service_url().is_none() {
        // Check context directly
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
        explicit_mana_cost: None, // Added missing field
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

    runtime_context
        .pending_mesh_jobs
        .lock()
        .unwrap()
        .push_back(job.clone());
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
    match storage.load_receipt(&potential_receipt_id).await {
        // storage is Arc<dyn RuntimeStorage>
        Ok(receipt) => {
            tracing::info!(receipt_id = %receipt.id, "Successfully loaded receipt for job.");
            assert_eq!(
                receipt.proposal_id, job_id,
                "Receipt proposal ID should match job ID"
            );
        }
        Err(e) => {
            // Fail if receipt should exist
            panic!("Failed to load receipt for job {}: {}. This might indicate the job failed or receipt IDing is different.", job_id, e);
        }
    }

    // --- Use receipt_count to verify a receipt was stored ---
    // let count = storage.receipt_count(); // Removed this line as receipt_count() doesn't exist
    // assert!(count > 0, "Expected at least one receipt to be stored, found {}", count);
    // ---------------------------------------------------------

    runtime_handle.abort();
    Ok(())
}

#[tokio::test]
async fn test_reputation_mana_pipeline() -> anyhow::Result<()> {
    init_test_tracing();

    // 1. Set up WASM
    let wasm_bytes = dummy_wasm_bytes();
    let mut hasher = Sha256::new();
    hasher.update(&wasm_bytes);
    let hash_result = hasher.finalize();
    let wasm_multihash =
        Multihash::wrap(Code::Sha2_256.into(), &hash_result).expect("Failed to wrap hash");
    let wasm_cid = IcnCid::new_v1(0x55, wasm_multihash).to_string();

    // 2. Node Identity & Mock Updater
    let node_keypair = IcnKeyPair::generate();
    let node_did = node_keypair.did.clone();
    let node_did_str = node_did.to_string();
    let mock_reputation_updater = Arc::new(TestMockReputationUpdater::new());

    // 3. Build runtime with Mock Updater
    let storage_for_runtime: Arc<dyn RuntimeStorage> = Arc::new(MemStorage::new());
    storage_for_runtime
        .store_wasm(&wasm_cid, &wasm_bytes)
        .await?;

    let mut context_builder = RuntimeContextBuilder::<InMemoryManaLedger>::new();
    context_builder = context_builder
        .with_identity(node_keypair.clone())
        .with_executor_id(node_did_str.clone())
        .with_federation_id("test-federation-mana-pipeline".to_string()); // For coop/community scope
    let runtime_context = Arc::new(context_builder.build());

    let mut runtime = Runtime::with_context(storage_for_runtime.clone(), runtime_context)
        .with_reputation_updater(mock_reputation_updater.clone()
            as Arc<dyn icn_runtime::reputation_integration::ReputationUpdater>);

    // 4. Create job with mana_cost
    let job_originator_keypair = IcnKeyPair::generate();
    let job_originator_did = job_originator_keypair.did;

    let mana_to_cost = 75u64;

    let job_params = MeshJobParams {
        wasm_cid: wasm_cid.clone(),
        description: "Test job for mana pipeline".to_string(),
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
        explicit_mana_cost: Some(mana_to_cost), // Set explicit mana cost
    };

    let job = MeshJob {
        job_id: Uuid::new_v4().to_string(),
        originator_did: job_originator_did.clone(),
        params: job_params,
        originator_org_scope: Some(OrgScopeIdentifier {
            coop_id: Some(CooperativeId::new("test-coop".to_string())), // Corrected type
            community_id: Some(CommunityId::new("test-community".to_string())), // Corrected type
        }),
        submission_timestamp: chrono::Utc::now().timestamp_millis() as u64,
    };

    // Push job to queue
    runtime
        .context()
        .pending_mesh_jobs
        .lock()
        .unwrap()
        .push_back(job.clone());
    tracing::info!(job_id = %job.job_id, "Pushed job with mana_cost to runtime queue");

    // 5. Spawn runtime in background
    let runtime_clone_for_task = runtime.clone();
    let _handle = tokio::spawn(async move {
        // Changed handle to _handle as it's not awaited here before abort
        match tokio::time::timeout(Duration::from_secs(5), runtime_clone_for_task.run_forever())
            .await
        {
            Ok(Err(e)) => tracing::error!("Runtime loop (mana test) exited with error: {:?}", e),
            Err(_) => tracing::warn!("Runtime loop (mana test) timed out"),
            Ok(Ok(_)) => tracing::info!("Runtime loop (mana test) finished cleanly (unexpected)"),
        }
    });

    // 6. Wait for job to be processed
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // 7. Assertions
    // Assert receipt creation
    // The receipt ID logic in Runtime::issue_receipt is internal.
    // We need a way to find the receipt. Assuming it gets stored in the DAG store (receipt_store).
    // RuntimeContext has `dag_store` and `receipt_store`. Runtime::anchor_receipt uses `self.context.dag_store`.
    // Let's try to list nodes in dag_store and find one matching our job.
    let dag_nodes = runtime.context().dag_store.list().await?;
    let mut found_receipt: Option<icn_types::runtime_receipt::RuntimeExecutionReceipt> = None;

    for node in dag_nodes {
        // The content of the DagNode is expected to be a JSON string of RuntimeExecutionReceipt
        if let Ok(receipt_content) = serde_json::from_str::<
            icn_types::runtime_receipt::RuntimeExecutionReceipt,
        >(&node.content)
        {
            if receipt_content.proposal_id == job.job_id {
                found_receipt = Some(receipt_content);
                break;
            }
        } else {
            // Log node.content if it's not the expected JSON or handle other DagNode types if necessary
            tracing::debug!(cid = %node.cid()?.to_string(), content_str = %node.content, "DAG node content not a RuntimeExecutionReceipt JSON or deserialization failed");
        }
    }

    assert!(
        found_receipt.is_some(),
        "Receipt for job {} should have been created and anchored.",
        job.job_id
    );
    if let Some(ref receipt) = found_receipt {
        assert_eq!(
            receipt.metrics.mana_cost,
            Some(mana_to_cost),
            "Receipt metrics should reflect mana_cost"
        );
    }

    // Assert mana deduction
    let deductions = mock_reputation_updater.get_mana_deductions();
    assert_eq!(deductions.len(), 1, "Expected one mana deduction call");

    let deduction = &deductions[0];
    // The executor_did for mana deduction will be the runtime's own DID (node_did)
    // because execute_mesh_job sets local_keypair.did as executor_did.
    assert_eq!(
        deduction.executor_did, node_did,
        "Mana should be deducted from the runtime/node DID"
    );
    assert_eq!(
        deduction.amount, mana_to_cost,
        "Deducted mana amount should match job's mana_cost"
    );
    assert_eq!(
        deduction.coop_id, "test-federation-mana-pipeline",
        "Coop ID for deduction should match federation ID from context"
    );
    assert_eq!(
        deduction.community_id, "test-federation-mana-pipeline",
        "Community ID for deduction should match federation ID from context"
    );

    // _handle.abort(); // Abort the runtime task
    // No need to abort if it's expected to finish or timeout. If it's truly `run_forever`, then abort.
    // The previous tests used handle.abort(), let's keep it for now.
    // However, the handle was shadowed in the spawned task. Re-exposing it.
    // The handle is from tokio::spawn, so it must be awaited or aborted.
    // For this test, since run_forever is... forever, aborting is fine.
    // Let's ensure the handle used for abort is the one from tokio::spawn.
    // The variable `_handle` was used. Let's rename it for clarity if we abort.
    // The timeout in the spawn makes abort less critical if it exits cleanly on timeout.
    // Let's assume the timeout handles graceful exit for the test.

    tracing::info!("Mana pipeline test finished.");
    Ok(())
}

#[tokio::test]
async fn test_mana_regeneration_loop_ticks() -> anyhow::Result<()> {
    // Initialize tracing for debug output (optional but helpful)
    let _ = tracing_subscriber::fmt()
        .with_env_filter("icn_runtime=debug,icn_economics=debug")
        .try_init();

    // Generate a test DID
    let test_user_did = IcnKeyPair::generate().did;

    // Create an InMemoryManaLedger and set initial mana
    let ledger = Arc::new(InMemoryManaLedger::default());
    ledger
        .set_initial_state(
            test_user_did.clone(),
            ManaState {
                current_mana: 50,
                max_mana: 100,
                regen_rate_per_epoch: 0.0, // Changed to f64
                last_updated_epoch: 0, 
            },
        )
        .await; // set_initial_state is async in the provided code for InMemoryManaLedger

    // Create a ManaRegenerator with fixed regen rate (10 per tick)
    let policy = RegenerationPolicy::FixedRatePerTick(10);
    let regenerator = Arc::new(ManaRegenerator::new(ledger.clone(), policy));

    // Build RuntimeContext with the regenerator
    // RuntimeContextBuilder is generic, defaults to InMemoryManaLedger
    let context_builder = RuntimeContextBuilder::<InMemoryManaLedger>::new()
        .with_identity(IcnKeyPair::generate()) // Runtime needs an identity
        .with_executor_id("test-runtime-did-for-mana-regen".to_string()) // And an executor ID
        .with_mana_regenerator(regenerator.clone());

    let runtime_context = Arc::new(context_builder.build());

    // Create a dummy storage for the runtime
    let dummy_storage = Arc::new(MemStorage::new());

    // Runtime::with_context is generic, specify InMemoryManaLedger
    let runtime =
        Runtime::<InMemoryManaLedger>::with_context(dummy_storage, runtime_context.clone());

    // Spawn runtime task. The run_forever loop has a 30s tick interval for mana.
    info!("Spawning runtime for mana regeneration test...");
    let runtime_handle = tokio::spawn(runtime.run_forever());

    // Wait long enough for at least one regeneration tick (e.g., 35 seconds for a 30s interval)
    info!("Test sleeping for 35 seconds to allow mana tick...");
    tokio::time::sleep(Duration::from_secs(35)).await;
    info!("Test woke up, checking mana state...");

    // Check mana state after tick
    let updated_state_option = ledger.get_mana_state(&test_user_did).await?;
    assert!(
        updated_state_option.is_some(),
        "ManaState should exist for the DID"
    );

    let updated_state = updated_state_option.unwrap();
    // Initial: 50, Regen: 10 per tick. Expected: 50 + 10 = 60
    assert_eq!(
        updated_state.current_mana, 60,
        "Mana should have regenerated by 10 units"
    );

    info!(
        "Mana regeneration test successful. Final mana: {}",
        updated_state.current_mana
    );

    // Clean up the runtime task
    runtime_handle.abort();

    Ok(())
}

#[tokio::test]
async fn test_mana_regeneration_policy_from_config() -> anyhow::Result<()> {
    init_test_tracing(); // Ensure tracing is initialized

    let test_user_did = IcnKeyPair::generate().did;
    let regeneration_amount = 7u64;
    let initial_mana = 5u64;
    let expected_mana_after_tick = initial_mana + regeneration_amount;

    // 1. Create RuntimeConfig with a specific mana regeneration policy
    let temp_dir = tempdir()?;
    let storage_path = temp_dir.path().to_path_buf();
    let keypair_path = temp_dir.path().join("test_runtime_key.bin");

    let config = RuntimeConfig {
        node_did: "test-node-did-for-config-test".to_string(), 
        storage_path: storage_path.clone(), 
        mana_regeneration_policy: Some(RegenerationPolicy::FixedRatePerTick(regeneration_amount)),
        key_path: Some(keypair_path.clone()), // Use a key_path for load_or_generate_keypair
        reputation_service_url: None, 
        mesh_job_service_url: None,   
        metrics_port: None,           
        log_level: Some("debug".to_string()),
        reputation_scoring_config_path: None,
        mana_tick_interval_seconds: Some(30), 
    };

    // 2. Construct Runtime manually based on config values
    let keypair = icn_runtime::load_or_generate_keypair(config.key_path.as_deref())?;
    let node_did_str = keypair.did.to_string(); // Get DID from loaded/generated keypair

    // Storage (assuming SledStorage was the intent for from_config with a path)
    // For tests, using MemStorage if Sled requires more setup or is heavier.
    // However, to match `from_config` intent with `storage_path`, Sled would be chosen.
    // Let's use MemStorage for simplicity in this refactor if Sled is problematic for tests.
    // If the original `from_config` ALWAYS used Sled when path is Some, then Sled is correct.
    // The user's main.rs refactor used SledStorage. So we use SledStorage here too.
    let sled_storage = Arc::new(icn_runtime::sled_storage::SledStorage::new(&storage_path)?);
    let runtime_storage: Arc<dyn RuntimeStorage> = sled_storage.clone();

    // Mana System (Using InMemoryManaLedger for now due to SledManaLedger visibility issues)
    let mana_ledger = Arc::new(InMemoryManaLedger::new()); // Changed to InMemoryManaLedger
    let policy = config.mana_regeneration_policy.unwrap_or_else(|| RegenerationPolicy::FixedRatePerTick(0));
    let mana_regenerator = Arc::new(ManaRegenerator::new(mana_ledger.clone(), policy));

    // Reputation Updater
    let reputation_updater: Option<Arc<dyn icn_runtime::reputation_integration::ReputationUpdater>> = 
        config.reputation_service_url.as_ref().map(|url| {
            Arc::new(icn_runtime::reputation_integration::HttpReputationUpdater::new(
                url.clone(), 
                keypair.did.clone()
            )) as Arc<dyn icn_runtime::reputation_integration::ReputationUpdater>
        });

    let mut context_builder = RuntimeContextBuilder::<InMemoryManaLedger>::new() // Changed to InMemoryManaLedger
        .with_identity(keypair)
        .with_executor_id(node_did_str)
        .with_mana_regenerator(mana_regenerator.clone())
        .with_dag_store(Arc::new(icn_types::dag_store::SharedDagStore::new())) // Add default dag_store
        .with_federation_id("test-federation-from-config".to_string()); // Add default federation_id
    
    if let Some(tick_interval) = config.mana_tick_interval_seconds {
        context_builder = context_builder.with_mana_tick_interval(Duration::from_secs(tick_interval));
    }

    let runtime_context = Arc::new(context_builder.build());

    let mut runtime = Runtime::<InMemoryManaLedger>::with_context(runtime_storage, runtime_context.clone()); // Changed to InMemoryManaLedger
    if let Some(rep_updater) = reputation_updater {
        runtime = runtime.with_reputation_updater(rep_updater);
    }

    // 3. Get the ledger from the runtime (it was created by the setup above).
    let ledger_from_runtime = mana_ledger; // We already have the Arc to the InMemoryManaLedger

    // 4. Set initial state on this InMemoryManaLedger instance.
    ledger_from_runtime
        .update_mana_state(
            &test_user_did,
            ManaState {
                current_mana: initial_mana,
                max_mana: 100,
                regen_rate_per_epoch: 0.0, // Changed to f64
                last_updated_epoch: 0,   
            },
        )
        .await?;

    // 5. Spawn runtime task.
    info!("Spawning runtime for mana regeneration (from_config with InMemoryManaLedger) test...");
    let runtime_handle = tokio::spawn(runtime.run_forever());

    // 6. Wait long enough for at least one regeneration tick.
    // Use the mana_tick_interval_seconds from config + a buffer
    let tick_interval_secs = 30;
    info!(
        "Test sleeping for {} seconds to allow mana tick...",
        tick_interval_secs + 5
    );
    tokio::time::sleep(Duration::from_secs(tick_interval_secs + 5)).await;
    info!("Test woke up, checking mana state (from_config with InMemoryManaLedger)...");

    // 7. Assert on the ledger.
    let updated_state_option = ledger_from_runtime.get_mana_state(&test_user_did).await?;
    assert!(
        updated_state_option.is_some(),
        "ManaState should exist for the DID (from_config with InMemoryManaLedger)"
    );

    let updated_state = updated_state_option.unwrap();
    assert_eq!(
        updated_state.current_mana, expected_mana_after_tick,
        "Mana should have regenerated by {} units as per config. Expected {}, got {}",
        regeneration_amount, expected_mana_after_tick, updated_state.current_mana
    );

    info!(
        "Mana regeneration (from_config with InMemoryManaLedger) test successful. Final mana: {}. Expected: {}",
        updated_state.current_mana, expected_mana_after_tick
    );

    // Clean up the runtime task and temp directory
    runtime_handle.abort();
    temp_dir.close()?;

    Ok(())
}
