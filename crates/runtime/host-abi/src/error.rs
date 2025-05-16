use thiserror::Error;
// use wasmtime::Trap; // Keep this commented if the impl From is commented

#[derive(Error, Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostAbiError {
    #[error("Unknown error: {0}")]
    UnknownError(String),
    #[error("Memory access error: {0}")]
    MemoryAccessError(String),
    #[error("Buffer too small: {0}")]
    BufferTooSmall(String),
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Not permitted")]
    NotPermitted,
    #[error("Not supported")]
    NotSupported,
    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),
    #[error("Data encoding error (UTF8/CBOR): {0}")]
    DataEncodingError(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Invalid DID format: {0}")]
    InvalidDIDFormat(String),
    #[error("Invalid CID format: {0}")]
    InvalidCIDFormat(String),
    #[error("Queue full: {0}")]
    QueueFull(String),
    #[error("Channel closed: {0}")]
    ChannelClosed(String),
    #[error("Insufficient balance")]
    InsufficientBalance,
    #[error("Invalid DID string: {0}")]
    InvalidDid(String),
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("Resource management error: {0}")]
    ResourceManagementError(String),
    // Consider adding other specific errors if needed, e.g.:
    // #[error("WASM guest module did not export a 'memory'")]
    // MissingMemory,
    // #[error("Context stack operation error: {0}")]
    // ContextError(String),
    // #[error("Schema validation failed: {0}")]
    // SchemaValidationError(String),
}

// TODO: Restore once Trap resolution issue is debugged.
/*
impl From<HostAbiError> for ::wasmtime::Trap {
    fn from(err: HostAbiError) -> ::wasmtime::Trap {
        ::wasmtime::Trap::new(err.to_string())
    }
}
*/ 