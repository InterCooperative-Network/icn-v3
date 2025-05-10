use icn_economics::{Economics, LedgerKey, ResourceAuthorizationPolicy, ResourceType};
use icn_identity::Did;
use icn_types::org::{CooperativeId, CommunityId};
use std::collections::HashMap;
use std::str::FromStr;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_scoped_resource_usage() {
    // Create a simple economics system with a default policy
    let economics = Economics::new(ResourceAuthorizationPolicy::default());
    let ledger = RwLock::new(HashMap::<LedgerKey, u64>::new());
    
    // Create test DIDs and organization IDs
    let did = Did::from_str("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")
        .expect("Failed to create test DID");
    let coop_id = CooperativeId::new("coop-123");
    let community_id = CommunityId::new("community-456");
    let different_coop = CooperativeId::new("different-coop");
    
    // Record usage within organization scope
    economics.record(
        &did, 
        Some(&coop_id),
        Some(&community_id),
        ResourceType::Cpu, 
        100,
        &ledger
    );
    
    // Get usage for the same scope - should be 100
    let usage = economics.get_usage(
        &did,
        Some(&coop_id),
        Some(&community_id),
        ResourceType::Cpu,
        &ledger
    ).await;
    assert_eq!(usage, 100, "Usage within the same organization scope should be 100");
    
    // Get usage for different coop scope - should be 0
    let usage = economics.get_usage(
        &did,
        Some(&different_coop),
        Some(&community_id),
        ResourceType::Cpu,
        &ledger
    ).await;
    assert_eq!(usage, 0, "Usage within a different coop should be 0");
    
    // Get usage for no org scope - should be 0
    let usage = economics.get_usage(
        &did,
        None,
        None,
        ResourceType::Cpu,
        &ledger
    ).await;
    assert_eq!(usage, 0, "Usage with no organization scope should be 0");
    
    // Record additional usage in the same scope
    economics.record(
        &did, 
        Some(&coop_id),
        Some(&community_id),
        ResourceType::Cpu, 
        50,
        &ledger
    );
    
    // Get updated usage for the same scope - should be 150
    let usage = economics.get_usage(
        &did,
        Some(&coop_id),
        Some(&community_id),
        ResourceType::Cpu,
        &ledger
    ).await;
    assert_eq!(usage, 150, "Usage should accumulate within the same scope");
    
    // Record usage with coop but no community
    economics.record(
        &did, 
        Some(&coop_id),
        None,
        ResourceType::Cpu, 
        75,
        &ledger
    );
    
    // Get usage for coop only scope - should be 75
    let usage = economics.get_usage(
        &did,
        Some(&coop_id),
        None,
        ResourceType::Cpu,
        &ledger
    ).await;
    assert_eq!(usage, 75, "Usage within coop-only scope should be 75");
    
    // Test cooperative usage aggregation
    let coop_usage = economics.get_cooperative_usage(
        &coop_id,
        ResourceType::Cpu,
        &ledger
    ).await;
    assert_eq!(coop_usage, 225, "Total cooperative usage should be 225 (150 + 75)");
}

#[tokio::test]
async fn test_scoped_token_operations() {
    // Create a simple economics system with a default policy
    let economics = Economics::new(ResourceAuthorizationPolicy::default());
    let ledger = RwLock::new(HashMap::<LedgerKey, u64>::new());
    
    // Create test DIDs and organization IDs
    let user1 = Did::from_str("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK")
        .expect("Failed to create user1 DID");
    let user2 = Did::from_str("did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH")
        .expect("Failed to create user2 DID");
    let coop_id = CooperativeId::new("coop-123");
    let community_id = CommunityId::new("community-456");
    
    // Mint 100 tokens for user1 in coop scope
    economics.mint(
        &user1,
        Some(&coop_id),
        None,
        ResourceType::Token,
        100,
        &ledger
    );
    
    // Mint 50 tokens for user1 in community scope
    economics.mint(
        &user1,
        Some(&coop_id),
        Some(&community_id),
        ResourceType::Token,
        50,
        &ledger
    );
    
    // Check user1's token balances (remember: lower usage means more tokens)
    // The starting balance is 0, so after minting 100, the usage should be -100
    let user1_coop_balance = economics.get_usage(
        &user1,
        Some(&coop_id),
        None,
        ResourceType::Token,
        &ledger
    ).await;
    assert_eq!(user1_coop_balance, 0, "User1's coop balance should show 0 usage (has 100 tokens)");
    
    let user1_community_balance = economics.get_usage(
        &user1,
        Some(&coop_id),
        Some(&community_id),
        ResourceType::Token,
        &ledger
    ).await;
    assert_eq!(user1_community_balance, 0, "User1's community balance should show 0 usage (has 50 tokens)");
    
    // Transfer 30 tokens from user1 to user2 in coop scope
    economics.transfer(
        &user1,
        Some(&coop_id),
        None,
        &user2,
        Some(&coop_id),
        None,
        ResourceType::Token,
        30,
        &ledger
    );
    
    // Check balances after transfer
    let user1_coop_balance = economics.get_usage(
        &user1,
        Some(&coop_id),
        None,
        ResourceType::Token,
        &ledger
    ).await;
    assert_eq!(user1_coop_balance, 30, "User1's coop balance should show 30 usage (has 70 tokens)");
    
    let user2_coop_balance = economics.get_usage(
        &user2,
        Some(&coop_id),
        None,
        ResourceType::Token,
        &ledger
    ).await;
    assert_eq!(user2_coop_balance, 0, "User2's coop balance should show 0 usage (has 30 tokens)");
    
    // Transfer 20 tokens from user1 to user2 in community scope
    economics.transfer(
        &user1,
        Some(&coop_id),
        Some(&community_id),
        &user2,
        Some(&coop_id),
        Some(&community_id),
        ResourceType::Token,
        20,
        &ledger
    );
    
    // Check community balances after transfer
    let user1_community_balance = economics.get_usage(
        &user1,
        Some(&coop_id),
        Some(&community_id),
        ResourceType::Token,
        &ledger
    ).await;
    assert_eq!(user1_community_balance, 20, "User1's community balance should show 20 usage (has 30 tokens)");
    
    let user2_community_balance = economics.get_usage(
        &user2,
        Some(&coop_id),
        Some(&community_id),
        ResourceType::Token,
        &ledger
    ).await;
    assert_eq!(user2_community_balance, 0, "User2's community balance should show 0 usage (has 20 tokens)");
    
    // Verify organization-wide token distribution
    let coop_token_usage = economics.get_cooperative_usage(
        &coop_id,
        ResourceType::Token,
        &ledger
    ).await;
    assert_eq!(coop_token_usage, 50, "Total cooperative token usage should be 50");
    
    let community_token_usage = economics.get_community_usage(
        &community_id,
        ResourceType::Token,
        &ledger
    ).await;
    assert_eq!(community_token_usage, 20, "Total community token usage should be 20");
} 