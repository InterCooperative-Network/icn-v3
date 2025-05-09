#!/bin/bash
set -e

echo "📦 Creating icn-types crate structure..."

mkdir -p crates/common/icn-types/src/dag
cd crates/common/icn-types

cat <<EOF > Cargo.toml
[package]
name = "icn-types"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_cbor = "0.11"
thiserror = "1.0"
cid = "0.10"
multihash = { version = "0.17", features = ["sha2"] }

[dev-dependencies]
serde_test = "1"
EOF

cat <<EOF > src/lib.rs
pub mod dag;
pub mod error;
EOF

cat <<EOF > src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DagError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("CID error: {0}")]
    Cid(String),

    #[error("Invalid DAG structure: {0}")]
    InvalidStructure(String),
}
EOF

cat <<EOF > src/dag/mod.rs
use serde::{Serialize, Deserialize};
use multihash::{Code, MultihashDigest};
use cid::Cid;
use crate::error::DagError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    pub content: String,
    pub parent: Option<Cid>,
}

impl DagNode {
    pub fn cid(&self) -> Result<Cid, DagError> {
        let encoded = serde_cbor::to_vec(&self).map_err(|e| DagError::Serialization(e.to_string()))?;
        let hash = Code::Sha2_256.digest(&encoded);
        Cid::new_v1(0x71, hash).map_err(|e| DagError::Cid(e.to_string()))
    }
}
EOF

mkdir -p tests
cat <<EOF > tests/dag.rs
use icn_types::dag::DagNode;

#[test]
fn test_dag_cid_generation() {
    let node = DagNode {
        content: "Hello, ICN!".into(),
        parent: None,
    };

    let cid = node.cid().expect("CID generation failed");
    println!("CID: {}", cid);
}
EOF

cd ../../../..  # Return to root
cargo fmt
cargo check --workspace

echo "✅ icn-types crate structure created and ready."
