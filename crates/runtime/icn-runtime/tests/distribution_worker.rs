use icn_identity::ScopeKey;
use icn_economics::mana::ManaManager;
use icn_types::dag::{DagEventType, DagNodeBuilder};
use icn_types::dag_store::SharedDagStore;
use icn_types::dag_store::DagStore;
use icn_runtime::distribution_worker::DistributionWorker; // path depending, adjust if module is public
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_distribution_tick() {
    // Setup node scope and manager
    let node_did = "did:icn:node1".to_string();
    let node_scope = ScopeKey::Individual(node_did.clone());

    let mana_mgr = Arc::new(Mutex::new(ManaManager::new()));
    {
        let mut mgr = mana_mgr.lock().unwrap();
        mgr.ensure_pool(&node_scope, 1_000, 1);
    }

    // Prepare DAG store and insert two receipt nodes for two originators
    let dag_store = SharedDagStore::new();

    let now_ms = chrono::Utc::now().timestamp_millis() as u64;

    let origins = vec!["did:icn:origin1", "did:icn:origin2"];
    for origin in &origins {
        let scope_id = format!("receipt/{}/{}", node_did, origin);
        let node = DagNodeBuilder::new()
            .content("test".into())
            .event_type(DagEventType::Receipt)
            .timestamp(now_ms)
            .scope_id(scope_id)
            .build()
            .unwrap();
        dag_store.insert(node).await.unwrap();
    }

    // Create worker with 60s interval but call tick directly
    let worker = DistributionWorker::new(node_scope.clone(), dag_store.clone(), mana_mgr.clone(), 60);

    let transfers = worker.tick().await;
    assert_eq!(transfers, 2);

    // Check balances
    let mut mgr = mana_mgr.lock().unwrap();
    let node_balance = mgr.balance(&node_scope).unwrap();
    assert_eq!(node_balance, 900); // 10% of 1000 distributed

    for origin in origins {
        let origin_scope = ScopeKey::Individual(origin.to_string());
        let bal = mgr.balance(&origin_scope).unwrap_or(0);
        assert_eq!(bal, 50);
    }
} 