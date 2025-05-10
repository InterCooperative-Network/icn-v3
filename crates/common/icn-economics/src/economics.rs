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
    #[error("insufficient funds for transfer")]
    InsufficientFunds,
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
    
    /// Transfer tokens from one DID to another
    /// Only works for Token resource type
    /// Returns: 
    /// - 0 on success
    /// - -1 on insufficient funds
    /// - -3 on invalid resource type
    pub fn transfer(
        &self,
        sender: &Did,
        recipient: &Did,
        rt: ResourceType,
        amt: u64,
        ledger: &RwLock<HashMap<LedgerKey, u64>>,
    ) -> i32 {
        // Only token type can be transferred
        if rt != ResourceType::Token {
            debug!("Attempted to transfer non-token resource type: {:?}", rt);
            return -3;
        }
        
        debug!("Transferring {} tokens from {} to {}", amt, sender, recipient);
        let mut l = ledger.blocking_write();
        
        // Create keys for sender and recipient
        let sender_key = LedgerKey {
            did: sender.to_string(),
            resource_type: rt,
        };
        
        let recipient_key = LedgerKey {
            did: recipient.to_string(),
            resource_type: rt,
        };
        
        // Check if sender has sufficient balance (remember: lower usage means more tokens)
        // Get the current usage for the sender
        let sender_usage = *l.get(&sender_key).unwrap_or(&0);
        
        // If sender doesn't have enough tokens (usage is too high), return error
        if sender_usage < amt {
            debug!("Insufficient funds: sender {} has usage {}, cannot transfer {}", 
                  sender, sender_usage, amt);
            return -1; // Insufficient funds
        }
        
        // Increase sender's usage (decreasing their token balance)
        l.insert(sender_key, sender_usage + amt);
        
        // Decrease recipient's usage (increasing their token balance)
        let recipient_usage = *l.get(&recipient_key).unwrap_or(&0);
        
        // Check for overflow
        let new_recipient_usage = if recipient_usage < amt {
            0
        } else {
            recipient_usage - amt
        };
        
        // Update recipient's usage
        l.insert(recipient_key, new_recipient_usage);
        
        debug!("Transfer complete. New balances: {} usage={}, {} usage={}",
              sender, sender_usage + amt, recipient, new_recipient_usage);
        0 // Success
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