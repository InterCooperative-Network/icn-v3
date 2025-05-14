use chrono::Utc;
use ed25519_dalek::SigningKey;
use icn_economics::mana::ManaManager;
use icn_identity::{Did, IdentityIndex, ScopeKey};
use icn_runtime::distribution_worker::DistributionWorker;
use icn_types::dag::{DagEventType, DagNodeBuilder};
use icn_types::dag_store::{DagStore, SharedDagStore};
use rand_core::OsRng;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_distribution_with_identity_index() {
    // --- Setup identity mappings ---
    let mut index = IdentityIndex::new();
    let sk = SigningKey::generate(&mut OsRng);
    let origin_did = Did::new_ed25519(&sk.verifying_key());
    let coop_id = "coopA".to_string();
    let community_id = "communityX".to_string();

    index.insert_did_coop(origin_did.clone(), coop_id.clone());
    index.insert_coop_community(coop_id.clone(), community_id.clone());

    let index_arc = Arc::new(index);

    // --- Setup mana manager and node pool ---
    let node_did = "did:icn:node1".to_string();
    let node_scope = ScopeKey::Individual(node_did.clone());

    let mana_mgr = Arc::new(Mutex::new(ManaManager::new()));
    {
        let mut mgr = mana_mgr.lock().unwrap();
        mgr.ensure_pool(&node_scope, 1_000, 1); // seed 1000 credits
    }

    // --- Create DAG with one receipt ---
    let dag_store = SharedDagStore::new();
    let now_ms = Utc::now().timestamp_millis() as u64;

    let scope_id = format!("receipt/{}/{}", node_did, origin_did.as_str());
    let node = DagNodeBuilder::new()
        .content("test".into())
        .event_type(DagEventType::Receipt)
        .timestamp(now_ms)
        .scope_id(scope_id)
        .build()
        .unwrap();
    dag_store.insert(node).await.unwrap();

    // --- Run worker tick ---
    let worker = DistributionWorker::new(
        node_scope.clone(),
        dag_store.clone(),
        mana_mgr.clone(),
        Some(index_arc.clone()),
        60,
    );

    let transfers = worker.tick().await;
    assert_eq!(transfers, 1);

    // --- Assert balances ---
    let mut mgr = mana_mgr.lock().unwrap();
    let node_balance = mgr.balance(&node_scope).unwrap();
    assert_eq!(node_balance, 900); // 10% distributed

    let community_scope = ScopeKey::Community(community_id.clone());
    let comm_bal = mgr.balance(&community_scope).unwrap_or(0);
    assert_eq!(comm_bal, 100);

    // Origin DID should not receive individual credits
    let origin_scope = ScopeKey::Individual(origin_did.as_str().to_string());
    assert!(mgr.balance(&origin_scope).unwrap_or(0) == 0);
}
