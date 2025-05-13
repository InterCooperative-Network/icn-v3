// This is a new file: crates/services/icn-mesh-jobs/src/job_assignment_tests.rs

#[cfg(test)]
mod reputation_selector_mana_tests {
    use super::super::*; // Imports items from job_assignment.rs
    use crate::models::BidEvaluatorConfig;
    use crate::reputation_client::ReputationClient; // The trait
    use icn_identity::Did;
    use icn_types::reputation::ReputationProfile as ICNReputationProfile;
    use icn_types::mana::{ManaState, ScopedMana};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use anyhow::Result;
    use async_trait::async_trait;
    use cid::Cid;

    // Mock for metrics
    static MOCK_BIDS_DISQUALIFIED_MANA_COUNT: AtomicUsize = AtomicUsize::new(0);
    mod mock_metrics {
        use super::MOCK_BIDS_DISQUALIFIED_MANA_COUNT;
        use std::sync::atomic::Ordering;
        pub fn increment_bids_disqualified_insufficient_mana() {
            MOCK_BIDS_DISQUALIFIED_MANA_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        pub fn get_disqualified_count() -> usize {
            MOCK_BIDS_DISQUALIFIED_MANA_COUNT.load(Ordering::SeqCst)
        }
        pub fn reset_disqualified_count() {
            MOCK_BIDS_DISQUALIFIED_MANA_COUNT.store(0, Ordering::SeqCst);
        }
    }

    // Mock Reputation Client
    #[derive(Clone)]
    struct MockReputationClient {
        profile_to_return: Option<ICNReputationProfile>,
        should_fail: bool,
    }

    #[async_trait]
    impl ReputationClient for MockReputationClient {
        async fn fetch_profile(&self, _did: &Did) -> Result<Option<ICNReputationProfile>> {
            if self.should_fail {
                Err(anyhow::anyhow!("Mock fetch_profile error"))
            } else {
                Ok(self.profile_to_return.clone())
            }
        }

        fn calculate_bid_score(
            &self,
            _config: &BidEvaluatorConfig,
            profile: &ICNReputationProfile,
            _normalized_price: f64,
            _resource_match: f64,
        ) -> f64 {
            // Simple scoring for testing: just use the reputation score directly
            // or a default if mana causes disqualification (though select handles that before this)
            profile.computed_score / 100.0 // Assuming score is 0-100, normalize to 0-1
        }

        async fn submit_record(&self, _record: icn_types::reputation::ReputationRecord) -> Result<()> {
            Ok(())
        }
    }

    // Helper to create a default JobRequest
    fn create_job_request(required_mana: Option<u64>) -> JobRequest {
        JobRequest {
            id: "job1".to_string(),
            owner_did: Did::parse("did:key:z6MkpTHR8VrstDBJmg3hOBaFzIYnPSfXiKjA7z32xN2gpfwU").unwrap(),
            cid: Cid::default(),
            requirements: JobRequirements {
                cpu_cores: 1,
                memory_mb: 1024,
                storage_gb: 10,
                max_price: 100,
                required_mana,
            },
        }
    }

    // Helper to create a Bid
    fn create_bid(bidder_did_str: &str, price: u64) -> Bid {
        Bid {
            job_id: "job1".to_string(),
            bidder_did: Did::parse(bidder_did_str).unwrap(),
            price,
            resources: JobRequirements {
                cpu_cores: 1,
                memory_mb: 1024,
                storage_gb: 10,
                max_price: 100, // Not directly used by selector, but part of struct
                required_mana: None, // Bid itself doesn't specify required mana
            },
        }
    }
    
    // Helper to create ReputationProfile
    fn create_reputation_profile(did_str: &str, mana: Option<(u64, u64, f64, u64)>, computed_score: f64) -> ICNReputationProfile {
        ICNReputationProfile {
            node_id: Did::parse(did_str).unwrap(),
            mana_state: mana.map(|(current, max, regen, last_updated)| ScopedMana {
                executor_did: Did::parse(did_str).unwrap(),
                cooperative_did: None,
                state: ManaState {
                    current_mana: current,
                    max_mana: max,
                    regen_rate_per_epoch: regen,
                    last_updated_epoch: last_updated,
                }
            }),
            last_updated: chrono::Utc::now(),
            total_jobs: 5, successful_jobs: 5, failed_jobs: 0, jobs_on_time: 5, jobs_late: 0,
            average_execution_ms: Some(100), average_bid_accuracy: Some(0.95),
            dishonesty_events: 0, endorsements: vec![], current_stake: None,
            computed_score, // e.g., 75.0 for a decent score
            latest_anchor_cid: None,
        }
    }

    fn default_config() -> BidEvaluatorConfig {
        BidEvaluatorConfig {
            weight_price: 0.3,
            weight_reputation: 0.7,
            weight_resources: 0.0, // Simplified for these tests
            weight_timeliness: 0.0, // Simplified for these tests
        }
    }

    #[tokio::test]
    async fn test_1_no_mana_requirement_no_executor_mana() {
        mock_metrics::reset_disqualified_count();
        let job_request = create_job_request(None);
        let bid1 = create_bid("did:key:z6MkjmmyvM3L4kEUDwT3LcdeXjrSKz2nCMs55Fia72aPjFqH", 50);
        
        // Executor has no specific mana state in their profile, but it's not required
        let profile1 = create_reputation_profile("did:key:z6MkjmmyvM3L4kEUDwT3LcdeXjrSKz2nCMs55Fia72aPjFqH", None, 75.0);

        let mock_client = Arc::new(MockReputationClient { profile_to_return: Some(profile1), should_fail: false });
        let selector = ReputationExecutorSelector {
            config: default_config(),
            reputation_client: mock_client,
        };

        let bids = vec![bid1.clone()];
        let result = selector.select(&job_request, &bids, Cid::default()).await.unwrap();

        assert!(result.is_some(), "Bid should be accepted as mana is not required");
        assert_eq!(result.as_ref().unwrap().0.bidder_did, bid1.bidder_did);
        assert_eq!(mock_metrics::get_disqualified_count(), 0);
    }

    #[tokio::test]
    async fn test_2_mana_required_executor_has_sufficient() {
        mock_metrics::reset_disqualified_count();
        let job_request = create_job_request(Some(100));
        let bid1 = create_bid("did:key:z6MkjmmyvM3L4kEUDwT3LcdeXjrSKz2nCMs55Fia72aPjFqH", 50);

        // Executor has 200 mana, needs 100
        let profile1 = create_reputation_profile("did:key:z6MkjmmyvM3L4kEUDwT3LcdeXjrSKz2nCMs55Fia72aPjFqH", Some((200, 1000, 10.0, 0)), 80.0);
        let mock_client = Arc::new(MockReputationClient { profile_to_return: Some(profile1), should_fail: false });
        let selector = ReputationExecutorSelector {
            config: default_config(),
            reputation_client: mock_client,
        };

        let bids = vec![bid1.clone()];
        let result = selector.select(&job_request, &bids, Cid::default()).await.unwrap();

        assert!(result.is_some(), "Bid should be accepted with sufficient mana");
        assert_eq!(result.as_ref().unwrap().0.bidder_did, bid1.bidder_did);
        assert_eq!(mock_metrics::get_disqualified_count(), 0);
    }

    #[tokio::test]
    async fn test_3_mana_required_executor_insufficient_mana() {
        mock_metrics::reset_disqualified_count();

        let job_request = create_job_request(Some(200));
        let bid1 = create_bid("did:key:z6MkjmmyvM3L4kEUDwT3LcdeXjrSKz2nCMs55Fia72aPjFqH", 50);

        let profile1 = create_reputation_profile("did:key:z6MkjmmyvM3L4kEUDwT3LcdeXjrSKz2nCMs55Fia72aPjFqH", Some((100, 1000, 10.0, 0)), 70.0);
        let mock_client = Arc::new(MockReputationClient { profile_to_return: Some(profile1), should_fail: false });
        let selector = ReputationExecutorSelector {
            config: default_config(),
            reputation_client: mock_client,
        };

        let bids = vec![bid1.clone()];
        let result = selector.select(&job_request, &bids, Cid::default()).await.unwrap();

        assert!(result.is_none(), "Bid should be disqualified due to insufficient mana");
        assert_eq!(mock_metrics::get_disqualified_count(), 1, "Metric for insufficient mana should be incremented");
    }

    #[tokio::test]
    async fn test_4_mana_required_executor_no_profile() {
        mock_metrics::reset_disqualified_count();
        let job_request = create_job_request(Some(100));
        let bid1 = create_bid("did:key:z6MkjmmyvM3L4kEUDwT3LcdeXjrSKz2nCMs55Fia72aPjFqH", 50);

        let mock_client = Arc::new(MockReputationClient { profile_to_return: None, should_fail: false });
        let selector = ReputationExecutorSelector {
            config: default_config(),
            reputation_client: mock_client,
        };

        let bids = vec![bid1.clone()];
        let result = selector.select(&job_request, &bids, Cid::default()).await.unwrap();

        assert!(result.is_none(), "Bid should be disqualified if mana is required and profile is missing");
        assert_eq!(mock_metrics::get_disqualified_count(), 1, "Metric for no profile (leading to no mana state) should be incremented");
    }

    #[tokio::test]
    async fn test_5_mana_required_profile_with_no_mana_state() {
        mock_metrics::reset_disqualified_count();
        let job_request = create_job_request(Some(100));
        let bid1 = create_bid("did:key:z6MkjmmyvM3L4kEUDwT3LcdeXjrSKz2nCMs55Fia72aPjFqH", 50);

        let profile1 = create_reputation_profile("did:key:z6MkjmmyvM3L4kEUDwT3LcdeXjrSKz2nCMs55Fia72aPjFqH", None, 78.0);
        let mock_client = Arc::new(MockReputationClient { profile_to_return: Some(profile1), should_fail: false });
        let selector = ReputationExecutorSelector {
            config: default_config(),
            reputation_client: mock_client,
        };

        let bids = vec![bid1.clone()];
        let result = selector.select(&job_request, &bids, Cid::default()).await.unwrap();

        assert!(result.is_none(), "Bid should be disqualified if mana is required and profile has no mana_state");
        assert_eq!(mock_metrics::get_disqualified_count(), 1, "Metric for no mana_state should be incremented");
    }
} 