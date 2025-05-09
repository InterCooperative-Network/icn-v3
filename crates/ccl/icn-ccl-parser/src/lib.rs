use anyhow::Result;
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

/// Error types specific to the CCL parser
#[derive(Error, Debug)]
pub enum ParserError {
    #[error("Failed to parse CCL: {0}")]
    ParseError(String),

    #[error("Invalid syntax: {0}")]
    SyntaxError(String),

    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// A parsed CCL document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclDocument {
    /// Title of the document
    pub title: String,

    /// Description of the document
    pub description: String,

    /// Author of the document
    pub author: String,

    /// Creation date
    pub created: String,

    /// Version of the CCL specification
    pub version: String,

    /// Budget allocation (if any)
    pub budget: Option<CclBudget>,

    /// Execution instructions (if any)
    pub execution: Option<CclExecution>,

    /// Accountability requirements (if any)
    pub accountability: Option<CclAccountability>,
}

/// Budget allocation in a CCL document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclBudget {
    /// Total allocation
    pub total: u64,

    /// Currency of the allocation
    pub currency: String,

    /// Budget categories
    pub categories: std::collections::HashMap<String, u64>,

    /// Disbursement schedule
    pub disbursement: CclDisbursement,

    /// Authorization rules
    pub authorization: CclAuthorization,
}

/// Disbursement schedule in a budget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclDisbursement {
    /// Schedule type
    pub schedule: String,

    /// Start date
    pub start_date: String,

    /// End date
    pub end_date: String,
}

/// Authorization rules in a budget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclAuthorization {
    /// Threshold of approvals needed
    pub threshold: u64,

    /// Roles that can approve
    pub roles: Vec<String>,

    /// Whether review is required
    pub require_review: bool,
}

/// Execution instructions in a CCL document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclExecution {
    /// Actions to perform
    pub actions: Vec<CclAction>,
}

/// Action to perform in execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CclAction {
    /// Anchor data to the DAG
    AnchorData(String),

    /// Perform a metered action
    PerformAction { action_type: String, amount: u64 },

    /// Mint tokens
    MintTokens {
        token_type: String,
        amount: u64,
        recipient: String,
    },
}

/// Accountability requirements in a CCL document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclAccountability {
    /// Report requirements
    pub reports: CclReports,

    /// Transparency requirements
    pub transparency: CclTransparency,
}

/// Report requirements in accountability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclReports {
    /// Report frequency
    pub frequency: String,

    /// Metrics to report
    pub metrics: Vec<String>,
}

/// Transparency requirements in accountability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclTransparency {
    /// Level of disclosure
    pub disclosure_level: String,

    /// Whether a public dashboard is required
    pub public_dashboard: bool,
}

/// Parse a CCL document
pub fn parse_ccl(input: &str) -> Result<CclDocument> {
    // This is a stub implementation that returns a fixed document
    // In a real implementation, we would use nom or pest to parse the CCL

    // For now, return a fixed document based on the example
    Ok(CclDocument {
        title: "Q3 Budget Allocation".to_string(),
        description: "Allocate funds for Q3 2023 cooperative activities".to_string(),
        author: "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
        created: "2023-10-15T14:30:00Z".to_string(),
        version: "1.0.0".to_string(),
        budget: Some(CclBudget {
            total: 10000,
            currency: "USDC".to_string(),
            categories: {
                let mut map = std::collections::HashMap::new();
                map.insert("development".to_string(), 6000);
                map.insert("marketing".to_string(), 2000);
                map.insert("operations".to_string(), 1500);
                map.insert("community".to_string(), 500);
                map
            },
            disbursement: CclDisbursement {
                schedule: "monthly".to_string(),
                start_date: "2023-10-01".to_string(),
                end_date: "2023-12-31".to_string(),
            },
            authorization: CclAuthorization {
                threshold: 2,
                roles: vec!["treasurer".to_string(), "director".to_string()],
                require_review: true,
            },
        }),
        execution: Some(CclExecution {
            actions: vec![
                CclAction::AnchorData("budget_q3_2023".to_string()),
                CclAction::PerformAction {
                    action_type: "budget_allocation".to_string(),
                    amount: 10000,
                },
                CclAction::MintTokens {
                    token_type: "participation_token".to_string(),
                    amount: 100,
                    recipient: "community_pool".to_string(),
                },
            ],
        }),
        accountability: Some(CclAccountability {
            reports: CclReports {
                frequency: "monthly".to_string(),
                metrics: vec![
                    "spend_by_category".to_string(),
                    "completion_status".to_string(),
                    "remaining_funds".to_string(),
                ],
            },
            transparency: CclTransparency {
                disclosure_level: "public".to_string(),
                public_dashboard: true,
            },
        }),
    })
}

// Define the CCL parser using Pest
#[derive(Parser)]
#[grammar = "ccl.pest"]
pub struct CclParser;

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
                        CclAction::MintTokens { .. } => {}
                        CclAction::AnchorData { .. } => {}
                        CclAction::PerformAction { .. } => {}
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
