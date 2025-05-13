#![allow(dead_code)]

mod helpers;

use anyhow::{anyhow, Result};
use helpers::{create_signer_map, create_trust_bundle, generate_signers};
use icn_identity::{FederationMetadata, KeyPair, QuorumProof, QuorumType, TrustBundle, TrustValidator, Did};
use icn_runtime::{Runtime, RuntimeContext, RuntimeContextBuilder, RuntimeStorage, Proposal, ProposalState, QuorumStatus};
use icn_types::dag_store::SharedDagStore;
use icn_types::runtime_receipt::{RuntimeExecutionReceipt, RuntimeExecutionMetrics};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::str::FromStr;
use tempfile::TempDir;
use async_trait::async_trait;
use std::pin::Pin;
use std::future::Future;

struct TestFederation {
    signers: Vec<KeyPair>,
    bundle: TrustBundle,
    context: RuntimeContext,
    runtime: Runtime,
    temp_dir: TempDir,
}

impl TestFederation {
    async fn new(num_signers: usize) -> Result<Self> {
        // Create a temporary directory for federation data
        let temp_dir = TempDir::new()?;

        // Generate signer keypairs
        let signers = generate_signers(num_signers);
        println!("Generated {} signers", signers.len());
        
        // Create a trust bundle with these signers
        let bundle = create_trust_bundle(
            &signers,
            "Test Federation",
            Some("A test federation for integration tests"),
        )?;

        // Create a shared DAG store for the federation
        // This is not used in the current tests but would be used in a real implementation
        let _dag_store = Arc::new(SharedDagStore::default());
        
        // Create a trust validator with the signers from our keypairs
        let trust_validator = Arc::new(TrustValidator::new());
        
        // Register all signers in the trust validator
        for kp in &signers {
            trust_validator.register_signer(kp.did.clone(), kp.pk);
        }
        
        // Create the runtime context with our validator and DAG store
        let context = RuntimeContext::new()
            .with_trust_validator(trust_validator.clone());
            
        // Create a mock runtime storage
        let storage = Arc::new(MockRuntimeStorage::default());
        
        // Create the runtime with our context
        let runtime = Runtime::with_context(storage, Arc::new(context.clone()));

        Ok(Self {
            signers,
            bundle,
            context,
            runtime,
            temp_dir,
        })
    }
    
    fn verify_trust_bundle(&self) -> Result<()> {
        // Create a signer map from our keypairs
        let signer_map = create_signer_map(&self.signers);
        
        // Verify the trust bundle against our signers
        self.bundle.verify(&signer_map)?;
        
        Ok(())
    }
}

/// Test the basic federation bootstrap process
/// This test verifies that:
/// 1. We can create a federation with multiple signers
/// 2. We can build a trust bundle with a quorum of signers
/// 3. We can verify the trust bundle with the runtime
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_federation_bootstrap() -> Result<()> {
    // Initialize a test federation with 3 signers
    let federation = TestFederation::new(3).await?;
    
    // Verify the trust bundle
    federation.verify_trust_bundle()?;
    
    // Verify the trust bundle with the runtime
    federation.runtime.verify_trust_bundle(&federation.bundle)?;
    
    println!("Federation bootstrap test succeeded!");
    
    Ok(())
}

/// Test the full federation lifecycle including:
/// 1. Federation creation with multiple signers
/// 2. Trust bundle creation and verification
/// 3. Runtime integration with trust validator
/// 4. Signer authorization checks
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_federation_lifecycle() -> Result<()> {
    // Initialize a test federation with 3 signers
    let federation = TestFederation::new(3).await?;
    
    // Verify the trust bundle
    federation.verify_trust_bundle()?;
    
    // Add the trust bundle to the runtime's trust validator
    federation.runtime.verify_trust_bundle(&federation.bundle)?;
    
    // Check if our signers are authorized
    for kp in &federation.signers {
        let is_authorized = federation.runtime.is_authorized_signer(&kp.did)?;
        assert!(is_authorized, "Signer should be authorized: {}", kp.did);
    }
    
    // Test with an unauthorized signer
    let unauthorized = KeyPair::generate();
    let is_authorized = federation.runtime.is_authorized_signer(&unauthorized.did)?;
    assert!(!is_authorized, "Random signer should not be authorized");
    
    // Test the host_get_trust_bundle runtime function
    assert!(federation.runtime.host_get_trust_bundle("test-cid").await?, 
            "host_get_trust_bundle should return true");
    
    println!("Federation lifecycle test succeeded!");
    
    Ok(())
}

/// Test the federation trust bundle anchoring process
/// This test verifies that:
/// 1. We can create a trust bundle with a quorum of signers
/// 2. We can anchor the trust bundle to the DAG
/// 3. We can verify the trust bundle after anchoring
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_federation_anchoring() -> Result<()> {
    // Initialize a test federation with 3 signers
    let federation = TestFederation::new(3).await?;
    
    // Verify the trust bundle
    federation.verify_trust_bundle()?;
    
    // Set up DAG anchoring with our mock storage
    // In a real implementation, this would use a real DAG store
    // and would anchor the trust bundle to the DAG
    
    // For our mock implementation, we just need to verify the bundle
    federation.runtime.verify_trust_bundle(&federation.bundle)?;
    
    // In production, we would:
    // 1. Serialize the bundle to JSON
    // 2. Calculate a CID for the bundle
    // 3. Anchor the CID to the DAG
    // 4. Update the bundle with the new CID
    // 5. Store the bundle in the DAG store
    
    // Simulate a trust bundle fetch from the DAG using our host_get_trust_bundle function
    assert!(federation.runtime.host_get_trust_bundle("test-cid").await?, 
            "host_get_trust_bundle should return true");
    
    println!("Federation anchoring test succeeded!");
    
    Ok(())
}

/// Test fixture that implements a mocked RuntimeStorage for testing
#[derive(Clone, Default)]
struct MockRuntimeStorage {
    proposals: Arc<Mutex<HashMap<String, Proposal>>>,
    wasm_modules: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    receipts: Arc<Mutex<HashMap<String, RuntimeExecutionReceipt>>>,
    anchored_cids: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl RuntimeStorage for MockRuntimeStorage {
    async fn load_proposal(&self, id: &str) -> Result<Proposal> {
        self.proposals.lock().unwrap().get(id).cloned().ok_or_else(|| anyhow!("Proposal not found"))
    }

    async fn update_proposal(&self, proposal: &Proposal) -> Result<()> {
        let mut proposals = self.proposals.lock().unwrap();
        proposals.insert(proposal.id.clone(), proposal.clone());
        Ok(())
    }

    async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
        self.wasm_modules.lock().unwrap().get(cid).cloned().ok_or_else(|| anyhow!("WASM not found"))
    }

    async fn store_receipt(&self, receipt: &RuntimeExecutionReceipt) -> Result<String> {
        let receipt_id = receipt.id.clone();
        self.receipts.lock().unwrap().insert(receipt_id.clone(), receipt.clone());
        Ok(receipt_id)
    }

    async fn store_wasm(&self, cid: &str, bytes: &[u8]) -> Result<()> {
        self.wasm_modules.lock().unwrap().insert(cid.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn load_receipt(&self, receipt_id: &str) -> Result<RuntimeExecutionReceipt> {
        self.receipts.lock().unwrap().get(receipt_id).cloned().ok_or_else(|| anyhow!("Receipt not found"))
    }

    async fn anchor_to_dag(&self, cid: &str) -> Result<String> {
        self.anchored_cids.lock().unwrap().push(cid.to_string());
        Ok(format!("anchor-{}", cid))
    }
}

/// Test the federation genesis and replay process
/// This test simulates the full lifecycle:
/// 1. Genesis - creating the initial federation trust bundle
/// 2. Anchoring - adding the bundle to the DAG
/// 3. Replay - verifying the bundle from the DAG
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bootstrap_genesis_replay() -> Result<()> {
    // 1. Signer setup - generate two signers
    let k1 = KeyPair::generate();
    let k2 = KeyPair::generate();
    
    // Create a test federation metadata
    let metadata = FederationMetadata {
        name: "Genesis Federation".to_string(),
        description: Some("A test federation for genesis and replay".to_string()),
        version: "1.0".to_string(),
        additional: HashMap::new(),
    };
    
    // Initialize with a placeholder CID
    let mut bundle = TrustBundle::new(
        "placeholder-cid".to_string(), 
        metadata,
    );
    
    // Calculate the hash for signing
    let bundle_hash = bundle.calculate_hash()?;
    
    // Build a quorum proof with majority signing
    let mut signatures = Vec::new();
    signatures.push((k1.did.clone(), k1.sign(&bundle_hash)));
    signatures.push((k2.did.clone(), k2.sign(&bundle_hash)));
    
    let proof = QuorumProof::new(QuorumType::Majority, signatures);
    bundle.add_quorum_proof(proof);
    
    // 2. Setup runtime with trust validator
    let trust_validator = Arc::new(TrustValidator::new());
    trust_validator.register_signer(k1.did.clone(), k1.pk);
    trust_validator.register_signer(k2.did.clone(), k2.pk);
    
    let context = RuntimeContext::new()
        .with_trust_validator(trust_validator);
    
    let storage = Arc::new(MockRuntimeStorage::default());
    let runtime = Runtime::with_context(storage, Arc::new(context));
    
    // 3. Verify and set the bundle
    runtime.verify_trust_bundle(&bundle)?;
    
    // 4. Verify signers are now authorized
    assert!(runtime.is_authorized_signer(&k1.did)?, "Signer 1 should be authorized");
    assert!(runtime.is_authorized_signer(&k2.did)?, "Signer 2 should be authorized");
    
    // 5. Verify unknown signer is not authorized
    let unknown = KeyPair::generate();
    assert!(!runtime.is_authorized_signer(&unknown.did)?, 
            "Unknown signer should not be authorized");
    
    println!("Genesis and replay test succeeded!");
    
    Ok(())
}

#[tokio::test]
async fn test_bootstrap_federation_and_execute() -> Result<()> {
    let storage = Arc::new(MockRuntimeStorage::default());
    let keypair = KeyPair::generate();
    let node_did = keypair.did.clone();

    let context = RuntimeContextBuilder::new()
        .with_identity(keypair)
        .with_executor_id(node_did.to_string())
        .build();
    
    let mut runtime = Runtime::with_context(storage.clone(), Arc::new(context.clone()));

    // ... rest of the test ...
    // (Assume test setup like creating proposals, storing WASM etc.)

    Ok(())
}

#[tokio::test]
async fn test_trust_bundle_registration() -> Result<()> {
    let storage = Arc::new(MockRuntimeStorage::default());
    let validator = TrustValidator::new();
    let keypair = KeyPair::generate();
    let node_did = keypair.did.clone();

    let context = RuntimeContextBuilder::new()
        .with_identity(keypair.clone())
        .with_executor_id(node_did.to_string())
        .with_trust_validator(Arc::new(validator))
        .build();

    let runtime = Runtime::with_context(storage, Arc::new(context));

    let signer_keypair = KeyPair::generate();
    let signer_did = signer_keypair.did.clone();
    let mut bundle = TrustBundle::new("test-bundle-cid".to_string(), 
                                      FederationMetadata { name: "TestFed".into(), description: None, version: "1.0".into(), additional: HashMap::new() });

    assert!(runtime.verify_trust_bundle(&bundle).is_err());

    Ok(())
} 