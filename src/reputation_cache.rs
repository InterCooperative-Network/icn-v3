use crate::types::{ReputationProfile, ReputationScore};
use crate::config::Config;
use crate::error::Error;
use crate::metrics::Metrics;
use crate::logging::Logger;
use crate::utils::time::get_current_timestamp;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::time::Duration;

impl ReputationCache {
    async fn get_reputation_profile(&self, node_id: &str) -> Result<Option<ReputationProfile>, Error> {
        let mut cache = self.cache.write().await;
        
        if let Some(profile) = cache.get(node_id) {
            if profile.last_updated + self.ttl > get_current_timestamp() {
                return Ok(Some(profile.clone()));
            }
        }
        
        Ok(None)
    }
} 