use crate::error::DagError;
use cid::multihash::{Code, MultihashDigest};
use cid::Cid;
use serde::{Deserialize, Serialize};

/// The type of event stored in the DAG
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub enum DagEventType {
    Genesis,
    Proposal,
    Vote,
    Execution,
    Attestation,
    Receipt,
    Anchor,
}

/// Represents a node in the Directed Acyclic Graph
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct DagNode {
    /// Content of the node (serialized payload)
    pub content: String,
    /// Optional parent CID
    #[serde(
        serialize_with = "serialize_cid_option",
        deserialize_with = "deserialize_cid_option"
    )]
    pub parent: Option<Cid>,
    /// Type of event this node represents
    pub event_type: DagEventType,
    /// Timestamp when this node was created (Unix timestamp in milliseconds)
    pub timestamp: u64,
    /// The scope ID this event belongs to (federation, cooperative, community)
    pub scope_id: String,
}

// Custom serializer for Option<Cid>
fn serialize_cid_option<S>(cid_opt: &Option<Cid>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match cid_opt {
        Some(cid) => serializer.serialize_str(&cid.to_string()),
        None => serializer.serialize_none(),
    }
}

// Custom deserializer for Option<Cid>
fn deserialize_cid_option<'de, D>(deserializer: D) -> Result<Option<Cid>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            let cid = Cid::try_from(s)
                .map_err(|e| serde::de::Error::custom(format!("Invalid CID: {}", e)))?;
            Ok(Some(cid))
        }
        None => Ok(None),
    }
}

/// The method used to calculate quorum for a governance event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub enum QuorumMethod {
    /// Simple majority (> 50%)
    Majority,
    /// Specific threshold (e.g., 66%)
    Threshold(u8),
    /// Weighted voting based on specified weights
    Weighted,
}

/// A lineage attestation that proves a connection between DAG events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct LineageAttestation {
    /// The CID of the event being attested to
    #[serde(serialize_with = "serialize_cid", deserialize_with = "deserialize_cid")]
    pub event_cid: Cid,
    /// Previous attestations in the lineage chain
    #[serde(
        serialize_with = "serialize_cid_vec",
        deserialize_with = "deserialize_cid_vec"
    )]
    pub previous_attestations: Vec<Cid>,
    /// Signatures from authenticating entities
    pub signatures: Vec<String>,
    /// Quorum method used for this attestation
    pub quorum_method: QuorumMethod,
}

/// A receipt proving that an execution occurred
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct ExecutionReceipt {
    /// The CID of the execution event
    #[serde(serialize_with = "serialize_cid", deserialize_with = "deserialize_cid")]
    pub execution_cid: Cid,
    /// Result of the execution (success/failure)
    pub success: bool,
    /// Output or error message from execution
    pub output: String,
    /// Resources consumed during execution
    pub resources_consumed: u64,
    /// Timestamp when execution completed
    pub timestamp: u64,
}

/// An anchor credential embedding a DAG root and epoch checkpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct AnchorCredential {
    /// The Merkle root of the DAG at this checkpoint
    #[serde(serialize_with = "serialize_cid", deserialize_with = "deserialize_cid")]
    pub dag_root: Cid,
    /// The epoch number for this checkpoint
    pub epoch: u64,
    /// Timestamp when this anchor was created
    pub timestamp: u64,
    /// Signatures from federation members
    pub signatures: Vec<String>,
}

// Serializer for single Cid
fn serialize_cid<S>(cid: &Cid, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&cid.to_string())
}

// Deserializer for single Cid
fn deserialize_cid<'de, D>(deserializer: D) -> Result<Cid, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Cid::try_from(s).map_err(|e| serde::de::Error::custom(format!("Invalid CID: {}", e)))
}

// Serializer for Vec<Cid>
fn serialize_cid_vec<S>(cids: &[Cid], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let strings: Vec<String> = cids.iter().map(|cid| cid.to_string()).collect();
    strings.serialize(serializer)
}

// Deserializer for Vec<Cid>
fn deserialize_cid_vec<'de, D>(deserializer: D) -> Result<Vec<Cid>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let strings: Vec<String> = Vec::deserialize(deserializer)?;
    let mut cids = Vec::with_capacity(strings.len());

    for s in strings {
        let cid = Cid::try_from(s)
            .map_err(|e| serde::de::Error::custom(format!("Invalid CID: {}", e)))?;
        cids.push(cid);
    }

    Ok(cids)
}

impl DagNode {
    pub fn cid(&self) -> Result<Cid, DagError> {
        let encoded =
            serde_cbor::to_vec(&self).map_err(|e| DagError::Serialization(e.to_string()))?;
        let hash = Code::Sha2_256.digest(&encoded);
        Ok(Cid::new_v1(0x71, hash))
    }

    /// Creates a builder initialized with values from this DagNode
    pub fn builder(&self) -> DagNodeBuilder {
        DagNodeBuilder {
            content: Some(self.content.clone()),
            parent: self.parent,
            event_type: Some(self.event_type.clone()),
            timestamp: Some(self.timestamp),
            scope_id: Some(self.scope_id.clone()),
        }
    }
}

/// Builder for creating DagNode instances
pub struct DagNodeBuilder {
    content: Option<String>,
    parent: Option<Cid>,
    event_type: Option<DagEventType>,
    timestamp: Option<u64>,
    scope_id: Option<String>,
}

impl Default for DagNodeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DagNodeBuilder {
    /// Creates a new DagNodeBuilder with no content or parent
    pub fn new() -> Self {
        Self {
            content: None,
            parent: None,
            event_type: None,
            timestamp: None,
            scope_id: None,
        }
    }

    /// Sets the content for the DagNode
    pub fn content(mut self, content: String) -> Self {
        self.content = Some(content);
        self
    }

    /// Sets the parent CID for the DagNode
    pub fn parent(mut self, parent_cid: Cid) -> Self {
        self.parent = Some(parent_cid);
        self
    }

    /// Sets the event type for the DagNode
    pub fn event_type(mut self, event_type: DagEventType) -> Self {
        self.event_type = Some(event_type);
        self
    }

    /// Sets the timestamp for the DagNode
    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// Sets the scope ID for the DagNode
    pub fn scope_id(mut self, scope_id: String) -> Self {
        self.scope_id = Some(scope_id);
        self
    }

    /// Builds a DagNode if all required fields are set
    pub fn build(self) -> Result<DagNode, DagError> {
        let content = self
            .content
            .ok_or_else(|| DagError::InvalidStructure("Content is required".to_string()))?;
        let event_type = self
            .event_type
            .ok_or_else(|| DagError::InvalidStructure("Event type is required".to_string()))?;
        let timestamp = self
            .timestamp
            .ok_or_else(|| DagError::InvalidStructure("Timestamp is required".to_string()))?;
        let scope_id = self
            .scope_id
            .ok_or_else(|| DagError::InvalidStructure("Scope ID is required".to_string()))?;

        Ok(DagNode {
            content,
            parent: self.parent,
            event_type,
            timestamp,
            scope_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_dag_node_creation() {
        let node = DagNodeBuilder::new()
            .content("Test content".to_string())
            .event_type(DagEventType::Genesis)
            .timestamp(1234567890)
            .scope_id("test-scope".to_string())
            .build()
            .unwrap();
        assert_eq!(node.content, "Test content");
        assert_eq!(node.event_type, DagEventType::Genesis);
        assert!(node.parent.is_none());
    }

    #[test]
    fn test_dag_node_with_parent() {
        let parent_node = DagNodeBuilder::new()
            .content("Parent content".to_string())
            .event_type(DagEventType::Genesis)
            .timestamp(1234500000)
            .scope_id("test-scope".to_string())
            .build()
            .unwrap();
        let parent_cid = parent_node.cid().unwrap();

        let child_node = DagNodeBuilder::new()
            .content("Child content".to_string())
            .parent(parent_cid)
            .event_type(DagEventType::Proposal)
            .timestamp(1234567890)
            .scope_id("test-scope".to_string())
            .build()
            .unwrap();
        assert_eq!(child_node.parent, Some(parent_cid));
    }

    #[test]
    fn test_dag_node_serialization() {
        let node = DagNodeBuilder::new()
            .content("{\"key\": \"value\"}".to_string())
            .event_type(DagEventType::Proposal)
            .timestamp(0)
            .scope_id("scope".to_string())
            .build()
            .unwrap();

        let serialized_cbor = serde_cbor::to_vec(&node).unwrap();
        let deserialized_node: DagNode = serde_cbor::from_slice(&serialized_cbor).unwrap();
        assert_eq!(node, deserialized_node);

        // Check CID generation consistency
        let cid1 = node.cid().unwrap();
        let cid2 = deserialized_node.cid().unwrap();
        assert_eq!(cid1, cid2);
    }

    // TODO: Add tests for LineageAttestation, ExecutionReceipt, AnchorCredential
    // TODO: Add tests for CID serialization/deserialization helpers

    #[test]
    fn test_lineage_attestation_serialization() {
        let event_cid =
            Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap(); // Example CID
        let prev_att_cid =
            Cid::try_from("bafybeihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku").unwrap(); // Example CID

        let attestation = LineageAttestation {
            event_cid,
            previous_attestations: vec![prev_att_cid],
            signatures: vec![
                "placeholder_sig_1".to_string(),
                "placeholder_sig_2".to_string(),
            ],
            quorum_method: QuorumMethod::Majority,
        };

        let serialized_cbor = serde_cbor::to_vec(&attestation).unwrap();
        let deserialized_attestation: LineageAttestation =
            serde_cbor::from_slice(&serialized_cbor).unwrap();
        assert_eq!(attestation, deserialized_attestation);

        let serialized_json = serde_json::to_string(&attestation).unwrap();
        let deserialized_attestation_json: LineageAttestation =
            serde_json::from_str(&serialized_json).unwrap();
        assert_eq!(attestation, deserialized_attestation_json);
    }

    #[test]
    fn test_execution_receipt_serialization() {
        let exec_cid =
            Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap(); // Example CID
        let receipt = ExecutionReceipt {
            execution_cid: exec_cid,
            success: true,
            output: "Execution successful".to_string(),
            resources_consumed: 12345,
            timestamp: 1678886400000, // Example timestamp
        };

        let serialized_cbor = serde_cbor::to_vec(&receipt).unwrap();
        let deserialized_receipt: ExecutionReceipt =
            serde_cbor::from_slice(&serialized_cbor).unwrap();
        assert_eq!(receipt, deserialized_receipt);

        let serialized_json = serde_json::to_string(&receipt).unwrap();
        let deserialized_receipt_json: ExecutionReceipt =
            serde_json::from_str(&serialized_json).unwrap();
        assert_eq!(receipt, deserialized_receipt_json);
    }

    #[test]
    fn test_anchor_credential_serialization() {
        let root_cid =
            Cid::try_from("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").unwrap(); // Example CID
        let anchor = AnchorCredential {
            dag_root: root_cid,
            epoch: 101,
            timestamp: 1678886400100, // Example timestamp
            signatures: vec!["fed_sig_1".to_string()],
        };

        let serialized_cbor = serde_cbor::to_vec(&anchor).unwrap();
        let deserialized_anchor: AnchorCredential =
            serde_cbor::from_slice(&serialized_cbor).unwrap();
        assert_eq!(anchor, deserialized_anchor);

        let serialized_json = serde_json::to_string(&anchor).unwrap();
        let deserialized_anchor_json: AnchorCredential =
            serde_json::from_str(&serialized_json).unwrap();
        assert_eq!(anchor, deserialized_anchor_json);
    }

    #[test]
    fn test_cid_serialization_helpers() {
        let cid_str = "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi";
        let cid = Cid::try_from(cid_str).unwrap();

        // Test single CID serde
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct CidWrapper {
            #[serde(serialize_with = "serialize_cid", deserialize_with = "deserialize_cid")]
            id: Cid,
        }
        let wrapper = CidWrapper { id: cid };
        let serialized = serde_json::to_string(&wrapper).unwrap();
        let deserialized: CidWrapper = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.id, cid);

        // Test Option<Cid> serde
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct OptionCidWrapper {
            #[serde(
                serialize_with = "serialize_cid_option",
                deserialize_with = "deserialize_cid_option"
            )]
            id: Option<Cid>,
        }
        let wrapper_some = OptionCidWrapper { id: Some(cid) };
        let serialized_some = serde_json::to_string(&wrapper_some).unwrap();
        let deserialized_some: OptionCidWrapper = serde_json::from_str(&serialized_some).unwrap();
        assert_eq!(deserialized_some.id, Some(cid));

        let wrapper_none = OptionCidWrapper { id: None };
        let serialized_none = serde_json::to_string(&wrapper_none).unwrap();
        let deserialized_none: OptionCidWrapper = serde_json::from_str(&serialized_none).unwrap();
        assert_eq!(deserialized_none.id, None); // Corrected assertion for None case

        // Test Vec<Cid> serde
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct VecCidWrapper {
            #[serde(
                serialize_with = "serialize_cid_vec",
                deserialize_with = "deserialize_cid_vec"
            )]
            ids: Vec<Cid>,
        }
        let wrapper_vec = VecCidWrapper {
            ids: vec![cid, cid],
        };
        let serialized_vec = serde_json::to_string(&wrapper_vec).unwrap();
        let deserialized_vec: VecCidWrapper = serde_json::from_str(&serialized_vec).unwrap();
        assert_eq!(deserialized_vec.ids, vec![cid, cid]);
    }
}

pub mod mesh;

// Reexport the public types
pub use mesh::ReceiptNode;
