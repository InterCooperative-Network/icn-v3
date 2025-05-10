#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuntimeContext;
    use anyhow::anyhow;
    use icn_identity::{Did, TrustBundle, TrustValidator, TrustValidationError};
    use std::fs;
    use std::sync::{Arc, Mutex};

    /// A no-op trust validator for testing
    pub struct NoopTrustValidator {
        is_valid: bool,
    }

    impl NoopTrustValidator {
        pub fn new(is_valid: bool) -> Self {
            Self { is_valid }
        }
    }

    impl TrustValidator {
        /// Create a no-op trust validator for tests
        pub fn noop_always_valid() -> Self {
            Self::new()
        }
    }

    // A mock storage implementation for testing
    struct MockStorage {
        proposals: Mutex<Vec<Proposal>>,
        wasm_modules: Mutex<std::collections::HashMap<String, Vec<u8>>>,
        receipts: Mutex<std::collections::HashMap<String, String>>,
        anchored_cids: Mutex<Vec<String>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                proposals: Mutex::new(vec![]),
                wasm_modules: Mutex::new(std::collections::HashMap::new()),
                receipts: Mutex::new(std::collections::HashMap::new()),
                anchored_cids: Mutex::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl RuntimeStorage for MockStorage {
        async fn load_proposal(&self, id: &str) -> Result<Proposal> {
            let proposals = self.proposals.lock().unwrap();
            proposals
                .iter()
                .find(|p| p.id == id)
                .cloned()
                .ok_or_else(|| anyhow!("Proposal not found"))
        }

        async fn update_proposal(&self, proposal: &Proposal) -> Result<()> {
            let mut proposals = self.proposals.lock().unwrap();

            // Remove existing proposal with the same ID
            proposals.retain(|p| p.id != proposal.id);

            // Add the updated proposal
            proposals.push(proposal.clone());

            Ok(())
        }

        async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
            let modules = self.wasm_modules.lock().unwrap();
            modules
                .get(cid)
                .cloned()
                .ok_or_else(|| anyhow!("WASM module not found"))
        }

        async fn store_receipt(&self, receipt: &ExecutionReceipt) -> Result<String> {
            let receipt_json = serde_json::to_string(receipt)?;
            let receipt_cid = format!("receipt-{}", Uuid::new_v4());

            let mut receipts = self.receipts.lock().unwrap();
            receipts.insert(receipt_cid.clone(), receipt_json);

            Ok(receipt_cid)
        }

        async fn anchor_to_dag(&self, cid: &str) -> Result<String> {
            let mut anchored = self.anchored_cids.lock().unwrap();
            anchored.push(cid.to_string());

            let anchor_id = format!("anchor-{}", Uuid::new_v4());
            Ok(anchor_id)
        }
    }

    #[tokio::test]
    async fn test_execute_wasm_file() -> Result<()> {
        // This test requires a compiled WASM file from CCL/DSL
        // For testing, we'll check if the file exists first
        let wasm_path = Path::new("../../../examples/budget.wasm");

        if !wasm_path.exists() {
            println!("Test WASM file not found, skipping test_execute_wasm_file test");
            return Ok(());
        }

        // Read the WASM file
        let wasm_bytes = fs::read(wasm_path)?;

        // Create a runtime with mock storage and trust validator
        let storage = Arc::new(MockStorage::new());
        let trust_validator = Arc::new(TrustValidator::new());
        let context = RuntimeContext::new()
            .with_trust_validator(trust_validator);
        let runtime = Runtime::with_context(storage, context);

        // Create a VM context
        let context = VmContext {
            executor_did: "did:icn:test".to_string(),
            scope: Some("test-scope".to_string()),
            epoch: Some("2023-01-01".to_string()),
            code_cid: Some("test-cid".to_string()),
            resource_limits: None,
        };

        // Execute the WASM module
        let result = runtime.execute_wasm(&wasm_bytes, context.clone())?;

        // Verify that execution succeeded and metrics were collected
        assert!(result.metrics.fuel_used > 0, "Expected fuel usage metrics");

        // Test trust bundle verification
        let test_bundle = TrustBundle::new(
            "test-cid".to_string(),
            icn_identity::FederationMetadata {
                name: "Test Federation".to_string(),
                description: Some("Test Description".to_string()),
                version: "1.0".to_string(),
                additional: std::collections::HashMap::new(),
            }
        );
        
        // This will fail because no signers are registered and no quorum proof is added
        assert!(runtime.verify_trust_bundle(&test_bundle).is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_trust_validator_integration() -> Result<()> {
        // Create a runtime with trust validator
        let storage = Arc::new(MockStorage::new());
        let trust_validator = Arc::new(TrustValidator::new());
        let context = RuntimeContext::new()
            .with_trust_validator(trust_validator.clone());
        let runtime = Runtime::with_context(storage, context);

        // Generate a test keypair
        let kp = icn_identity::KeyPair::generate();
        
        // Register as a trusted signer
        runtime.register_trusted_signer(kp.did.clone(), kp.pk)?;
        
        // Check if the signer is authorized
        let is_authorized = runtime.is_authorized_signer(&kp.did)?;
        assert!(is_authorized, "Signer should be authorized");

        // Test host_get_trust_bundle
        let result = runtime.host_get_trust_bundle("test-cid").await?;
        assert!(result, "host_get_trust_bundle should return true with a configured trust validator");

        Ok(())
    }
} 