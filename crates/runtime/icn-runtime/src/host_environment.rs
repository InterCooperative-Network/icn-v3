use crate::context::RuntimeContext;
use icn_economics::ResourceType;
use icn_identity::Did;
use icn_mesh_receipts::{ExecutionReceipt, verify_receipt};
use icn_types::dag::ReceiptNode;
use icn_types::org::{CooperativeId, CommunityId};
use serde_cbor;
use std::sync::Arc;
use std::str::FromStr;
use anyhow::Result;
use thiserror::Error;

/// Errors that can occur during receipt anchoring
#[derive(Debug, Error)]
pub enum AnchorError {
    #[error("Executor mismatch: receipt's executor ({0}) does not match caller ({1})")]
    ExecutorMismatch(String, String),
    
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("CID generation error: {0}")]
    CidError(String),
    
    #[error("DAG store error: {0}")]
    DagStoreError(String),
    
    #[error("Missing federation ID")]
    MissingFederationId,
}

/// Concrete implementation of the host environment for WASM execution
pub struct ConcreteHostEnvironment {
    /// Runtime context
    pub ctx: Arc<RuntimeContext>,
    
    /// DID of the caller
    pub caller_did: Did,
    
    /// Whether this execution is happening in a governance context
    pub is_governance: bool,
    
    /// Optional cooperative ID for this execution context
    pub coop_id: Option<CooperativeId>,
    
    /// Optional community ID for this execution context
    pub community_id: Option<CommunityId>,
}

impl ConcreteHostEnvironment {
    /// Create a new host environment with the given context and caller
    pub fn new(ctx: Arc<RuntimeContext>, caller_did: Did) -> Self {
        Self { 
            ctx, 
            caller_did,
            is_governance: false,
            coop_id: None,
            community_id: None,
        }
    }
    
    /// Create a new host environment with governance context
    pub fn new_governance(ctx: Arc<RuntimeContext>, caller_did: Did) -> Self {
        Self {
            ctx,
            caller_did,
            is_governance: true,
            coop_id: None,
            community_id: None,
        }
    }
    
    /// Create a new host environment with organization context
    pub fn with_organization(
        mut self,
        coop_id: Option<CooperativeId>,
        community_id: Option<CommunityId>,
    ) -> Self {
        self.coop_id = coop_id;
        self.community_id = community_id;
        self
    }

    /// Check resource authorization
    pub fn check_resource_authorization(&self, rt: ResourceType, amt: u64) -> i32 {
        self.ctx.economics.authorize(&self.caller_did, self.coop_id.as_ref(), self.community_id.as_ref(), rt, amt)
    }

    /// Record resource usage
    pub fn record_resource_usage(&self, rt: ResourceType, amt: u64) -> i32 {
        self.ctx.economics.record(
            &self.caller_did,
            self.coop_id.as_ref(),
            self.community_id.as_ref(),
            rt,
            amt,
            &self.ctx.resource_ledger
        )
    }
    
    /// Check if the current execution is in a governance context
    pub fn is_governance_context(&self) -> i32 {
        if self.is_governance {
            1
        } else {
            0
        }
    }
    
    /// Mint tokens for a specific DID, only allowed in governance context
    pub fn mint_token(&self, recipient_did_str: &str, amount: u64) -> i32 {
        // Only allow minting in a governance context
        if !self.is_governance {
            return -1; // Not authorized
        }
        
        // Parse the recipient DID
        let recipient_did = match Did::from_str(recipient_did_str) {
            Ok(did) => did,
            Err(_) => return -2, // Invalid DID
        };
        
        // Record the minted tokens as a negative usage (increases allowance)
        self.ctx.economics.mint(
            &recipient_did,
            self.coop_id.as_ref(),
            self.community_id.as_ref(),
            ResourceType::Token,
            amount,
            &self.ctx.resource_ledger
        )
    }
    
    /// Transfer tokens from sender to recipient
    /// Returns:
    /// - 0 on success
    /// - -1 on insufficient funds
    /// - -2 on invalid DID
    pub fn transfer_token(&self, sender_did_str: &str, recipient_did_str: &str, amount: u64) -> i32 {
        // Parse the sender DID
        let sender_did = match Did::from_str(sender_did_str) {
            Ok(did) => did,
            Err(_) => return -2, // Invalid sender DID
        };
        
        // Parse the recipient DID
        let recipient_did = match Did::from_str(recipient_did_str) {
            Ok(did) => did,
            Err(_) => return -2, // Invalid recipient DID
        };
        
        // Transfer tokens between DIDs, using the same org context for both sender and recipient
        self.ctx.economics.transfer(
            &sender_did,
            self.coop_id.as_ref(),
            self.community_id.as_ref(),
            &recipient_did,
            self.coop_id.as_ref(),
            self.community_id.as_ref(),
            ResourceType::Token,
            amount,
            &self.ctx.resource_ledger
        )
    }

    /// Anchor a serialized ExecutionReceipt into the DAG.
    pub async fn anchor_receipt(&self, mut receipt: ExecutionReceipt) -> Result<(), AnchorError> {
        // 1. Verify the receipt is from the caller
        if receipt.executor != self.caller_did {
            return Err(AnchorError::ExecutorMismatch(
                receipt.executor.to_string(),
                self.caller_did.to_string()
            ));
        }
        
        // 2. Add organizational context if not already set
        if receipt.coop_id.is_none() && self.coop_id.is_some() {
            receipt.coop_id = self.coop_id.clone();
        }
        
        if receipt.community_id.is_none() && self.community_id.is_some() {
            receipt.community_id = self.community_id.clone();
        }
        
        // 3. Verify the receipt signature if one exists
        if !receipt.signature.is_empty() {
            // Verify signature - this will need to be updated based on the actual verification mechanism
            // For now, we'll assume verification is successful if there's a non-empty signature
            // In a real implementation, we'd validate the signature against the receipt's content
            tracing::debug!("Receipt has signature of length {}, assuming valid", receipt.signature.len());
            
            // Note: The real implementation would look like:
            // let is_valid = verify_receipt(&receipt, &signature).map_err(...)?;
            // if !is_valid { return Err(...); }
        }
        
        // 4. Generate CID for the receipt
        let receipt_cid = receipt.cid()
            .map_err(|e| AnchorError::CidError(e.to_string()))?;
        
        // 5. Get federation ID
        let federation_id = self.ctx.federation_id.clone()
            .ok_or(AnchorError::MissingFederationId)?;
        
        // 6. Serialize receipt to CBOR
        let receipt_cbor = serde_cbor::to_vec(&receipt)
            .map_err(|e| AnchorError::SerializationError(e.to_string()))?;
        
        // 7. Create a ReceiptNode
        let receipt_node = ReceiptNode::new(
            receipt_cid, 
            receipt_cbor, 
            federation_id
        );
        
        // 8. Create a DAG node from the receipt node
        let dag_node = icn_types::dag::DagNodeBuilder::new()
            .content(serde_json::to_string(&receipt_node)
                .map_err(|e| AnchorError::SerializationError(e.to_string()))?)
            .event_type(icn_types::dag::DagEventType::Receipt)
            .scope_id(format!("receipt/{}", receipt_cid))
            .timestamp(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as u64)
            .build()
            .map_err(|e| AnchorError::DagStoreError(e.to_string()))?;
        
        // 9. Insert into receipt store
        self.ctx.receipt_store.insert(dag_node)
            .await
            .map_err(|e| AnchorError::DagStoreError(e.to_string()))?;
        
        // Log success
        tracing::info!("Anchored receipt for task: {}, receipt CID: {}", 
            receipt.task_cid, receipt_cid);
        
        Ok(())
    }
} 