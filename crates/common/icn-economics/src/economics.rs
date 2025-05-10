use crate::{policy::ResourceAuthorizationPolicy, types::ResourceType};
use icn_identity::Did;
use icn_types::org::{CooperativeId, CommunityId};
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

/// Represents a key for the resource ledger, combining DID, organization scope, and resource type
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct LedgerKey {
    pub did: String,
    /// Optional cooperative ID that this ledger entry is associated with
    pub coop_id: Option<String>,
    /// Optional community ID that this ledger entry is associated with
    pub community_id: Option<String>,
    pub resource_type: ResourceType,
}

pub struct Economics {
    policy: ResourceAuthorizationPolicy,
}

impl Economics {
    pub fn new(policy: ResourceAuthorizationPolicy) -> Self { Self { policy } }

    pub fn authorize(
        &self,
        caller: &Did,
        coop_id: Option<&CooperativeId>,
        community_id: Option<&CommunityId>,
        rt: ResourceType,
        amt: u64
    ) -> i32 {
        debug!("Authorizing {} units of {:?} for {} (coop: {:?}, community: {:?})",
              amt, rt, caller, coop_id, community_id);
        if self.policy.authorized(rt, amt) { 
            0 
        } else { 
            debug!("Authorization denied for {} to use {} units of {:?}", caller, amt, rt);
            -1 
        }
    }

    /// Record resource usage for a specific DID
    pub async fn record(
        &self,
        caller: &Did,
        coop_id: Option<&CooperativeId>,
        community_id: Option<&CommunityId>,
        rt: ResourceType,
        amt: u64,
        ledger: &RwLock<HashMap<LedgerKey, u64>>,
    ) -> i32 {
        debug!("Recording {} units of {:?} for {} (coop: {:?}, community: {:?})",
              amt, rt, caller, coop_id, community_id);
        let mut l = ledger.write().await;
        let key = LedgerKey {
            did: caller.to_string(),
            coop_id: coop_id.map(|c| c.to_string()),
            community_id: community_id.map(|c| c.to_string()),
            resource_type: rt,
        };
        *l.entry(key).or_insert(0) += amt;
        0
    }
    
    /// Mint tokens for a DID, which reduces their token usage (increases token allowance)
    /// Only works for Token resource type
    pub async fn mint(
        &self,
        recipient: &Did,
        coop_id: Option<&CooperativeId>,
        community_id: Option<&CommunityId>,
        rt: ResourceType,
        amt: u64,
        ledger: &RwLock<HashMap<LedgerKey, u64>>,
    ) -> i32 {
        // Only token type can be minted
        if rt != ResourceType::Token {
            debug!("Attempted to mint non-token resource type: {:?}", rt);
            return -3;
        }
        
        debug!("Minting {} tokens for {} (coop: {:?}, community: {:?})",
              amt, recipient, coop_id, community_id);
        let mut l = ledger.write().await;
        let key = LedgerKey {
            did: recipient.to_string(),
            coop_id: coop_id.map(|c| c.to_string()),
            community_id: community_id.map(|c| c.to_string()),
            resource_type: rt,
        };
        
        // Get the current usage and subtract the amount (minting reduces usage)
        // In our token model, lower usage means more tokens
        let current = l.entry(key.clone()).or_insert(0);
        
        // Check for overflow - ensure usage doesn't go negative
        if *current < amt {
            *current = 0;
        } else {
            *current -= amt;
        }
        
        let token_max: u64 = 100; // Maximum token allowance
        let available_tokens = token_max.saturating_sub(*current);
        debug!("New token balance for {}: {} tokens (usage: {})", recipient, available_tokens, *current);
        0
    }
    
    /// Transfer tokens from one DID to another
    /// Only works for Token resource type
    /// Returns: 
    /// - 0 on success
    /// - -1 on insufficient funds
    /// - -3 on invalid resource type
    pub async fn transfer(
        &self,
        sender: &Did,
        sender_coop_id: Option<&CooperativeId>,
        sender_community_id: Option<&CommunityId>,
        recipient: &Did,
        recipient_coop_id: Option<&CooperativeId>,
        recipient_community_id: Option<&CommunityId>,
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
        let mut l = ledger.write().await;
        
        // Create keys for sender and recipient
        let sender_key = LedgerKey {
            did: sender.to_string(),
            coop_id: sender_coop_id.map(|c| c.to_string()),
            community_id: sender_community_id.map(|c| c.to_string()),
            resource_type: rt,
        };
        
        let recipient_key = LedgerKey {
            did: recipient.to_string(),
            coop_id: recipient_coop_id.map(|c| c.to_string()),
            community_id: recipient_community_id.map(|c| c.to_string()),
            resource_type: rt,
        };
        
        // In our token model, a usage of 0 means full tokens, and higher usage means fewer tokens
        // Get the current usage for the sender
        let sender_usage = *l.get(&sender_key).unwrap_or(&0);
        
        // Check if the sender has enough tokens (represented by available headroom)
        // The amount must be able to fit in the sender's available "usage headroom"
        // For example: if someone has a usage of 80, they have 20 tokens available to transfer
        // If someone has a usage of 0, they have 100 tokens available (assuming 100 is the max)
        let token_max: u64 = 100; // Maximum token allowance
        let available_tokens = token_max.saturating_sub(sender_usage);
        
        // If sender doesn't have enough available tokens, return insufficient funds
        if available_tokens < amt {
            debug!("Insufficient funds: sender {} has usage {} (available tokens: {}), cannot transfer {}", 
                  sender, sender_usage, available_tokens, amt);
            return -1; // Insufficient funds
        }
        
        // Increase sender's usage (decreasing their token balance)
        l.insert(sender_key, sender_usage + amt);
        
        // Decrease recipient's usage (increasing their token balance)
        let recipient_usage = *l.get(&recipient_key).unwrap_or(&0);
        
        // Check for overflow - ensure recipient's usage doesn't go negative
        // Decreasing usage means giving tokens to the recipient
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
    pub async fn get_usage(
        &self,
        caller: &Did,
        coop_id: Option<&CooperativeId>,
        community_id: Option<&CommunityId>,
        rt: ResourceType,
        ledger: &RwLock<HashMap<LedgerKey, u64>>
    ) -> u64 {
        let l = ledger.read().await;
        let key = LedgerKey {
            did: caller.to_string(),
            coop_id: coop_id.map(|c| c.to_string()),
            community_id: community_id.map(|c| c.to_string()),
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
    
    /// Get the total usage of a specific resource type for a cooperative
    pub async fn get_cooperative_usage(
        &self,
        coop_id: &CooperativeId,
        rt: ResourceType,
        ledger: &RwLock<HashMap<LedgerKey, u64>>
    ) -> u64 {
        let l = ledger.read().await;
        l.iter()
            .filter(|(k, _)| {
                k.resource_type == rt && 
                k.coop_id.as_ref().map_or(false, |cid| cid == &coop_id.to_string())
            })
            .map(|(_, v)| *v)
            .sum()
    }
    
    /// Get the total usage of a specific resource type for a community
    pub async fn get_community_usage(
        &self,
        community_id: &CommunityId,
        rt: ResourceType,
        ledger: &RwLock<HashMap<LedgerKey, u64>>
    ) -> u64 {
        let l = ledger.read().await;
        l.iter()
            .filter(|(k, _)| {
                k.resource_type == rt && 
                k.community_id.as_ref().map_or(false, |cid| cid == &community_id.to_string())
            })
            .map(|(_, v)| *v)
            .sum()
    }
} 