use thiserror::Error;

/// Errors related to identity operations
#[derive(Error, Debug)]
pub enum IdentityError {
    #[error("Invalid credential: {0}")]
    InvalidCredential(String),
    
    #[error("Invalid DID: {0}")]
    InvalidDid(String),
    
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    
    #[error("Deserialization failed: {0}")]
    DeserializationFailed(String),
}

/// Errors related to crypto operations
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    
    #[error("Signature verification failed: {0}")]
    VerificationFailed(String),
    
    #[error("Encoding error: {0}")]
    EncodingError(String),
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

/// Errors related to DAG operations
#[derive(Error, Debug)]
pub enum DagError {
    #[error("Invalid CID: {0}")]
    InvalidCid(String),
    
    #[error("Encoding error: {0}")]
    EncodingError(String),
    
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Error, Debug)]
pub enum IcnError {
    #[error("DAG error: {0}")]
    Dag(#[from] DagError),

    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("Identity error: {0}")]
    Identity(#[from] IdentityError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unknown error: {0}")]
    Unknown(String),
}
