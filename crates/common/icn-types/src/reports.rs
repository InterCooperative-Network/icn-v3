use icn_identity::Did;
use crate::error::JobFailureReason; // Assumes JobFailureReason is in icn-types/src/error.rs
use serde::{Deserialize, Serialize};

/// Report from a runtime/worker indicating a job has failed.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuntimeJobFailureReport {
    pub reporting_node_did: Did,
    pub reason: JobFailureReason,
} 