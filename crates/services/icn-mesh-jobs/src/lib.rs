// Expose modules
pub mod types;
pub mod models;
pub mod job_assignment;
pub mod bid_logic;
pub mod storage;
pub mod sqlite_store;
pub mod reputation_client;
pub mod reputation_cache;
pub mod metrics;
pub mod error;

// Re-export common types
pub use types::*;
pub use models::*;
pub use job_assignment::*;
pub use storage::MeshJobStore;
pub use sqlite_store::SqliteStore;
pub use reputation_client::ReputationClient;
pub use reputation_cache::CachingReputationClient;
pub use error::AppError; 