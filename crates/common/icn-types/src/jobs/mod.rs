pub mod policy;

/// Represents an amount of ICN tokens, typically for bids, costs, or stakes.
pub type TokenAmount = u64;

use crate::error::JobFailureReason;
use icn_identity::Did;
use serde::{Deserialize, Serialize};

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
            JobStatus::Running { runner } => ("Running".to_string(), Some(runner.to_string()), None, None, None), // Assuming runner is Did
            JobStatus::Completed => ("Completed".to_string(), None, None, None, None), // Adjust if it has fields like result_cid
            JobStatus::Failed { reason } => ("Failed".to_string(), None, None, None, Some(format!("{:?}", reason))),
            JobStatus::Cancelled => ("Cancelled".to_string(), None, None, None, None),
            JobStatus::BiddingExpired => ("BiddingExpired".to_string(), None, None, None, Some("Bidding expired without assignment".to_string())),
            // Handle other variants as needed, providing appropriate string representations and None for unused fields.
        }
    }

    pub fn from_db_fields(
        status_type: &str,
        bidder_did_str: Option<&str>,
        _node_id_str: Option<&str>, // Assuming node_id is not directly part of JobStatus variants for now
        _result_cid_str: Option<&str>, // Assuming result_cid is not directly part of JobStatus variants for now
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
            "Completed" => Ok(JobStatus::Completed), // Adjust if it expects fields like result_cid
            "Failed" => {
                let reason = JobFailureReason::Unknown(error_message_str.unwrap_or("Unknown error from DB").to_string());
                Ok(JobStatus::Failed { reason })
            },
            "Cancelled" => Ok(JobStatus::Cancelled),
            "BiddingExpired" => Ok(JobStatus::BiddingExpired),
            _ => Err(format!("Unknown job status type from DB: {}", status_type)),
        }
    }
}
