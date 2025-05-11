use chrono::{DateTime, Utc};
use cid::Cid;
use icn_identity::Did;
use serde::{Deserialize, Serialize};

// Assuming TokenAmount is accessible. If it's defined in crate::jobs, this import is appropriate.
// If TokenAmount becomes a more globally used type, it might move to a more central location.
use crate::jobs::TokenAmount;
use crate::crypto::Signature; // Import the Signature struct

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReputationRecord {
    pub timestamp: DateTime<Utc>,
    pub issuer: Did,                          // Who emitted the event (e.g., executor, federation, job submitter)
    pub subject: Did,                         // Node whose profile is affected
    pub event: ReputationUpdateEvent,         // The actual update (success, penalty, etc.)
    pub anchor: Option<Cid>,                  // Link to supporting receipt or proof
    pub signature: Option<Signature>,         // If signed, validates issuer identity
}

impl ReputationProfile {
    /// Apply a single reputation event to this profile,
    /// updating its metrics. Does *not* recompute `computed_score`—
    /// call `.recompute_score()` afterward if you have one.
    pub fn apply_event(&mut self, event: &ReputationUpdateEvent) {
        use ReputationUpdateEvent::*; // Make enum variants directly accessible

        // Always bump total_jobs when a job event occurs
        match event {
            JobCompletedSuccessfully { execution_duration_ms, bid_accuracy, on_time, .. } => {
                self.successful_jobs = self.successful_jobs.saturating_add(1);
                if *on_time {
                    self.jobs_on_time = self.jobs_on_time.saturating_add(1);
                } else {
                    self.jobs_late = self.jobs_late.saturating_add(1);
                }
                self.average_execution_ms = Some(
                    average_u32(self.average_execution_ms, *execution_duration_ms, self.successful_jobs),
                );
                self.average_bid_accuracy = Some(
                    average_f32(self.average_bid_accuracy, *bid_accuracy, self.successful_jobs),
                );
                self.total_jobs = self.total_jobs.saturating_add(1);
            }

            JobFailed { .. } => {
                self.failed_jobs = self.failed_jobs.saturating_add(1);
                self.total_jobs = self.total_jobs.saturating_add(1);
            }

            DishonestyPenalty { score_impact, .. } => { // Added score_impact from event to be used
                self.dishonesty_events = self.dishonesty_events.saturating_add(1);
                // If score_impact is intended to directly modify computed_score, it would happen here or in recompute_score
                // For now, just tracking the event count. The direct impact could be part of recompute_score logic.
            }

            StakeIncreased { new_total_stake, .. } => { // Changed to use new_total_stake for consistency
                self.current_stake = Some(*new_total_stake);
            }

            StakeDecreased { new_total_stake, .. } => { // Changed to use new_total_stake for consistency
                self.current_stake = Some(*new_total_stake);
            }

            EndorsementReceived { from, weight, .. } => { // Added weight from event
                if !self.endorsements.contains(from) {
                    self.endorsements.push(from.clone());
                }
                // The weight could be stored alongside the DID or used in recompute_score
            }

            EndorsementRevoked { from, .. } => {
                self.endorsements.retain(|d| d != from);
            }
            
            ProfileScoreManuallyAdjusted { new_score, .. } => {
                // This event directly sets the score, bypassing normal recomputation logic for this update.
                self.computed_score = *new_score;
            }
        }

        // Update the timestamp
        self.last_updated = chrono::Utc::now();
    }
}

/// Utility to incrementally update a u32 average:
fn average_u32(prev: Option<u32>, new_val: u32, count: u64) -> u32 {
    if count == 0 { // Should not happen if called after incrementing count, but good guard
        return new_val;
    }
    match prev {
        Some(old_avg) => (((old_avg as u64).saturating_mul(count.saturating_sub(1))).saturating_add(new_val as u64) / count) as u32,
        None => new_val, // If no previous average, the new value is the average (assuming count is 1 here)
    }
}

/// Utility to incrementally update a f32 average:
fn average_f32(prev: Option<f32>, new_val: f32, count: u64) -> f32 {
    if count == 0 { // Guard clause
        return new_val;
    }
    match prev {
        Some(old_avg) => (old_avg * ((count.saturating_sub(1)) as f32) + new_val) / (count as f32),
        None => new_val, // If no previous average, the new value is the average (assuming count is 1 here)
    }
}

pub fn compute_score(profile: &ReputationProfile) -> f64 {
    const BASE_SCORE: f64 = 0.5;

    const SUCCESS_WEIGHT: f64 = 2.0;
    const TIMELINESS_WEIGHT: f64 = 1.0;
    const ACCURACY_WEIGHT: f64 = 1.5;
    const STAKE_WEIGHT: f64 = 0.3;
    const PENALTY_WEIGHT: f64 = 0.7;

    // Ensure total is at least 1.0 to avoid division by zero for rates if total_jobs is 0.
    let total = (profile.total_jobs as f64).max(1.0);

    let success_rate = profile.successful_jobs as f64 / total;
    
    // For on_time_rate, it should be based on successful_jobs or total_jobs depending on definition.
    // If it's % of successful jobs that were on time:
    let successful_jobs_total = (profile.successful_jobs as f64).max(1.0); // Avoid div by zero if no successful jobs
    let on_time_rate = profile.jobs_on_time as f64 / successful_jobs_total;
    // Alternatively, if it's % of all jobs that were on_time (less common interpretation):
    // let on_time_rate = profile.jobs_on_time as f64 / total;

    let avg_accuracy = profile.average_bid_accuracy.unwrap_or(0.5); // Default to neutral if no data
    let dishonesty_events_count = profile.dishonesty_events as f64;

    let stake_log = profile
        .current_stake
        .map(|s| (s as f64 + 1.0).ln()) // log(1 + s) is ln_1p, or (s+1.0).ln()
        .unwrap_or(0.0);

    let raw_score = BASE_SCORE
        + SUCCESS_WEIGHT * success_rate
        + TIMELINESS_WEIGHT * on_time_rate
        + ACCURACY_WEIGHT * avg_accuracy
        + STAKE_WEIGHT * stake_log
        - PENALTY_WEIGHT * dishonesty_events_count;

    // Clamp score to a defined range, e.g., 0.0 to 10.0
    raw_score.clamp(0.0, 10.0)
}

// TODO:
// - Consider how f32/f64 fields impact needs for Eq/Hash if ReputationProfile or Event instances were used as HashMap keys.
// - Implement actual reputation scoring algorithms (`compute_score` function) and integrate with `apply_event`.
// - Design DAG anchoring for profiles and events (possibly via ReputationRecord).
// - Ensure Signature type is robust and integrated with actual crypto libraries. 