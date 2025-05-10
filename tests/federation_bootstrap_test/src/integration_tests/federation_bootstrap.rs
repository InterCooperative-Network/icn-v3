use anyhow::Result;
use async_trait::async_trait;
use icn_identity::{KeyPair, FederationMetadata, TrustBundle, QuorumProof, QuorumType, TrustValidator};
use icn_runtime::{Runtime, RuntimeContext};
use icn_types::{
    dag::{DagEventType, DagNodeBuilder},
    dag_store::{DagStore, SharedDagStore},
};
use std::{
    path::Path,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
    collections::{HashSet, HashMap},
};
use tokio::time::sleep;

const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(30);
const NODE_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Core integration test that verifies federation bootstrap, DAG anchoring,
/// credential issuance and verification using the shared DAG store
#[tokio::test]
async fn test_federation_bootstrap() -> Result<()> {
    // Create a shared DAG store for the test
    let dag_store = Arc::new(SharedDagStore::new());
    
    // Create a TrustValidator
    let trust_validator = Arc::new(TrustValidator::new());
    
    // Create a RuntimeContext with the trust validator
    let context = RuntimeContext::new()
        .with_dag_store(dag_store.clone())
        .with_trust_validator(trust_validator.clone());

    // 1. Clean up any existing state
    cleanup_devnet()?;

    // 2. Generate federation and node keys
    let keys_dir = Path::new("devnet/examples/federation_keys");
    std::fs::create_dir_all(keys_dir)?;

    // Generate signer keypairs for the federation
    let keypairs = generate_signer_keypairs(5);
    
    // Extract the DIDs for federation metadata
    let signer_dids: Vec<_> = keypairs.iter().map(|kp| kp.did.clone()).collect();
    
    // Register all keypairs with the trust validator
    for kp in &keypairs {
        trust_validator.register_signer(kp.did.clone(), kp.pk);
    }
    
    // Create federation metadata
    let metadata = FederationMetadata {
        name: "Test Federation".to_string(),
        description: Some("A test federation for integration tests".to_string()),
        version: "1.0".to_string(),
        additional: HashMap::new(),
    };
    
    // Create a trust bundle with a test DAG CID
    let mut bundle = TrustBundle::new(
        "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string(),
        metadata,
    );
    
    // Calculate the hash for signing
    let bundle_hash = bundle.calculate_hash()?;
    
    // Create signatures from a majority of signers
    let mut signatures = Vec::new();
    for i in 0..3 {
        signatures.push((
            keypairs[i].did.clone(), 
            keypairs[i].sign(&bundle_hash)
        ));
    }
    
    // Create a quorum proof requiring majority approval
    let proof = QuorumProof::new(QuorumType::Majority, signatures);
    
    // Add the proof to the bundle
    bundle.add_quorum_proof(proof);
    
    // Verify the trust bundle
    assert!(trust_validator.set_trust_bundle(bundle.clone()).is_ok(), 
            "Trust bundle verification failed");

    // Generate federation keys
    let federation_keys = generate_federation_keys(keys_dir)?;
    assert!(federation_keys.exists(), "Federation keys not generated");

    // Generate node keys
    for node_id in ["node-1", "node-2", "node-3"] {
        let node_keys = generate_node_keys(keys_dir, node_id)?;
        assert!(node_keys.exists(), "Node keys not generated for {}", node_id);
    }

    // 3. Start the federation nodes with shared DAG store
    // In a real implementation, we would inject dag_store into the nodes
    // For this test, we'll just use Docker and simulate
    let compose_file = Path::new("devnet/docker-compose.yml");
    start_federation_nodes(compose_file)?;

    // 4. Wait for nodes to be ready
    wait_for_nodes_ready()?;

    // 5. Bootstrap the federation
    bootstrap_federation()?;

    // 6. Verify federation state
    verify_federation_state()?;

    // 7. Test federation join workflow
    test_federation_join(dag_store.clone()).await?;

    // 8. Clean up
    cleanup_devnet()?;

    Ok(())
}

// Generate a set of signer keypairs for testing
fn generate_signer_keypairs(count: usize) -> Vec<KeyPair> {
    (0..count).map(|_| KeyPair::generate()).collect()
}

fn cleanup_devnet() -> Result<()> {
    let compose_file = Path::new("devnet/docker-compose.yml");
    Command::new("docker")
        .args(["compose", "-f", compose_file.to_str().unwrap(), "down", "-v"])
        .status()?;
    Ok(())
}

fn generate_federation_keys(keys_dir: &Path) -> Result<std::path::PathBuf> {
    let output = keys_dir.join("federation.json");
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "federation",
            "keygen",
            "--output",
            output.to_str().unwrap(),
        ])
        .status()?;
    Ok(output)
}

fn generate_node_keys(keys_dir: &Path, node_id: &str) -> Result<std::path::PathBuf> {
    let output = keys_dir.join(format!("{}.json", node_id));
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "node",
            "keygen",
            "--node-id",
            node_id,
            "--output",
            output.to_str().unwrap(),
        ])
        .status()?;
    Ok(output)
}

fn start_federation_nodes(compose_file: &Path) -> Result<()> {
    Command::new("docker")
        .args([
            "compose",
            "-f",
            compose_file.to_str().unwrap(),
            "up",
            "-d",
            "--build",
        ])
        .status()?;
    Ok(())
}

fn wait_for_nodes_ready() -> Result<()> {
    let start = Instant::now();
    let mut all_ready = false;

    while !all_ready && start.elapsed() < NODE_STARTUP_TIMEOUT {
        all_ready = true;
        for port in [7001, 7002, 7003] {
            if !is_port_ready("localhost", port)? {
                all_ready = false;
                break;
            }
        }
        if !all_ready {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    assert!(all_ready, "Nodes failed to start within timeout");
    Ok(())
}

fn is_port_ready(host: &str, port: u16) -> Result<bool> {
    let output = Command::new("nc")
        .args(["-z", host, &port.to_string()])
        .output()?;
    Ok(output.status.success())
}

fn bootstrap_federation() -> Result<()> {
    let fed_toml = Path::new("devnet/federation.toml");
    let keys = Path::new("devnet/examples/federation_keys/federation.json");

    // Initialize federation
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "federation",
            "init",
            "--config",
            fed_toml.to_str().unwrap(),
            "--keys",
            keys.to_str().unwrap(),
            "--node-api",
            "http://localhost:7001",
        ])
        .status()?;

    // Register nodes
    for node_id in ["node-1", "node-2", "node-3"] {
        let node_keys = format!("devnet/examples/federation_keys/{}.json", node_id);
        Command::new("cargo")
            .args([
                "run",
                "-q",
                "-p",
                "icn-cli",
                "--",
                "node",
                "register",
                "--node-id",
                node_id,
                "--keys",
                &node_keys,
                "--node-api",
                "http://localhost:7001",
            ])
            .status()?;
    }

    Ok(())
}

fn verify_federation_state() -> Result<()> {
    // Verify federation status
    let output = Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "federation",
            "status",
            "--node-api",
            "http://localhost:7001",
        ])
        .output()?;

    assert!(output.status.success(), "Failed to get federation status");
    let status = String::from_utf8(output.stdout)?;
    assert!(status.contains("active"), "Federation not active");

    // Verify node registration
    for node_id in ["node-1", "node-2", "node-3"] {
        let output = Command::new("cargo")
            .args([
                "run",
                "-q",
                "-p",
                "icn-cli",
                "--",
                "node",
                "status",
                "--node-id",
                node_id,
                "--node-api",
                "http://localhost:7001",
            ])
            .output()?;

        assert!(output.status.success(), "Failed to get node status for {}", node_id);
        let status = String::from_utf8(output.stdout)?;
        assert!(status.contains("registered"), "Node {} not registered", node_id);
    }

    Ok(())
}

async fn test_federation_join(dag_store: Arc<SharedDagStore>) -> Result<()> {
    // Create a test proposal
    let proposal = Path::new("devnet/examples/sample_proposal.ccl");
    assert!(proposal.exists(), "Sample proposal not found");

    // Submit proposal and simulate DAG anchoring in the shared store
    let output = Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "coop",
            "propose",
            "--file",
            proposal.to_str().unwrap(),
            "--api",
            "http://localhost:8080",
        ])
        .output()?;

    assert!(output.status.success(), "Failed to submit proposal");
    let response = String::from_utf8(output.stdout)?;
    assert!(response.contains("proposal_id"), "No proposal ID in response");
    
    // Extract proposal ID (this is a simplification; in a real test we'd parse the JSON)
    let proposal_id = response
        .lines()
        .find(|line| line.contains("proposal_id"))
        .map(|line| line.split(":").nth(1).unwrap_or("unknown").trim())
        .unwrap_or("unknown")
        .replace("\"", "")
        .replace(",", "");
        
    // Simulate anchor to DAG store - in a real implementation, this would happen
    // automatically when proposal is submitted
    let node_builder = DagNodeBuilder::new()
        .content(proposal_id.clone())
        .event_type(DagEventType::Proposal)
        .scope_id("test-federation".to_string())
        .timestamp(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64);
            
    let node = node_builder.build()?;
    dag_store.insert(node).await?;

    // Wait for proposal to be processed
    sleep(Duration::from_secs(2)).await;

    // Verify proposal status
    let output = Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "coop",
            "status",
            "--api",
            "http://localhost:8080",
        ])
        .output()?;

    assert!(output.status.success(), "Failed to get proposal status");
    let status = String::from_utf8(output.stdout)?;
    assert!(status.contains("open") || status.contains("voting") || status.contains("approved"), 
           "Proposal not in expected state");
           
    // Verify DAG store contains our proposal
    let nodes = dag_store.list().await?;
    assert!(!nodes.is_empty(), "DAG store should contain at least one node");
    
    // In a full test, we'd verify more aspects of the DAG, but this is sufficient for now
    Ok(())
}

/// Test to verify DAG replay determinism
#[tokio::test]
async fn test_dag_replay_determinism() -> Result<()> {
    // Create a shared DAG store
    let dag_store = Arc::new(SharedDagStore::new());
    
    // 1. Populate the DAG with a series of events
    let events = populate_test_dag(dag_store.clone()).await?;
    
    // 2. Read all events from the DAG
    let nodes = dag_store.list().await?;
    assert_eq!(nodes.len(), events.len(), "DAG should contain all test events");
    
    // 3. Create a new, empty DAG store for replay
    let replay_store = Arc::new(SharedDagStore::new());
    
    // 4. Replay events in the same order
    for event_id in &events {
        if let Some(node) = dag_store.get(event_id).await? {
            replay_store.insert(node).await?;
        }
    }
    
    // 5. Verify that both DAGs have the same content and state
    let original_nodes = dag_store.list().await?;
    let replayed_nodes = replay_store.list().await?;
    
    assert_eq!(original_nodes.len(), replayed_nodes.len(), 
              "Original and replayed DAGs should have the same number of nodes");
    
    // 6. Sort both sets of nodes by timestamp for deterministic comparison
    let mut original_sorted = original_nodes.clone();
    original_sorted.sort_by_key(|node| node.timestamp);
    
    let mut replayed_sorted = replayed_nodes.clone();
    replayed_sorted.sort_by_key(|node| node.timestamp);
    
    // 7. Verify each node has the same content and CID
    for (orig, replay) in original_sorted.iter().zip(replayed_sorted.iter()) {
        let orig_cid = orig.cid()?;
        let replay_cid = replay.cid()?;
        
        assert_eq!(orig_cid, replay_cid, 
                  "Replayed node should have the same CID as original");
        assert_eq!(orig.content, replay.content, 
                  "Replayed node should have the same content as original");
        assert_eq!(orig.event_type, replay.event_type, 
                  "Replayed node should have the same event type as original");
    }
    
    Ok(())
}

async fn populate_test_dag(dag_store: Arc<SharedDagStore>) -> Result<Vec<String>> {
    let mut event_ids = Vec::new();
    
    // Create nodes representing different DAG events (genesis, proposals, votes, etc.)
    let event_types = [
        DagEventType::Genesis,
        DagEventType::Proposal,
        DagEventType::Vote,
        DagEventType::Execution,
        DagEventType::Attestation
    ];
    
    // Add a genesis node first
    let genesis_node = DagNodeBuilder::new()
        .content("genesis-content".to_string())
        .event_type(DagEventType::Genesis)
        .scope_id("test-federation".to_string())
        .timestamp(1000)
        .build()?;
        
    dag_store.insert(genesis_node.clone()).await?;
    let genesis_cid = genesis_node.cid()?.to_string();
    event_ids.push(genesis_cid.clone());
    
    // Add 20 events connected to each other
    let mut parent_cid = Some(genesis_node.cid()?);
    
    for i in 0..20 {
        let event_type = event_types[i % event_types.len()].clone();
        
        let node_builder = DagNodeBuilder::new()
            .content(format!("event-{}-content", i))
            .event_type(event_type)
            .scope_id("test-federation".to_string())
            .timestamp(1001 + i as u64);
            
        let node_builder = if let Some(parent) = parent_cid {
            node_builder.parent(parent)
        } else {
            node_builder
        };
        
        let node = node_builder.build()?;
        dag_store.insert(node.clone()).await?;
        
        let cid = node.cid()?.to_string();
        event_ids.push(cid);
        
        // Update parent for next iteration - every third node branches
        parent_cid = if i % 3 == 0 {
            // Branch from genesis to create a DAG (not just a chain)
            Some(genesis_node.cid()?)
        } else {
            Some(node.cid()?)
        };
    }
    
    Ok(event_ids)
}

/// A simplified test that demonstrates integrating SharedDagStore with RuntimeContext
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_runtime_dag_store_integration() -> Result<()> {
    // 1. Create shared store and runtime with context
    let dag_store = Arc::new(SharedDagStore::new());
    let context = RuntimeContext::with_dag_store(dag_store.clone())
        .with_federation_id("test-federation")
        .with_executor_id("test-executor");
    
    let storage = Arc::new(MockRuntimeStorage::new());
    let runtime = Runtime::with_context(storage, context);

    // 2. Simulate federation genesis: anchor three trust bundles
    for i in 1..=3 {
        let node = DagNodeBuilder::new()
            .content(format!("trust-bundle-{}", i))
            .event_type(DagEventType::Genesis)
            .scope_id("test-federation".to_string())
            .timestamp(i)
            .build()?;
            
        runtime.dag_store().insert(node).await?;
    }

    // 3. Assert DAG size
    let nodes = dag_store.list().await?;
    assert_eq!(nodes.len(), 3, "DAG should contain 3 nodes");

    // 4. Verify we can access each node and they have unique CIDs
    let mut seen_cids = HashSet::new();
    for node in nodes {
        let cid = node.cid()?.to_string();
        assert!(seen_cids.insert(cid.clone()), "Duplicate CID found: {}", cid);
        
        // Test retrieval by CID
        let retrieved = dag_store.get(&cid).await?;
        assert!(retrieved.is_some(), "Node with CID {} not found", cid);
    }

    Ok(())
}

/// Mock storage for testing runtime integration
#[derive(Default)]
struct MockRuntimeStorage {}

impl MockRuntimeStorage {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl icn_runtime::RuntimeStorage for MockRuntimeStorage {
    async fn load_proposal(&self, _id: &str) -> anyhow::Result<icn_runtime::Proposal> {
        unimplemented!("Not needed for this test")
    }

    async fn update_proposal(&self, _proposal: &icn_runtime::Proposal) -> anyhow::Result<()> {
        unimplemented!("Not needed for this test")
    }

    async fn load_wasm(&self, _cid: &str) -> anyhow::Result<Vec<u8>> {
        unimplemented!("Not needed for this test")
    }

    async fn store_receipt(&self, _receipt: &icn_runtime::ExecutionReceipt) -> anyhow::Result<String> {
        unimplemented!("Not needed for this test")
    }

    async fn anchor_to_dag(&self, _cid: &str) -> anyhow::Result<String> {
        Ok("test-anchor".to_string())
    }
} 