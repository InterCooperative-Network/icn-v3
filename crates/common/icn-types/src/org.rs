// Organization types for the ICN v3 platform
//
// This module defines identifier types for organizational structures within the ICN:
// - Federations (groups of cooperatives)
// - Cooperatives (groups of communities)
// - Communities (functional units)
//
// These identifiers allow scoping of compute work, token allocation, and resource tracking.

use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

/// Unique identifier for a Cooperative within a Federation.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CooperativeId(pub String);

impl CooperativeId {
    /// Create a new CooperativeId
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl Display for CooperativeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a Community within a Cooperative.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommunityId(pub String);

impl CommunityId {
    /// Create a new CommunityId
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl Display for CommunityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
} 