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
