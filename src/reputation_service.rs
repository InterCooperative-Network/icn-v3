use crate::types::{ReputationProfile, ReputationScore};
use crate::config::Config;
use crate::error::Error;
use crate::metrics::Metrics;
use crate::logging::Logger;
use crate::utils::time::get_current_timestamp;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

impl ReputationService {
    async fn get_reputation_profile(&self, node_id: &str) -> Result<Option<ReputationProfile>, Error> {
        // Try cache first
        if let Some(profile) = self.cache.get_reputation_profile(node_id).await? {
            return Ok(Some(profile));
        }

        // If not in cache, get from store
        let profile = self.store.get_reputation_profile(node_id).await?;
        
        // Update cache if found
        if let Some(profile) = &profile {
            self.cache.update_reputation_profile(profile.clone()).await?;
        }

        Ok(profile)
    }
} 