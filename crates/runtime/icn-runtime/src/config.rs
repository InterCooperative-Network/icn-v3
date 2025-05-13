use serde::Deserialize;
use std::path::PathBuf;

/// Configuration for the ICN Runtime
#[derive(Debug, Deserialize, Clone)]
pub struct RuntimeConfig {
    /// The DID of this runtime node. This will be derived from the key_path if provided.
    pub node_did: String,

    /// Path to the directory for persistent storage (e.g., Sled DB).
    pub storage_path: PathBuf,

    /// Optional path to a file storing the node's identity KeyPair.
    /// If not provided, or if the file doesn't exist, a new keypair will be generated.
    /// If provided and the file exists but is invalid, an error will occur.
    pub key_path: Option<PathBuf>,

    /// Optional URL for the reputation service.
    pub reputation_service_url: Option<String>,

    /// Optional URL for the mesh job service to poll for new jobs.
    pub mesh_job_service_url: Option<String>,

    /// Optional port for Prometheus metrics http endpoint.
    pub metrics_port: Option<u16>,

    /// Optional log level string (e.g., "info", "debug", "icn_runtime=trace").
    pub log_level: Option<String>,
} 