use crate::context::RuntimeContext;
use icn_economics::ResourceType;
use icn_identity::Did;
use std::sync::Arc;
use std::str::FromStr;

/// Concrete implementation of the host environment for WASM execution
pub struct ConcreteHostEnvironment {
    /// Runtime context
    pub ctx: Arc<RuntimeContext>,
    
    /// DID of the caller
    pub caller_did: Did,
    
    /// Whether this execution is happening in a governance context
    pub is_governance: bool,
}

impl ConcreteHostEnvironment {
    /// Create a new host environment with the given context and caller
    pub fn new(ctx: Arc<RuntimeContext>, caller_did: Did) -> Self {
        Self { 
            ctx, 
            caller_did,
            is_governance: false,
        }
    }
    
    /// Create a new host environment with governance context
    pub fn new_governance(ctx: Arc<RuntimeContext>, caller_did: Did) -> Self {
        Self {
            ctx,
            caller_did,
            is_governance: true,
        }
    }

    /// Check resource authorization
    pub fn check_resource_authorization(&self, rt: ResourceType, amt: u64) -> i32 {
        self.ctx.economics.authorize(&self.caller_did, rt, amt)
    }

    /// Record resource usage
    pub fn record_resource_usage(&self, rt: ResourceType, amt: u64) -> i32 {
        self.ctx.economics.record(&self.caller_did, rt, amt, &self.ctx.resource_ledger)
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
        self.ctx.economics.mint(&recipient_did, ResourceType::Token, amount, &self.ctx.resource_ledger)
    }
} 