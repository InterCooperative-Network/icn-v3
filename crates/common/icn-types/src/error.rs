use thiserror::Error;
// Add imports for specific error types
use icn_identity::vc::CredentialError;
use icn_identity::did::DidError;
use icn_crypto::jws::JwsError;
use icn_identity::quorum::QuorumError;
use icn_identity::trust_bundle::TrustBundleError;
use serde_json;

/// Errors related to identity operations
#[derive(Error, Debug)]
pub enum IdentityError {
    #[error("Local Verifiable Credential error: {0}")]
    LocalVc(#[from] VcError),

    #[error("External Credential processing error: {source}")]
    ExternalCredentialProcessing { #[from] source: CredentialError },

    #[error("DID processing error: {source}")]
    DidProcessing { #[from] source: DidError },

    #[error("JWS processing error: {source}")]
    JwsProcessing { #[from] source: JwsError },

    #[error("Quorum rule processing error: {source}")]
    QuorumProcessing { #[from] source: QuorumError },

    #[error("Trust bundle processing error: {source}")]
    TrustBundleProcessing { #[from] source: TrustBundleError },

    #[error("JSON deserialization error: {source}")]
    Deserialization { #[from] source: serde_json::Error },
}

/// Errors related to trust operations
#[derive(Error, Debug)]
pub enum TrustError {
    #[error("Invalid trust bundle: {0}")]
    InvalidBundle(String),

    #[error("Invalid credential in bundle: {0}")]
    InvalidCredential(String),

    #[error("Invalid quorum configuration: {0}")]
    InvalidQuorumConfig(String),

    #[error("Quorum not satisfied")]
    QuorumNotSatisfied,

    #[error("Unauthorized signer: {0}")]
    UnauthorizedSigner(String),

    #[error("Duplicate signers detected")]
    DuplicateSigners,

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("Identity error: {0}")]
    IdentityError(#[from] IdentityError),

    #[error("Crypto error: {0}")]
    CryptoError(#[from] CryptoError),
}

/// Generic error type for ICN operations
#[derive(Debug, thiserror::Error)]
pub enum IcnError {
    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("DAG error: {0}")]
    Dag(#[from] DagError),

    #[error("Multicodec error: {0}")]
    Multicodec(#[from] MulticodecError),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("General error: {0}")]
    General(String),
}

/// Crypto-related error types
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("Key generation error: {0}")]
    KeyGenError(String),

    #[error("Signature error: {0}")]
    SignatureError(String),

    #[error("Verification error: {0}")]
    VerificationError(String),

    #[error("Invalid key format: {0}")]
    InvalidKeyFormat(String),

    #[error("Encoding error: {0}")]
    EncodingError(String),

    #[error("JWS error: {0}")]
    JwsError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Multicodec-related error types
#[derive(Debug, thiserror::Error)]
pub enum MulticodecError {
    #[error("Unknown codec: {0}")]
    UnknownCodec(String),

    #[error("Unsupported codec: {0}")]
    UnsupportedCodec(String),

    #[error("Invalid multicodec header: {0}")]
    InvalidHeader(String),

    #[error("Encoding error: {0}")]
    EncodingError(String),

    #[error("Decoding error: {0}")]
    DecodingError(String),
}

/// DAG-related error types
#[derive(Debug, thiserror::Error)]
pub enum DagError {
    #[error("Invalid link: {0}")]
    InvalidLink(String),

    #[error("Missing link: {0}")]
    MissingLink(String),

    #[error("Invalid CID: {0}")]
    InvalidCid(String),

    #[error("Invalid DAG node: {0}")]
    InvalidNode(String),

    #[error("DAG verification failed: {0}")]
    VerificationFailed(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Invalid structure: {0}")]
    InvalidStructure(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Error types for Verifiable Credential operations
#[derive(thiserror::Error, Debug)]
pub enum VcError {
    #[error("Failed to serialize credential: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Failed to sign credential: {0}")]
    Signing(#[from] icn_crypto::jws::JwsError),

    #[error("Invalid credential structure")]
    InvalidStructure,

    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Mesh-related error types
#[derive(thiserror::Error, Debug)]
pub enum MeshError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Job submission failed: {0}")]
    JobSubmission(String),
    #[error("Receipt error: {0}")]
    ReceiptError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Operation timeout: {0}")]
    Timeout(String),
    #[error("Resource not found: {0}")]
    NotFound(String),
}
