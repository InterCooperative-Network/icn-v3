use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use crate::reputation_client::{ReputationClient, ReputationProfile, DefaultReputationClient};
use crate::models::BidEvaluatorConfig;
use crate::metrics;

/// Cache entry for a reputation profile
struct CacheEntry {
    /// The profile data
    profile: ReputationProfile,
    /// When this entry was last updated
    last_updated: Instant,
}

/// Caching reputation client that wraps another client
pub struct CachingReputationClient {
    /// The underlying client to use when cache misses
    inner_client: Box<dyn ReputationClient>,
    /// The cache of profiles
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// TTL for cache entries in seconds
    cache_ttl: Duration,
}

impl CachingReputationClient {
    /// Create a new caching client with the specified TTL in seconds
    pub fn new(inner_client: Box<dyn ReputationClient>, cache_ttl_seconds: u64) -> Self {
        Self {
            inner_client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: Duration::from_secs(cache_ttl_seconds),
        }
    }
    
    /// Create a new caching client with default settings
    pub fn with_defaults(reputation_url: Arc<String>) -> Self {
        // Default 30 second TTL
        let inner_client = Box::new(DefaultReputationClient::new(reputation_url));
        Self::new(inner_client, 30)
    }
    
    /// Purge expired entries from the cache
    pub fn purge_expired(&self) {
        let mut cache = self.cache.write().unwrap();
        let now = Instant::now();
        
        // Remove expired entries
        cache.retain(|_, entry| now.duration_since(entry.last_updated) < self.cache_ttl);
        
        // Update cache size metric
        metrics::update_reputation_cache_size(cache.len());
    }
}

#[async_trait::async_trait]
impl ReputationClient for CachingReputationClient {
    async fn fetch_profile(&self, did: &str) -> Result<ReputationProfile> {
        // Check cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(entry) = cache.get(did) {
                let now = Instant::now();
                if now.duration_since(entry.last_updated) < self.cache_ttl {
                    tracing::debug!("Cache hit for reputation profile of {}", did);
                    metrics::record_reputation_cache_hit();
                    return Ok(entry.profile.clone());
                }
                // Entry expired, will be refreshed
                tracing::debug!("Cache entry expired for {}", did);
            }
        }
        
        // Cache miss or expired, fetch fresh data
        tracing::debug!("Cache miss for reputation profile of {}", did);
        metrics::record_reputation_cache_miss();
        metrics::record_reputation_query();
        
        let profile = self.inner_client.fetch_profile(did).await?;
        
        // Update cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(did.to_string(), CacheEntry {
                profile: profile.clone(),
                last_updated: Instant::now(),
            });
            
            // Update cache size metric
            metrics::update_reputation_cache_size(cache.len());
        }
        
        Ok(profile)
    }
    
    fn calculate_bid_score(
        &self,
        config: &BidEvaluatorConfig,
        profile: &ReputationProfile,
        normalized_price: f64,
        resource_match: f64,
    ) -> f64 {
        let score = self.inner_client.calculate_bid_score(config, profile, normalized_price, resource_match);
        metrics::record_bid_score(score);
        score
    }
}

// Implement cleanup task that runs periodically to remove expired entries
pub fn spawn_cache_cleanup_task(cache: Arc<CachingReputationClient>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            cache.purge_expired();
            tracing::debug!("Purged expired reputation cache entries");
        }
    });
} 