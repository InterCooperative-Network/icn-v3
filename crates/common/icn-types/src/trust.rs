use crate::error::TrustError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The type of quorum rule to apply
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
#[serde(tag = "type", content = "value")]
pub enum QuorumRule {
    /// Require a majority of signers
    Majority,
    
    /// Require a specific threshold percentage (0-100)
    Threshold(u8),
    
    /// Require weighted signatures to reach a threshold
    Weighted {
        /// Weights assigned to each DID
        weights: HashMap<String, u32>,
        
        /// Required threshold to reach
        threshold: u32,
    },
}

impl Default for QuorumRule {
    fn default() -> Self {
        Self::Majority
    }
}

/// Configuration for quorum validation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct QuorumConfig {
    /// The rule to apply for quorum validation
    pub rule: QuorumRule,
    
    /// The list of authorized DIDs that can participate in the quorum
    pub authorized_dids: Vec<String>,
}

impl QuorumConfig {
    /// Create a new majority quorum config
    pub fn new_majority(authorized_dids: Vec<String>) -> Self {
        Self {
            rule: QuorumRule::Majority,
            authorized_dids,
        }
    }
    
    /// Create a new threshold quorum config
    pub fn new_threshold(authorized_dids: Vec<String>, threshold: u8) -> Result<Self, TrustError> {
        if threshold > 100 {
            return Err(TrustError::InvalidQuorumConfig("Threshold must be between 0 and 100".to_string()));
        }
        
        Ok(Self {
            rule: QuorumRule::Threshold(threshold),
            authorized_dids,
        })
    }
    
    /// Create a new weighted quorum config
    pub fn new_weighted(
        weights: HashMap<String, u32>,
        threshold: u32,
    ) -> Result<Self, TrustError> {
        // Extract the authorized DIDs from the weights
        let authorized_dids = weights.keys().cloned().collect();
        
        // Calculate the total possible weight
        let total_weight: u32 = weights.values().sum();
        
        // Ensure the threshold is achievable
        if threshold > total_weight {
            return Err(TrustError::InvalidQuorumConfig(
                "Threshold exceeds total possible weight".to_string(),
            ));
        }
        
        Ok(Self {
            rule: QuorumRule::Weighted {
                weights,
                threshold,
            },
            authorized_dids,
        })
    }
    
    /// Validate if a set of signers satisfies the quorum rule
    pub fn validate_quorum(&self, signers: &[String]) -> Result<bool, TrustError> {
        // Verify all signers are authorized
        for signer in signers {
            if !self.authorized_dids.contains(signer) {
                return Err(TrustError::UnauthorizedSigner(signer.clone()));
            }
        }
        
        // Check for duplicate signers
        let unique_signers: std::collections::HashSet<&String> = signers.iter().collect();
        if unique_signers.len() != signers.len() {
            return Err(TrustError::DuplicateSigners);
        }
        
        match &self.rule {
            QuorumRule::Majority => {
                // More than half of authorized DIDs must sign
                let required = (self.authorized_dids.len() / 2) + 1;
                Ok(signers.len() >= required)
            }
            
            QuorumRule::Threshold(threshold) => {
                // Calculate the percentage of authorized DIDs that have signed
                let percentage = (signers.len() * 100) / self.authorized_dids.len();
                Ok(percentage >= *threshold as usize)
            }
            
            QuorumRule::Weighted { weights, threshold } => {
                // Sum the weights of all signers
                let mut total = 0;
                for signer in signers {
                    if let Some(weight) = weights.get(signer) {
                        total += weight;
                    }
                }
                
                Ok(total >= *threshold)
            }
        }
    }
}

/// A proof that a quorum of signers have approved something
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct QuorumProof {
    /// The DIDs of the signers
    pub signers: Vec<String>,
    
    /// The quorum rule that was applied
    pub rule: QuorumRule,
    
    /// Timestamp when the quorum was reached
    pub timestamp: String,
} 