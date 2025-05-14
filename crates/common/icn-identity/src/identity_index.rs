use crate::Did;
use crate::ScopeKey;
use std::collections::HashMap;

/// In-memory index mapping DIDs -> organization hierarchy.
#[derive(Default, Clone)]
pub struct IdentityIndex {
    did_to_coop: HashMap<Did, String>,
    coop_to_community: HashMap<String, String>,
    community_to_federation: HashMap<String, String>,
}

impl IdentityIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register DID membership in a cooperative.
    pub fn insert_did_coop(&mut self, did: Did, coop_id: impl Into<String>) {
        self.did_to_coop.insert(did, coop_id.into());
    }

    pub fn insert_coop_community(
        &mut self,
        coop_id: impl Into<String>,
        community_id: impl Into<String>,
    ) {
        self.coop_to_community
            .insert(coop_id.into(), community_id.into());
    }

    pub fn insert_community_federation(
        &mut self,
        community_id: impl Into<String>,
        federation: impl Into<String>,
    ) {
        self.community_to_federation
            .insert(community_id.into(), federation.into());
    }

    /// Resolve an accounting `ScopeKey` for the given DID.
    pub fn resolve_scope_key(&self, did: &Did) -> ScopeKey {
        if let Some(coop) = self.did_to_coop.get(did) {
            if let Some(comm) = self.coop_to_community.get(coop) {
                if let Some(fid) = self.community_to_federation.get(comm) {
                    return ScopeKey::Federation(fid.clone());
                }
                return ScopeKey::Community(comm.clone());
            }
            return ScopeKey::Cooperative(coop.clone());
        }
        ScopeKey::Individual(did.to_string())
    }
}
