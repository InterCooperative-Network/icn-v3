/// An in-memory mana ledger for testing and development.
#[derive(Default, Debug, Clone)]
pub struct InMemoryManaLedger {
    balances: Arc<RwLock<HashMap<LedgerKey, ManaState>>>,
} 