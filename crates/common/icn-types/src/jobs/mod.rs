pub mod policy;

/// Represents an amount of ICN tokens, typically for bids, costs, or stakes.
pub type TokenAmount = u64;

use crate::error::JobFailureReason;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use serde_json;
use tracing;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    Pending,
    Bidding,
    Assigned { bidder_did: Did },
    Running { runner: Did },
    Completed,
    Failed { reason: JobFailureReason },
    Cancelled,
    BiddingExpired,
}

impl JobStatus {
    // Returns: (status_type_str, bidder_did_str, node_id_str, result_cid_str, error_message_str)
    pub fn to_db_fields(&self) -> (String, Option<String>, Option<String>, Option<String>, Option<String>) {
        match self {
            JobStatus::Pending => ("Pending".to_string(), None, None, None, None),
            JobStatus::Bidding => ("Bidding".to_string(), None, None, None, None),
            JobStatus::Assigned { bidder_did } => ("Assigned".to_string(), Some(bidder_did.to_string()), None, None, None),
            JobStatus::Running { runner } => ("Running".to_string(), Some(runner.to_string()), None, None, None),
            JobStatus::Completed => ("Completed".to_string(), None, None, None, None),
            JobStatus::Failed { reason } => (
                "Failed".to_string(), 
                None, 
                None, 
                None, 
                serde_json::to_string(reason).ok()
            ),
            JobStatus::Cancelled => ("Cancelled".to_string(), None, None, None, None),
            JobStatus::BiddingExpired => ("BiddingExpired".to_string(), None, None, None, Some("Bidding expired without assignment".to_string())),
        }
    }

    pub fn from_db_fields(
        status_type: &str,
        bidder_did_str: Option<&str>,
        _node_id_str: Option<&str>,
        _result_cid_str: Option<&str>,
        error_message_str: Option<&str>,
    ) -> Result<Self, String> {
        match status_type {
            "Pending" => Ok(JobStatus::Pending),
            "Bidding" => Ok(JobStatus::Bidding),
            "Assigned" => {
                let did = bidder_did_str.ok_or_else(|| "Missing bidder_did for Assigned status".to_string())?
                                    .parse().map_err(|e| format!("Invalid DID for Assigned status: {} ({})", bidder_did_str.unwrap_or("None"), e))?;
                Ok(JobStatus::Assigned { bidder_did: did })
            },
            "Running" => {
                let did = bidder_did_str.ok_or_else(|| "Missing runner_did for Running status".to_string())?
                                    .parse().map_err(|e| format!("Invalid DID for Running status: {} ({})", bidder_did_str.unwrap_or("None"), e))?;
                Ok(JobStatus::Running { runner: did })
            },
            "Completed" => Ok(JobStatus::Completed),
            "Failed" => {
                let reason = if let Some(msg_str) = error_message_str {
                    serde_json::from_str(msg_str).unwrap_or_else(|e| {
                        tracing::warn!("Failed to parse JobFailureReason from DB string '{}': {}. Falling back to Unknown.", msg_str, e);
                        JobFailureReason::Unknown(format!("Unparsable failure reason from DB: {}", msg_str))
                    })
                } else {
                    JobFailureReason::Unknown("No failure reason provided".to_string())
                };
                Ok(JobStatus::Failed { reason })
            },
            "Cancelled" => Ok(JobStatus::Cancelled),
            "BiddingExpired" => Ok(JobStatus::BiddingExpired),
            _ => Err(format!("Unknown job status type from DB: {}", status_type)),
        }
    }
}
