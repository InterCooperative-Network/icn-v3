use crate::context::RuntimeContext;
use icn_economics::ResourceType;
use icn_identity::Did;
use std::sync::Arc;

/// Concrete implementation of the host environment for WASM execution
pub struct ConcreteHostEnvironment {
    /// Runtime context
    pub ctx: Arc<RuntimeContext>,
    
    /// DID of the caller
    pub caller_did: Did,
}

impl ConcreteHostEnvironment {
    /// Create a new host environment with the given context and caller
    pub fn new(ctx: Arc<RuntimeContext>, caller_did: Did) -> Self {
        Self { ctx, caller_did }
    }

    /// Check resource authorization
    pub fn check_resource_authorization(&self, rt: ResourceType, amt: u64) -> i32 {
        self.ctx.economics.authorize(&self.caller_did, rt, amt)
    }

    /// Record resource usage
    pub fn record_resource_usage(&self, rt: ResourceType, amt: u64) -> i32 {
        self.ctx.economics.record(&self.caller_did, rt, amt, &self.ctx.resource_ledger)
    }
} 