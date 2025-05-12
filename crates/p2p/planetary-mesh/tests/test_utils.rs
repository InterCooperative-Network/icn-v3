use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use cid::Cid;
use icn_types::reputation::ReputationRecord;
// Assuming MeshNode is accessible from this path, adjust if necessary
// For a test utility within the same crate's tests/ directory, you might access MeshNode via crate::node::MeshNode
// However, the user's snippet used `use crate::node::MeshNode;` which implies it might be a module within src or lib.rs is re-exporting it.
// For now, let's assume MeshNode will be resolved. If it's in `src/node.rs`, and `lib.rs` has `pub mod node;`, then `crate::node::MeshNode` is fine.
// If `test_utils.rs` is a module *within* `full_job_lifecycle.rs` or another test file, then `super::node::MeshNode` might be needed or direct `crate::node::MeshNode`.
// The prompt's `use crate::node::MeshNode` seems like a good default if `lib.rs` makes `node` public.
// Given the previous context where `node.rs` is `planetary_mesh/src/node.rs`, `crate::node::MeshNode` is the correct path
// when `test_utils.rs` is in `planetary_mesh/tests/test_utils.rs` if `planetary_mesh/src/lib.rs` has `pub mod node;`
// Let's assume `planetary_mesh/src/lib.rs` exists and declares `pub mod node;`
use planetary_mesh::node::MeshNode; // Using the crate name directly for items in src

/// Get the verified reputation records from a node for testing.
pub fn get_verified_reputation_records_arc(
    mesh_node: &MeshNode,
) -> Arc<RwLock<HashMap<Cid, ReputationRecord>>> {
    mesh_node.verified_reputation_records.clone()
}

// It seems the user snippet for full_job_lifecycle.rs implies NodeController, not direct MeshNode.
// I will stick to the user's accessor signature for now and they can adjust its usage.
// The previous accessor for test_observed_reputation_submissions also used NodeController.
// Let's provide a version that expects NodeController to be consistent.

/* 
// If NodeController is the common way to access MeshNode in tests:
pub fn get_verified_reputation_records_arc_with_controller(
    node_controller: &super::NodeController, // Assuming NodeController is defined in the parent (tests module) or accessible via super
) -> Arc<RwLock<HashMap<Cid, ReputationRecord>>> {
    node_controller.get_mesh_node_access(|mesh_node| {
        mesh_node.verified_reputation_records.clone()
    })
}
*/ 