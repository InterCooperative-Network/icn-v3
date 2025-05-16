use thiserror::Error;
// use wasmtime::Trap; // Keep this commented if the impl From is commented

#[derive(Error, Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostAbiError {
    #[error("Unknown error")]
    UnknownError,
    #[error("Memory access error")]
    MemoryAccessError,
    #[error("Buffer too small")]
    BufferTooSmall,
    #[error("Invalid arguments")]
    InvalidArguments,
    #[error("Not found")]
    NotFound,
    #[error("Timeout")]
    Timeout,
    #[error("Not permitted")]
    NotPermitted,
    #[error("Not supported")]
    NotSupported,
    #[error("Resource limit exceeded")]
    ResourceLimitExceeded,
    #[error("Data encoding error (UTF8/CBOR)")]
    DataEncodingError,
    #[error("Invalid state")]
    InvalidState,
    #[error("Network error")]
    NetworkError,
    #[error("Storage error")]
    StorageError,
    #[error("Serialization error")]
    SerializationError,
    #[error("Invalid DID format")]
    InvalidDIDFormat,
    #[error("Invalid CID format")]
    InvalidCIDFormat,
    #[error("Queue full")]
    QueueFull,
    #[error("Channel closed")]
    ChannelClosed,
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