use icn_economics::mana::{ManaError, ManaManager};
use icn_identity::ScopeKey;

#[test]
fn transfer_moves_and_creates_pools() {
    let mut mgr = ManaManager::new();

    // Seed source pool with 500 credits (max 500, regen 1/s)
    let from = ScopeKey::Individual("did:icn:user1".to_string());
    mgr.ensure_pool(&from, 500, 1);

    // Destination cooperative scope (will be auto-created by transfer)
    let to = ScopeKey::Cooperative("did:icn:coopA".to_string());

    // Transfer 200 credits
    mgr.transfer(&from, &to, 200)
        .expect("transfer should succeed");

    // Source should have 300 left
    assert_eq!(mgr.balance(&from).unwrap(), 300);

    // Destination should now exist with at least 200
    assert_eq!(mgr.balance(&to).unwrap(), 200);

    // Attempt to overdraw should error
    let err = mgr
        .transfer(&from, &to, 400)
        .expect_err("expected InsufficientMana error");
    matches!(err, ManaError::InsufficientMana { .. });
}
