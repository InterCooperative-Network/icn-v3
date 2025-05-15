use thiserror::Error;
// Add imports for specific error types
use icn_identity::vc::CredentialError;
use icn_identity::did::DidError;
use icn_crypto::jws::JwsError;
use icn_identity::quorum::QuorumError;
use icn_identity::trust_bundle::TrustBundleError;
use serde_json;
use std::io;
use url;
use ed25519_dalek;
use base64;
use serde_cbor;
use cid;
use serde_ipld_dagcbor::{DecodeError as IpldDecodeError, EncodeError as IpldEncodeError};

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
    #[error("Trust bundle processing error: {0}")]
    BundleProcessing(#[from] icn_identity::trust_bundle::TrustBundleError),

    #[error("Error with local credential in bundle: {0}")]
    LocalCredentialInBundle(#[from] VcError),

    #[error("Error with external credential in bundle: {0}")]
    ExternalCredentialInBundle(#[from] icn_identity::vc::CredentialError),

    #[error("Quorum processing error: {0}")]
    QuorumProcessing(#[from] icn_identity::quorum::QuorumError),

    #[error("JWS verification failed: {0}")]
    JwsVerification(#[from] icn_crypto::jws::JwsError),

    #[error("Identity error underlying trust operation: {0}")]
    Identity(#[from] IdentityError),

    #[error("Cryptographic error underlying trust operation: {0}")]
    Crypto(#[from] CryptoError),
}

/// Generic error type for ICN operations
#[derive(Debug, thiserror::Error)]
pub enum IcnError {
    // --- Errors from dependent local ICN modules/types ---
    #[error("Cryptography error: {0}")]
    Crypto(#[from] CryptoError),
    #[error("DAG processing error: {0}")]
    Dag(#[from] DagError),
    #[error("Multicodec error: {0}")]
    Multicodec(#[from] MulticodecError),
    #[error("Identity operation error: {0}")]
    Identity(#[from] IdentityError),
    #[error("Trust operation error: {0}")]
    Trust(#[from] TrustError),
    #[error("Mesh operation error: {0}")]
    Mesh(#[from] MeshError),
    
    #[error("Economics error: {0}")]
    Economics(String),

    // --- Common I/O, Parsing, and System Errors ---
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization/Deserialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Invalid URI: {0}")]
    InvalidUri(#[from] url::ParseError),

    #[error("Operation timed out: {0}")]
    Timeout(String),
    #[error("Configuration error: {0}")]
    Config(String),

    // --- General Application-level Errors ---
    #[error("Storage operation failed: {0}")]
    Storage(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Plugin error: {0}")]
    Plugin(String),
    #[error("Consensus error: {0}")]
    Consensus(String),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
    #[error("Resource not found: {0}")]
    NotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("General error: {0}")]
    General(String),
}

/// Crypto-related error types
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("Key generation failed: {source}")]
    KeyGeneration { #[source] source: ed25519_dalek::SignatureError },

    #[error("Digital signature creation failed: {0}")]
    SignatureCreationFailure(String),

    #[error("Signature verification failed: {source}")]
    Verification { #[source] source: ed25519_dalek::SignatureError },

    // InvalidKeyFormat variants
    #[error("Invalid key data for cryptographic operation: {0}")]
    KeyDataInvalid(#[from] ed25519_dalek::SignatureError),
    #[error("Invalid key format (base64 decode failed): {0}")]
    KeyFormatBase64(#[from] base64::DecodeError),
    #[error("Invalid key format (json deserialize failed): {0}")]
    KeyFormatJson(#[from] serde_json::Error),
    #[error("Invalid key format (unspecified): {0}")]
    KeyFormatGeneric(String),

    // EncodingError variants
    #[error("Base64 encoding/decoding error: {source}")]
    Base64Processing { #[source] source: base64::DecodeError },
    #[error("Generic encoding error: {0}")]
    EncodingGeneric(String),

    #[error("JWS processing error: {0}")]
    Jws(#[from] icn_crypto::jws::JwsError),

    // SerializationError variants
    #[error("JSON serialization/deserialization error: {source}")]
    JsonProcessing { #[source] source: serde_json::Error },
    #[error("CBOR serialization/deserialization error: {0}")]
    CborProcessing(#[from] serde_cbor::Error),
    #[error("Generic serialization error: {0}")]
    SerializationGeneric(String),

    #[error("Unknown or unspecified crypto error: {0}")]
    Unknown(String),
}

/// Multicodec-related error types
#[derive(Debug, thiserror::Error)]
pub enum MulticodecError {
    #[error("Multicodec processing error from underlying library: {0}")]
    CidLib(#[from] cid::Error),

    #[error("Application does not support codec 0x{code:x}{}", name.as_ref().map_or_else(String::new, |n| format!(" ({})", n)))]
    UnsupportedByApplication { code: u64, name: Option<String> },

    #[error("Application-specific multicodec logic error: {0}")]
    AppLogic(String),
}

/// DAG-related error types
#[derive(Debug, thiserror::Error)]
pub enum DagError {
    #[error("Invalid CID format or value: {0}")]
    MalformedCid(#[from] cid::Error),

    #[error("IPLD encoding failed: {0}")]
    IpldEncode(#[from] IpldEncodeError),

    #[error("IPLD decoding failed: {0}")]
    IpldDecode(#[from] IpldDecodeError),

    #[error("CBOR processing error: {0}")]
    Cbor(#[from] serde_cbor::Error),

    #[error("Link target not found for CID: {cid}")]
    LinkNotFound { cid: cid::Cid },

    #[error("Link is structurally invalid in node (CID: {node_cid:?}): {reason}. Link: '{link_value}'")]
    LinkInvalidInNode {
        reason: String,
        node_cid: Option<cid::Cid>,
        link_value: String,
    },

    #[error("Node content or structure is invalid after decoding (CID: {node_cid:?}): {reason}")]
    NodeValidation {
        reason: String,
        node_cid: Option<cid::Cid>,
    },

    #[error("DAG integrity verification failed for CID {cid}: {reason}")]
    Integrity { cid: cid::Cid, reason: String },

    #[error("Cycle detected in DAG traversal: {context}")]
    CycleDetected { context: String },

    #[error("DAG traversal failed: {reason}")]
    TraversalFailure { reason: String },

    #[error("DAG operation failed due to unspecified reason: {0}")]
    Unspecified(String),
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
    #[error("Mesh network I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to submit job to mesh: {0}")]
    JobSubmission(String),

    #[error("Error related to mesh execution receipt: {0}")]
    ReceiptProcessing(String),

    #[error("Mesh configuration error: {0}")]
    Configuration(String),

    #[error("Mesh operation timed out: {0}")]
    OperationTimeout(String),

    #[error("Mesh resource not found - Type: {resource_type}, ID: {identifier}")]
    ResourceNotFound {
        resource_type: String,
        identifier: String,
    },

    #[error("Invalid mesh message format: {0}")]
    InvalidMessage(String),

    #[error("Peer unreachable: {peer_id}")]
    PeerUnreachable { peer_id: String },

    #[error("Mesh protocol violation: {0}")]
    ProtocolViolation(String),
}
