use crate::types::ResourceType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceAuthorizationPolicy {
    pub max_cpu: u64,
    pub max_memory: u64,
    pub token_allowance: u64,
}

impl Default for ResourceAuthorizationPolicy {
    fn default() -> Self {
        Self { max_cpu: 1_000_000, max_memory: 512 * 1024 * 1024, token_allowance: 1_000 }
    }
}

impl ResourceAuthorizationPolicy {
    pub fn authorized(&self, rt: ResourceType, amt: u64) -> bool {
        use ResourceType::*;
        match rt {
            Cpu    => amt <= self.max_cpu,
            Memory => amt <= self.max_memory,
            Token  => amt <= self.token_allowance,
            Io     => true, // unlimited for now
        }
    }
} 