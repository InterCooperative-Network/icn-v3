use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct RuntimeConfig {
    /// The DID of this runtime node (used for signing, reputation, etc.)
    pub node_did: String,

    /// Path to local storage directory (e.g., for DAG store, job queue, etc.)
    pub storage_path: PathBuf,

    /// Optional path to a file or keystore containing the node's private key
    pub key_path: Option<PathBuf>,

    /// Base URL of the reputation service (used for reporting receipts, etc.)
    pub reputation_service_url: Option<String>,

    /// Base URL of the mesh job service (used to submit or fetch job data)
    pub mesh_job_service_url: Option<String>,

    /// Prometheus metrics exporter port
    pub metrics_port: Option<u16>,

    /// Verbosity / log level (e.g., "info", "debug", "trace")
    pub log_level: Option<String>,
} 