use icn_identity::KeyPair;
use icn_economics::{Economics, ResourceAuthorizationPolicy, ResourceType, LedgerKey};
use std::collections::HashMap;
use tokio::sync::RwLock;

#[test]
fn authorize_ok_and_record() {
    let econ = Economics::new(ResourceAuthorizationPolicy::default());
    let did  = KeyPair::generate().did;
    let ledger = RwLock::new(HashMap::new());

    assert_eq!(econ.authorize(&did, ResourceType::Token, 10), 0);
    assert_eq!(econ.record(&did, ResourceType::Token, 10, &ledger), 0);

    let l = ledger.blocking_read();
    let key = LedgerKey {
        did: did.to_string(),
        resource_type: ResourceType::Token,
    };
    assert_eq!(*l.get(&key).unwrap(), 10);
}

#[test]
fn authorize_fail() {
    let econ = Economics::new(ResourceAuthorizationPolicy { token_allowance: 5, ..Default::default() });
    let did  = KeyPair::generate().did;
    assert_eq!(econ.authorize(&did, ResourceType::Token, 10), -1);
}

#[test]
fn test_transfer_success() {
    let econ = Economics::new(ResourceAuthorizationPolicy::default());
    let sender = KeyPair::generate().did;
    let recipient = KeyPair::generate().did;
    let ledger = RwLock::new(HashMap::new());
    
    // Set up initial balances (lower usage = more tokens)
    // Give the sender 100 tokens (usage of 0)
    {
        let mut l = ledger.blocking_write();
        let sender_key = LedgerKey {
            did: sender.to_string(),
            resource_type: ResourceType::Token,
        };
        l.insert(sender_key, 0); // 0 usage = full balance
    }
    
    // Transfer 40 tokens from sender to recipient
    assert_eq!(econ.transfer(&sender, &recipient, ResourceType::Token, 40, &ledger), 0);
    
    // Verify balances after transfer
    {
        let l = ledger.blocking_read();
        
        // Sender should now have 60 tokens (40 usage)
        let sender_key = LedgerKey {
            did: sender.to_string(),
            resource_type: ResourceType::Token,
        };
        assert_eq!(*l.get(&sender_key).unwrap_or(&0), 40);
        
        // Recipient should now have 40 tokens (0 usage)
        let recipient_key = LedgerKey {
            did: recipient.to_string(),
            resource_type: ResourceType::Token,
        };
        let recipient_usage = *l.get(&recipient_key).unwrap_or(&100); // Default to high usage if not found
        assert_eq!(recipient_usage, 0); // 0 usage = 40 tokens received
    }
}

#[test]
fn test_transfer_insufficient_funds() {
    let econ = Economics::new(ResourceAuthorizationPolicy::default());
    let sender = KeyPair::generate().did;
    let recipient = KeyPair::generate().did;
    let ledger = RwLock::new(HashMap::new());
    
    // Set up initial balances
    // Give the sender 20 tokens (80 usage)
    {
        let mut l = ledger.blocking_write();
        let sender_key = LedgerKey {
            did: sender.to_string(),
            resource_type: ResourceType::Token,
        };
        l.insert(sender_key, 80); // 80 usage = 20 tokens
    }
    
    // Try to transfer 40 tokens from sender to recipient (should fail)
    assert_eq!(econ.transfer(&sender, &recipient, ResourceType::Token, 40, &ledger), -1);
    
    // Verify balances unchanged
    {
        let l = ledger.blocking_read();
        
        // Sender should still have 20 tokens (80 usage)
        let sender_key = LedgerKey {
            did: sender.to_string(),
            resource_type: ResourceType::Token,
        };
        assert_eq!(*l.get(&sender_key).unwrap_or(&0), 80);
        
        // Recipient should have 0 tokens (default high usage)
        let recipient_key = LedgerKey {
            did: recipient.to_string(),
            resource_type: ResourceType::Token,
        };
        assert_eq!(l.get(&recipient_key), None); // No entry = no tokens received
    }
} 