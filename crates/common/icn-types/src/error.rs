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

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Signing error: {0}")]
    SigningError(String),

    #[error("Verification error: {0}")]
    VerificationError(String),

    #[error("Key generation error: {0}")]
    KeyGenError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

#[derive(Error, Debug)]
pub enum IdentityError {
    #[error("Invalid DID: {0}")]
    InvalidDid(String),

    #[error("Invalid credential: {0}")]
    InvalidCredential(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),
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
