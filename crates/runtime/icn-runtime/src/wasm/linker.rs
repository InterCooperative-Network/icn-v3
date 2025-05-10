use anyhow::Result;
use host_abi::*;
use icn_economics::ResourceType;
use icn_mesh_receipts::ExecutionReceipt;
use wasmtime::{Caller, Linker};
use std::cell::RefCell;

/// Store data for the WASM engine, contains the host environment
pub struct StoreData {
    host_env: Option<RefCell<crate::host_environment::ConcreteHostEnvironment>>,
}

impl StoreData {
    /// Create a new store data instance
    pub fn new() -> Self {
        Self { host_env: None }
    }

    /// Set the host environment
    pub fn set_host(&mut self, host_env: crate::host_environment::ConcreteHostEnvironment) {
        self.host_env = Some(RefCell::new(host_env));
    }

    /// Get a reference to the host environment
    pub fn host(&self) -> std::cell::Ref<crate::host_environment::ConcreteHostEnvironment> {
        self.host_env.as_ref().expect("Host environment not set").borrow()
    }
    
    /// Get a mutable reference to the host environment
    pub fn host_mut(&self) -> std::cell::RefMut<crate::host_environment::ConcreteHostEnvironment> {
        self.host_env.as_ref().expect("Host environment not set").borrow_mut()
    }
}

/// Register all host functions for the economics module
pub fn register_host_functions(linker: &mut Linker<StoreData>) -> Result<()> {
    // Register the resource authorization check function
    linker.func_wrap(
        "icn_host", 
        "host_check_resource_authorization",
        |caller: Caller<'_, StoreData>, resource_type: u32, amount: u64| -> i32 {
            let rt: ResourceType = resource_type.into();
            caller.data().host().check_resource_authorization(rt, amount)
        },
    )?;

    // Register the resource usage recording function
    linker.func_wrap(
        "icn_host", 
        "host_record_resource_usage",
        |caller: Caller<'_, StoreData>, resource_type: u32, amount: u64| -> i32 {
            let rt: ResourceType = resource_type.into();
            let host_env = caller.data().host();
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(host_env.record_resource_usage(rt, amount))
        },
    )?;
    
    // Register the governance context check function
    linker.func_wrap(
        "icn_host",
        "host_is_governance_context",
        |caller: Caller<'_, StoreData>| -> i32 {
            caller.data().host().is_governance_context()
        },
    )?;
    
    // Register the mint token function
    linker.func_wrap(
        "icn_host",
        "host_mint_token",
        |mut caller: Caller<'_, StoreData>, recipient_ptr: i32, recipient_len: i32, amount: u64| -> i32 {
            // Read the recipient DID string from WASM memory
            let memory = caller.get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("WASM module must export memory");
            
            let mut buffer = vec![0u8; recipient_len as usize];
            if memory.read(&mut caller, recipient_ptr as usize, &mut buffer).is_err() {
                return -2; // Memory read error
            }
            
            // Convert to UTF-8 string
            match String::from_utf8(buffer) {
                Ok(recipient_did) => {
                    let host_env = caller.data().host_mut();
                    let runtime = tokio::runtime::Runtime::new().unwrap();
                    runtime.block_on(host_env.mint_token(&recipient_did, amount))
                },
                Err(_) => -2, // Invalid UTF-8
            }
        },
    )?;
    
    // Register the transfer token function
    linker.func_wrap(
        "icn_host",
        "host_transfer_token",
        |mut caller: Caller<'_, StoreData>, sender_ptr: i32, sender_len: i32, 
                                       recipient_ptr: i32, recipient_len: i32, amount: u64| -> i32 {
            // Get the memory
            let memory = caller.get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("WASM module must export memory");
            
            // Read the sender DID string
            let mut sender_buffer = vec![0u8; sender_len as usize];
            if memory.read(&mut caller, sender_ptr as usize, &mut sender_buffer).is_err() {
                return -2; // Memory read error
            }
            
            // Read the recipient DID string
            let mut recipient_buffer = vec![0u8; recipient_len as usize];
            if memory.read(&mut caller, recipient_ptr as usize, &mut recipient_buffer).is_err() {
                return -2; // Memory read error
            }
            
            // Convert to UTF-8 strings
            match (String::from_utf8(sender_buffer), String::from_utf8(recipient_buffer)) {
                (Ok(sender_did), Ok(recipient_did)) => {
                    let host_env = caller.data().host_mut();
                    let runtime = tokio::runtime::Runtime::new().unwrap();
                    runtime.block_on(host_env.transfer_token(&sender_did, &recipient_did, amount))
                },
                _ => -2, // Invalid UTF-8
            }
        },
    )?;
    
    // Register the anchor receipt function
    linker.func_wrap(
        "icn_host", 
        "host_anchor_receipt", 
        |mut caller: Caller<'_, StoreData>, ptr: u32, len: u32| -> i32 {
            // Get the memory
            let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                Some(mem) => mem,
                None => return -3, // Memory not found
            };
            
            // Read the receipt bytes from memory
            let data = memory.data(&caller);
            let start = ptr as usize;
            let end = start + len as usize;
            
            // Check bounds
            if end > data.len() {
                return -2; // Out of bounds
            }
            
            let bytes = &data[start..end];
            
            // Deserialize the receipt
            let receipt: ExecutionReceipt = match serde_cbor::from_slice(bytes) {
                Ok(r) => r,
                Err(_) => return -1, // Deserialization error
            };
            
            // We'll use a blocking approach here since wasmtime doesn't support async calls
            // In a production system, this should be handled differently with proper async
            let host_env = caller.data().host_mut();
            let runtime = tokio::runtime::Runtime::new().unwrap();
            
            match runtime.block_on(host_env.anchor_receipt(receipt)) {
                Ok(()) => 0, // Success
                Err(e) => {
                    use crate::host_environment::AnchorError;
                    match e {
                        AnchorError::ExecutorMismatch(_, _) => -10, // Executor mismatch
                        AnchorError::InvalidSignature(_) => -11,     // Invalid signature
                        AnchorError::SerializationError(_) => -12,   // Serialization error
                        AnchorError::CidError(_) => -13,             // CID error
                        AnchorError::DagStoreError(_) => -14,        // DAG store error
                        AnchorError::MissingFederationId => -15,     // Missing federation ID
                    }
                }
            }
        },
    )?;

    Ok(())
} 