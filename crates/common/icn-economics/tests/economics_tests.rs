use icn_identity::KeyPair;
use icn_economics::{Economics, ResourceAuthorizationPolicy, ResourceType, LedgerKey};
use std::collections::HashMap;
use tokio::sync::RwLock;

#[test]
fn authorize_ok_and_record() {
    let econ = Economics::new(ResourceAuthorizationPolicy::default());
    let did  = KeyPair::generate().did;
    let ledger = RwLock::new(HashMap::new());

    assert_eq!(econ.authorize(&did, None, None, ResourceType::Token, 10), 0);
    assert_eq!(econ.record(&did, None, None, ResourceType::Token, 10, &ledger), 0);

    let l = ledger.blocking_read();
    let key = LedgerKey {
        did: did.to_string(),
        coop_id: None,
        community_id: None,
        resource_type: ResourceType::Token,
    };
    assert_eq!(*l.get(&key).unwrap(), 10);
}

#[test]
fn authorize_fail() {
    let econ = Economics::new(ResourceAuthorizationPolicy { token_allowance: 5, ..Default::default() });
    let did  = KeyPair::generate().did;
    assert_eq!(econ.authorize(&did, None, None, ResourceType::Token, 10), -1);
}

#[test]
fn test_transfer_success() {
    let econ = Economics::new(ResourceAuthorizationPolicy::default());
    let sender = KeyPair::generate().did;
    let recipient = KeyPair::generate().did;
    let ledger = RwLock::new(HashMap::new());
    
    // Set up initial balances:
    // Lower usage = more tokens available
    // 0 usage = full balance (e.g., 100 tokens)
    {
        let mut l = ledger.blocking_write();
        let sender_key = LedgerKey {
            did: sender.to_string(),
            coop_id: None,
            community_id: None,
            resource_type: ResourceType::Token,
        };
        // Set sender's usage to 0 (full balance)
        l.insert(sender_key, 0);
    }
    
    // Transfer 40 tokens from sender to recipient
    // This should increase sender's usage by 40 (decreasing their balance)
    // and set recipient's usage to 0 (giving them 40 tokens)
    let result = econ.transfer(
        &sender, None, None,
        &recipient, None, None,
        ResourceType::Token, 40, &ledger);
    
    assert_eq!(result, 0, "Transfer should succeed");
    
    // Verify balances after transfer
    {
        let l = ledger.blocking_read();
        
        // Sender should now have usage of 40 (60 tokens left)
        let sender_key = LedgerKey {
            did: sender.to_string(),
            coop_id: None,
            community_id: None,
            resource_type: ResourceType::Token,
        };
        assert_eq!(*l.get(&sender_key).unwrap_or(&0), 40, "Sender should have usage of 40");
        
        // Recipient should have usage of 0 (full tokens received)
        let recipient_key = LedgerKey {
            did: recipient.to_string(),
            coop_id: None,
            community_id: None,
            resource_type: ResourceType::Token,
        };
        assert_eq!(*l.get(&recipient_key).unwrap_or(&100), 0, "Recipient should have usage of 0");
    }
}

#[test]
fn test_transfer_insufficient_funds() {
    let econ = Economics::new(ResourceAuthorizationPolicy::default());
    let sender = KeyPair::generate().did;
    let recipient = KeyPair::generate().did;
    let ledger = RwLock::new(HashMap::new());
    
    // Set up initial balances
    // In our model:
    // - Usage of 0 means full balance (100 tokens)
    // - Usage of 80 means 20 tokens available
    {
        let mut l = ledger.blocking_write();
        let sender_key = LedgerKey {
            did: sender.to_string(),
            coop_id: None,
            community_id: None,
            resource_type: ResourceType::Token,
        };
        // Set sender's usage to 80 (20 tokens available)
        l.insert(sender_key, 80);
    }
    
    // Try to transfer 40 tokens from sender to recipient (should fail)
    // This would require 40 tokens, but sender only has 20
    let result = econ.transfer(
        &sender, None, None,
        &recipient, None, None,
        ResourceType::Token, 40, &ledger);
    
    assert_eq!(result, -1, "Transfer should fail due to insufficient funds");
    
    // Verify balances unchanged
    {
        let l = ledger.blocking_read();
        
        // Sender should still have 20 tokens (80 usage)
        let sender_key = LedgerKey {
            did: sender.to_string(),
            coop_id: None,
            community_id: None,
            resource_type: ResourceType::Token,
        };
        assert_eq!(*l.get(&sender_key).unwrap_or(&0), 80, "Sender should still have usage of 80");
        
        // Recipient should have no entry (no tokens received)
        let recipient_key = LedgerKey {
            did: recipient.to_string(),
            coop_id: None,
            community_id: None,
            resource_type: ResourceType::Token,
        };
        assert!(l.get(&recipient_key).is_none(), "Recipient should have no entry");
    }
} 