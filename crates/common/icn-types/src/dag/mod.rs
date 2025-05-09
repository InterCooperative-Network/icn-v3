use crate::error::DagError;
use cid::multihash::{Code, MultihashDigest};
use cid::Cid;
use serde::{Deserialize, Serialize};

// Using a string representation of Cid for serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(crate = "serde")]
pub struct DagNode {
    pub content: String,
    #[serde(
        serialize_with = "serialize_cid_option",
        deserialize_with = "deserialize_cid_option"
    )]
    pub parent: Option<Cid>,
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
            parent: self.parent.clone(),
        }
    }
}

/// Builder for creating DagNode instances
pub struct DagNodeBuilder {
    content: Option<String>,
    parent: Option<Cid>,
}

impl DagNodeBuilder {
    /// Creates a new DagNodeBuilder with no content or parent
    pub fn new() -> Self {
        Self {
            content: None,
            parent: None,
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

    /// Builds a DagNode if all required fields are set
    pub fn build(self) -> Result<DagNode, DagError> {
        match self.content {
            Some(content) => Ok(DagNode {
                content,
                parent: self.parent,
            }),
            None => Err(DagError::InvalidStructure(
                "Content is required".to_string(),
            )),
        }
    }
}
