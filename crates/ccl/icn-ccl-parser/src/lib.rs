use pest::Parser;
use pest_derive::Parser;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error types for CCL parsing
#[derive(Error, Debug)]
pub enum CclError {
    #[error("Failed to parse CCL: {0}")]
    ParseError(String),
    
    #[error("Failed to convert CCL to DSL: {0}")]
    DslConversionError(String),
    
    #[error("Invalid CCL structure: {0}")]
    InvalidStructure(String),
    
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type for CCL operations
pub type Result<T> = std::result::Result<T, CclError>;

// Define the CCL parser using Pest
#[derive(Parser)]
#[grammar = "ccl.pest"]
pub struct CclParser;

/// A parsed CCL document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclDocument {
    pub statements: Vec<CclStatement>,
}

/// Represents a CCL statement
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CclStatement {
    Organization {
        name: String,
        description: Option<String>,
        version: Option<String>,
    },
    Role {
        name: String,
        description: Option<String>,
        permissions: Vec<String>,
    },
    Governance {
        quorum: f64,
        proposals: Vec<CclProposal>,
    },
    Action {
        event: String,
        actions: Vec<CclAction>,
    },
    Budget {
        name: String,
        description: Option<String>,
        currency: Option<String>,
        period: Option<String>,
    },
    Election {
        name: String,
        description: Option<String>,
        roles: Vec<CclElectionRole>,
    },
    Custom {
        name: String,
        value: serde_json::Value,
    },
}

/// Represents a CCL proposal configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclProposal {
    pub name: String,
    pub description: Option<String>,
    pub approval_threshold: f64,
    pub voting_period: String,
    pub requires_role: Option<String>,
}

/// Represents a CCL action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum CclAction {
    MintToken {
        token_type: String,
        recipient: String,
        data: serde_json::Value,
    },
    AnchorData {
        path: String,
        data: serde_json::Value,
    },
    PerformMeteredAction {
        action: String,
        args: serde_json::Value,
    },
    Conditional {
        condition: String,
        then_actions: Vec<Box<CclAction>>,
        else_actions: Option<Vec<Box<CclAction>>>,
    },
}

/// Represents a role in an election
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclElectionRole {
    pub name: String,
    pub description: Option<String>,
    pub term_length: String,
    pub term_limit: Option<u32>,
    pub seats: u32,
    pub requirements: serde_json::Value,
}

impl CclDocument {
    /// Parse a CCL string into a document
    pub fn parse(input: &str) -> Result<Self> {
        // Parse using Pest
        let parsed = CclParser::parse(Rule::ccl, input)
            .map_err(|e| CclError::ParseError(e.to_string()))?
            .next()
            .unwrap();
        
        // Convert the parsed result to a CclDocument
        // For now, just return a placeholder
        Ok(CclDocument {
            statements: Vec::new(),
        })
    }
    
    /// Convert the CCL document to a DSL representation
    pub fn to_dsl(&self) -> Result<String> {
        // Convert the document to DSL
        // For now, just return a placeholder
        Ok("// Generated DSL code\n".to_string())
    }
    
    /// Verify that the CCL document is valid
    pub fn verify(&self) -> Result<()> {
        // Check for required elements
        
        // Check for valid actions
        for statement in &self.statements {
            if let CclStatement::Action { actions, .. } = statement {
                for action in actions {
                    match action {
                        CclAction::MintToken { .. } => {},
                        CclAction::AnchorData { .. } => {},
                        CclAction::PerformMeteredAction { .. } => {},
                        CclAction::Conditional { then_actions, else_actions, .. } => {
                            // Verify nested actions
                            for nested in then_actions {
                                // Recursive verification could be added here
                            }
                            if let Some(else_blocks) = else_actions {
                                for nested in else_blocks {
                                    // Recursive verification could be added here
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
}

// Re-export the Rule type for tests
#[cfg(test)]
pub use Rule; 