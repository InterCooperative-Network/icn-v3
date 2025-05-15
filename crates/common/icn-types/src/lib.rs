pub mod crypto;
pub mod dag;
pub mod dag_store;
pub mod error;
pub mod identity;
pub mod jobs;
pub mod mana;
pub mod mesh;
pub mod org;
pub mod receipt_verification;
pub mod reputation;
pub mod resource;
pub mod runtime_receipt;
pub mod trust;

pub use error::{IcnError, CryptoError, DagError, MulticodecError, IdentityError, TrustError, MeshError, VcError, SignError, EconomicsError, JobFailureReason};
pub use runtime_receipt::{RuntimeExecutionMetrics, RuntimeExecutionReceipt};
pub use mesh::{JobStatus as MeshJobStatus, MeshJob, MeshJobParams, QoSProfile, WorkflowType};
pub use org::{CommunityId, CooperativeId};
pub use receipt_verification::{ExecutionReceiptPayload, VerifiableReceipt};

// Corrected jobs re-export to only include types actually defined in icn_types::jobs
pub use jobs::{policy::ExecutionPolicy, TokenAmount};

pub use mana::{ManaState, ScopedMana};
pub use reputation::{
    compute_score as compute_reputation_score, ReputationProfile, ReputationRecord,
    ReputationUpdateEvent,
};
pub use resource::ResourceType;

// Re-export did and cid types from icn_identity and cid crates for convenience
pub use icn_identity::{Did, DidError, CredentialError, QuorumError, TrustBundleError, /* TrustAnchor, */ TrustBundle};
pub use cid::Cid;
