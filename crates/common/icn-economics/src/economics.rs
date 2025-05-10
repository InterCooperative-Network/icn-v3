use crate::{policy::ResourceAuthorizationPolicy, types::ResourceType};
use icn_identity::Did;
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::RwLock;
use log::debug;

#[derive(Debug, Error)]
pub enum EconomicsError {
    #[error("unauthorized resource usage")]
    Unauthorized,
}

/// Represents a key for the resource ledger, combining DID and resource type
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct LedgerKey {
    pub did: String,
    pub resource_type: ResourceType,
}

pub struct Economics {
    policy: ResourceAuthorizationPolicy,
}

impl Economics {
    pub fn new(policy: ResourceAuthorizationPolicy) -> Self { Self { policy } }

    pub fn authorize(&self, caller: &Did, rt: ResourceType, amt: u64) -> i32 {
        debug!("Authorizing {} units of {:?} for {}", amt, rt, caller);
        if self.policy.authorized(rt, amt) { 
            0 
        } else { 
            debug!("Authorization denied for {} to use {} units of {:?}", caller, amt, rt);
            -1 
        }
    }

    pub fn record(
        &self,
        caller: &Did,
        rt: ResourceType,
        amt: u64,
        ledger: &RwLock<HashMap<LedgerKey, u64>>,
    ) -> i32 {
        debug!("Recording {} units of {:?} for {}", amt, rt, caller);
        let mut l = ledger.blocking_write();
        let key = LedgerKey {
            did: caller.to_string(),
            resource_type: rt,
        };
        *l.entry(key).or_insert(0) += amt;
        0
    }
    
    /// Mint tokens for a DID, which reduces their token usage (increases token allowance)
    /// Only works for Token resource type
    pub fn mint(
        &self,
        recipient: &Did,
        rt: ResourceType,
        amt: u64,
        ledger: &RwLock<HashMap<LedgerKey, u64>>,
    ) -> i32 {
        // Only token type can be minted
        if rt != ResourceType::Token {
            debug!("Attempted to mint non-token resource type: {:?}", rt);
            return -3;
        }
        
        debug!("Minting {} tokens for {}", amt, recipient);
        let mut l = ledger.blocking_write();
        let key = LedgerKey {
            did: recipient.to_string(),
            resource_type: rt,
        };
        
        // Get the current usage and subtract the amount (minting reduces usage)
        let current = l.entry(key.clone()).or_insert(0);
        
        // Check for overflow
        if *current < amt {
            *current = 0;
        } else {
            *current -= amt;
        }
        
        debug!("New token balance for {}: {}", recipient, *current);
        0
    }
    
    /// Get the usage of a specific resource type for a specific DID
    pub async fn get_usage(&self, caller: &Did, rt: ResourceType, ledger: &RwLock<HashMap<LedgerKey, u64>>) -> u64 {
        let l = ledger.read().await;
        let key = LedgerKey {
            did: caller.to_string(),
            resource_type: rt,
        };
        *l.get(&key).unwrap_or(&0)
    }
    
    /// Get the total usage of a specific resource type across all DIDs
    pub async fn get_total_usage(&self, rt: ResourceType, ledger: &RwLock<HashMap<LedgerKey, u64>>) -> u64 {
        let l = ledger.read().await;
        l.iter()
            .filter(|(k, _)| k.resource_type == rt)
            .map(|(_, v)| *v)
            .sum()
    }
} 