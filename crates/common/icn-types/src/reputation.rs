use chrono::{DateTime, Utc};
use cid::Cid;
use icn_identity::Did;
use serde::{Deserialize, Serialize};

// Assuming TokenAmount is accessible. If it's defined in crate::jobs, this import is appropriate.
// If TokenAmount becomes a more globally used type, it might move to a more central location.
use crate::jobs::TokenAmount;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReputationProfile {
    pub node_id: Did,
    pub last_updated: DateTime<Utc>,
    pub total_jobs: u64,
    pub successful_jobs: u64,
    pub failed_jobs: u64,
    pub jobs_on_time: u64,
    pub jobs_late: u64,
    pub average_execution_ms: Option<u32>,
    pub average_bid_accuracy: Option<f32>, // f32 also doesn't have Eq/Hash, but PartialEq is fine.
    pub dishonesty_events: u32,
    pub endorsements: Vec<Did>,
    pub current_stake: Option<TokenAmount>,
    pub computed_score: f64, // f64 also doesn't have Eq/Hash, but PartialEq is fine.
    pub latest_anchor_cid: Option<Cid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReputationUpdateEvent {
    JobCompletedSuccessfully {
        job_id: Cid,
        execution_duration_ms: u32,
        bid_accuracy: f32, // 0.0–1.0
        on_time: bool,
        anchor_cid: Option<Cid>, // Optional CID of the execution receipt or result anchor
    },
    JobFailed {
        job_id: Cid,
        reason: String,
        anchor_cid: Option<Cid>, // Optional CID of any failure report or evidence
    },
    DishonestyPenalty {
        // Could be related to a specific job or general misbehavior
        job_id: Option<Cid>,
        details: String, // Description of the dishonest act
        penalty_amount: Option<TokenAmount>, // Optional direct token penalty
        score_impact: f64, // Direct impact on computed_score, if not derived otherwise
    },
    StakeIncreased {
        by_amount: TokenAmount,
        new_total_stake: TokenAmount,
    },
    StakeDecreased {
        by_amount: TokenAmount,
        new_total_stake: TokenAmount,
    },
    EndorsementReceived {
        from: Did,
        context: Option<String>, // e.g., "Completed Project X successfully"
        weight: Option<f32>,    // Optional weight of the endorsement
    },
    EndorsementRevoked {
        from: Did,
        reason: Option<String>,
    },
    ProfileScoreManuallyAdjusted { // For admin interventions or specific non-event-driven changes
        new_score: f64,
        previous_score: f64,
        reason: String,
    },
    // This specific event might be redundant if profile score is always recomputed after other events.
    // If it's a separate observable event, it stays. Otherwise, it's an outcome of other events.
    // ProfileScoreRecomputed {
    //     new_score: f64,
    //     previous_score: f64,
    // },
}

// TODO:
// - Consider how f32/f64 fields impact needs for Eq/Hash if ReputationProfile or Event instances were used as HashMap keys.
//   For now, PartialEq is sufficient.
// - Implement actual reputation scoring algorithms and the apply_update function.
// - Design DAG anchoring for profiles and events (possibly via ReputationRecord).
// - Define ReputationRecord struct (timestamp, event, issuer, signature). 