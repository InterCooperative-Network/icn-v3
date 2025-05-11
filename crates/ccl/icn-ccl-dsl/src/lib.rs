#![deny(missing_docs)]

//! Core, serde-friendly AST that the CCL compiler emits before WASM code-gen.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use uuid::Uuid;
use icn_economics::ResourceType;

/// Represents a generic section of the CCL that hasn't been fully modeled yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericSection {
    /// The kind of section, e.g., "organization", "process", "membership".
    pub kind: String,
    /// Optional quoted title string that might follow the keyword (e.g., organization "title").
    pub title: Option<String>,
    /// Everything inside the section's block, parsed as DSL rules.
    pub rules: Vec<Rule>,
}

/// Every top-level cooperative artefact the DSL can emit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DslModule {
    /// A proposal module.
    Proposal(Proposal),
    /// A vote module.
    Vote(Vote),
    /// An anchor module.
    Anchor(Anchor),
    /// A metered action module.
    MeteredAction(MeteredAction),
    /// A role definition module.
    Role(Role),
    /// An action handler module, defining steps for a specific event.
    ActionHandler(ActionHandler),
    /// A generic section, for definitions not yet fully modeled.
    Section(GenericSection),
}

/// Canonically-typed proposal object (post-parse, pre-codegen).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Unique ID for the proposal.
    pub id: Uuid,
    /// Title of the proposal.
    pub title: String,
    /// Version of the proposal.
    pub version: String,
    /// Body content of the proposal.
    pub body: String,
    /// Author identifier (e.g., DID).
    pub author: String,
    /// Creation timestamp (Unix epoch seconds).
    pub created_at: i64,
    /// Associated rules for the proposal.
    pub rules: Vec<Rule>,
}

/// Simple vote artefact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// ID of the proposal being voted on.
    pub proposal_id: Uuid,
    /// Voter identifier.
    pub voter: String,
    /// Stance of the vote.
    pub stance: VoteStance,
    /// Optional rationale for the vote.
    pub rationale: Option<String>,
    /// Signature timestamp (Unix epoch seconds).
    pub signed_at: i64,
}

/// Represents the stance of a vote.
#[derive(Debug, Clone, Serialize, Deserialize, Display, EnumString)]
pub enum VoteStance {
    /// Affirmative vote.
    #[strum(serialize = "yes")]
    Yes,
    /// Negative vote.
    #[strum(serialize = "no")]
    No,
    /// Abstention from voting.
    #[strum(serialize = "abstain")]
    Abstain,
}

/// Data anchoring request (will call `host_anchor_to_dag`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anchor {
    /// A reference to the data to be anchored, can be a CID or other identifier.
    pub data_reference: String,
    /// Optional path for where the data is anchored, e.g., a namespace or directory.
    pub path: Option<String>,
}

/// Execution metering (resource consumption).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeteredAction {
    /// Type of resource being consumed or minted.
    pub resource_type: String,
    /// Amount of resource, if applicable (defaults to 1 for minting if not specified).
    pub amount: u64,
    /// Optional recipient for the minted resource or action.
    pub recipient: Option<String>,
    /// Optional structured data associated with the action.
    pub data: Option<Vec<Rule>>,
}

/// Represents a role definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// The name of the role.
    pub name: String,
    /// Optional description for the role.
    pub description: Option<String>,
    /// Attributes associated with the role (e.g., term_length, seats).
    pub attributes: Vec<Rule>,
}

/// Generic on-chain rule block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Key for the rule.
    pub key: String,
    /// Value of the rule.
    pub value: RuleValue,
}

/// Represents the value of a rule, which can be of various types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuleValue {
    /// A string value.
    String(String),
    /// A floating-point number value.
    Number(f64),
    /// A boolean value.
    Boolean(bool),
    /// A list of rule values.
    List(Vec<RuleValue>),
    /// A map represented as a list of rules (key-value pairs).
    Map(Vec<Rule>),
    /// A range rule, typically for numeric thresholds.
    Range(Box<RangeRule>),
    /// An if-expression, for conditional rules.
    If(Box<IfExpr>),
}

/// Represents a rule defining a numeric range and associated sub-rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeRule {
    /// The start of the range (inclusive).
    pub start: f64,
    /// The end of the range (inclusive).
    pub end: f64,
    /// Sub-rules that apply within this range.
    pub rules: Vec<Rule>,
}

/// Represents an if-expression with a condition and corresponding rule blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfExpr {
    /// The raw condition string (e.g., "proposal.type == "bylaw_change"").
    pub condition_raw: String,
    /// Rules to be applied if the condition is true.
    pub then_rules: Vec<Rule>,
    /// Optional rules to be applied if the condition is false.
    pub else_rules: Option<Vec<Rule>>,
}

/// Represents a handler for a specific event, containing a sequence of actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionHandler {
    /// The name of the event that triggers this handler.
    pub event: String,
    /// A list of steps to execute when the event occurs.
    pub steps: Vec<ActionStep>,
}

/// Represents a single step within an ActionHandler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionStep {
    /// A metered action, typically involving resource tokens or other quantifiable operations.
    Metered(MeteredAction),
    /// An action that anchors data, usually by storing a CID on a DAG.
    Anchor(Anchor),
    /// A metered action with specific resource type and amount
    PerformMeteredAction {
        /// Action identifier
        ident: String,
        /// Resource type to use
        resource: ResourceType,
        /// Amount of resource
        amount: u64,
    },
    /// Transfer tokens from one DID to another
    TransferToken {
        /// Token type being transferred
        token_type: String,
        /// Amount of tokens to transfer
        amount: u64,
        /// Sender DID
        sender: String,
        /// Recipient DID
        recipient: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // A fixed UUID for reproducible tests
    const TEST_UUID: &str = "a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8";

    #[test]
    fn roundtrip_proposal() {
        let p = Proposal {
            id: Uuid::parse_str(TEST_UUID).unwrap(), // Use fixed UUID
            title: "Test".into(),
            version: "1.0".into(),
            body: "hello".into(),
            author: "did:key:z6M...".into(),
            created_at: 0,
            rules: vec![],
        };
        let json = serde_json::to_string_pretty(&p).unwrap();
        let back: Proposal = serde_json::from_str(&json).unwrap();
        insta::assert_snapshot!(json);
        assert_eq!(p.title, back.title);
        // Add more assertions if needed, e.g., for id, body, etc.
        assert_eq!(p.id, back.id);
        assert_eq!(p.body, back.body);
        assert_eq!(p.author, back.author);
        assert_eq!(p.created_at, back.created_at);
        assert_eq!(p.rules.len(), back.rules.len());
    }
}
