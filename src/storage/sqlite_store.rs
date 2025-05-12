use crate::types::{ReputationProfile, ReputationScore};
use crate::storage::types::{BidEvaluation, BidEvaluationResult, EvaluationMetrics};
use crate::config::Config;
use crate::error::Error;
use crate::metrics::Metrics;
use crate::logging::Logger;
use crate::utils::time::get_current_timestamp;
use sqlx::sqlite::{SqlitePool, SqliteConnectOptions};
use sqlx::Row;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use std::time::Duration;

impl SqliteStore {
    async fn get_reputation_profile(&self, node_id: &str) -> Result<Option<ReputationProfile>, Error> {
        let mut conn = self.pool.acquire().await?;
        
        let profile = sqlx::query!(
            r#"
            SELECT 
                node_id,
                total_score,
                reliability_score,
                performance_score,
                security_score,
                last_updated
            FROM reputation_profiles
            WHERE node_id = ?
            "#,
            node_id
        )
        .fetch_optional(&mut *conn)
        .await?;

        Ok(profile.map(|p| ReputationProfile {
            node_id: p.node_id,
            total_score: p.total_score,
            reliability_score: p.reliability_score,
            performance_score: p.performance_score,
            security_score: p.security_score,
            last_updated: p.last_updated,
        }))
    }
} 