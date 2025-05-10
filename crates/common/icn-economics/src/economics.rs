use crate::{policy::ResourceAuthorizationPolicy, types::ResourceType};
use icn_identity::Did;
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum EconomicsError {
    #[error("unauthorized resource usage")]
    Unauthorized,
}

pub struct Economics {
    policy: ResourceAuthorizationPolicy,
}

impl Economics {
    pub fn new(policy: ResourceAuthorizationPolicy) -> Self { Self { policy } }

    pub fn authorize(&self, _caller: &Did, rt: ResourceType, amt: u64) -> i32 {
        if self.policy.authorized(rt, amt) { 0 } else { -1 }
    }

    pub fn record(
        &self,
        _caller: &Did,
        rt: ResourceType,
        amt: u64,
        ledger: &RwLock<HashMap<ResourceType, u64>>,
    ) -> i32 {
        let mut l = ledger.blocking_write();
        *l.entry(rt).or_insert(0) += amt;
        0
    }
} 