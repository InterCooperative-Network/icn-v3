use crate::types::{ReputationProfile, ReputationScore};
use crate::config::Config;
use crate::error::Error;
use crate::metrics::Metrics;
use crate::logging::Logger;
use crate::utils::time::get_current_timestamp;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

impl ReputationAllocator {
    async fn get_reputation_profile(&self, node_id: &str) -> Result<Option<ReputationProfile>, Error> {
        self.balancer.get_reputation_profile(node_id).await
    }
} 