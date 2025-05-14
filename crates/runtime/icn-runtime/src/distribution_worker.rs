//! Periodic mana distribution worker.
#![forbid(unsafe_code)]

use icn_economics::mana::ManaManager;
use icn_identity::Did;
use icn_identity::IdentityIndex;
use icn_identity::ScopeKey;
use icn_types::dag::{DagEventType, DagNode};
use icn_types::dag_store::{DagStore, SharedDagStore};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::time::{self, Duration, Interval};

/// Attempt to extract the originator DID from a `DagNode`.
/// For now we fall back to taking the final path segment of `scope_id`.
fn extract_originator_did(node: &DagNode) -> String {
    node.scope_id
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Helper: query all `DagNode`s of a given type since `since_ms` (naive in-memory filter).
async fn query_events(
    store: &SharedDagStore,
    event_type: DagEventType,
    since_ms: u64,
) -> Vec<DagNode> {
    let all = store.list().await.unwrap_or_default();
    all.into_iter()
        .filter(|node| node.event_type == event_type && node.timestamp >= since_ms)
        .collect()
}

/// Periodic worker that redistributes a fixed percentage of the node's mana pool
/// to the originators it served in the previous interval (as evidenced by
/// `DagEventType::Receipt` nodes it anchored).
pub struct DistributionWorker {
    node_scope: ScopeKey,
    fraction_percent: u64,
    dag_store: SharedDagStore,
    mana_mgr: Arc<Mutex<ManaManager>>,
    identity_index: Option<Arc<IdentityIndex>>,
    interval: Interval,
}

impl DistributionWorker {
    /// Create a new worker; runs every `interval_secs` seconds.
    pub fn new(
        node_scope: ScopeKey,
        dag_store: SharedDagStore,
        mana_mgr: Arc<Mutex<ManaManager>>,
        identity_index: Option<Arc<IdentityIndex>>,
        interval_secs: u64,
    ) -> Self {
        Self {
            node_scope,
            fraction_percent: 10,
            dag_store,
            mana_mgr,
            identity_index,
            interval: time::interval(Duration::from_secs(interval_secs)),
        }
    }

    /// Perform one distribution tick; returns number of successful transfers.
    pub async fn tick(&self) -> usize {
        // 1. Read node pool balance
        let mut mgr = self.mana_mgr.lock().unwrap();
        let node_balance = mgr.balance(&self.node_scope).unwrap_or(0);
        if node_balance == 0 {
            return 0;
        }

        // 2. Compute payout pool (e.g. 10 %)
        let payout = node_balance * self.fraction_percent / 100;
        if payout == 0 {
            return 0;
        }

        // 3. Query receipts anchored by this node in the last interval
        let since = chrono::Utc::now().timestamp_millis() as u64
            - self.interval.period().as_millis() as u64;
        let receipts = query_events(&self.dag_store, DagEventType::Receipt, since).await;

        let total_receipts = receipts.len() as u64;
        if total_receipts == 0 {
            return 0;
        }

        let share_per = payout / total_receipts;
        let mut count = 0;

        for node in receipts {
            let did = extract_originator_did(&node);
            let origin_scope = if let Some(index) = &self.identity_index {
                if let Ok(did_parsed) = Did::from_str(&did) {
                    index.resolve_scope_key(&did_parsed)
                } else {
                    ScopeKey::Individual(did.clone())
                }
            } else {
                ScopeKey::Individual(did)
            };

            if mgr
                .transfer(&self.node_scope, &origin_scope, share_per)
                .is_ok()
            {
                count += 1;
            }
        }
        count
    }

    /// Run the distribution loop forever.
    pub async fn run(mut self) {
        loop {
            self.interval.tick().await;
            let _ = self.tick().await;
        }
    }
}
