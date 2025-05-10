use icn_identity::KeyPair;
use icn_economics::{Economics, ResourceAuthorizationPolicy, ResourceType};
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
    assert_eq!(*l.get(&ResourceType::Token).unwrap(), 10);
}

#[test]
fn authorize_fail() {
    let econ = Economics::new(ResourceAuthorizationPolicy { token_allowance: 5, ..Default::default() });
    let did  = KeyPair::generate().did;
    assert_eq!(econ.authorize(&did, ResourceType::Token, 10), -1);
} 