use anyhow::Result;
use host_abi::*;
use icn_economics::ResourceType;
use wasmtime::{Caller, Linker};

/// Store data for the WASM engine, contains the host environment
pub struct StoreData {
    host_env: Option<crate::host_environment::ConcreteHostEnvironment>,
}

impl StoreData {
    /// Create a new store data instance
    pub fn new() -> Self {
        Self { host_env: None }
    }

    /// Set the host environment
    pub fn set_host(&mut self, host_env: crate::host_environment::ConcreteHostEnvironment) {
        self.host_env = Some(host_env);
    }

    /// Get a reference to the host environment
    pub fn host(&self) -> &crate::host_environment::ConcreteHostEnvironment {
        self.host_env.as_ref().expect("Host environment not set")
    }
}

/// Register all host functions for the economics module
pub fn register_host_functions(linker: &mut Linker<StoreData>) -> Result<()> {
    // Register the resource authorization check function
    linker.func_wrap(
        "icn_host", 
        "host_check_resource_authorization",
        |mut caller: Caller<'_, StoreData>, resource_type: u32, amount: u64| -> i32 {
            let rt: ResourceType = resource_type.into();
            caller.data().host().check_resource_authorization(rt, amount)
        },
    )?;

    // Register the resource usage recording function
    linker.func_wrap(
        "icn_host", 
        "host_record_resource_usage",
        |mut caller: Caller<'_, StoreData>, resource_type: u32, amount: u64| -> i32 {
            let rt: ResourceType = resource_type.into();
            caller.data().host().record_resource_usage(rt, amount)
        },
    )?;
    
    // Register the governance context check function
    linker.func_wrap(
        "icn_host",
        "host_is_governance_context",
        |mut caller: Caller<'_, StoreData>| -> i32 {
            caller.data().host().is_governance_context()
        },
    )?;
    
    // Register the mint token function
    linker.func_wrap(
        "icn_host",
        "host_mint_token",
        |mut caller: Caller<'_, StoreData>, recipient_ptr: i32, recipient_len: i32, amount: u64| -> i32 {
            // Get the host environment
            let host = caller.data().host();
            
            // Read the recipient DID string from WASM memory
            let memory = caller.get_export("memory").and_then(|e| e.into_memory()).expect("WASM module must export memory");
            let mut buffer = vec![0u8; recipient_len as usize];
            if memory.read(&mut caller, recipient_ptr as usize, &mut buffer).is_err() {
                return -2; // Memory read error
            }
            
            // Convert to UTF-8 string
            match String::from_utf8(buffer) {
                Ok(recipient_did) => host.mint_token(&recipient_did, amount),
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
            // Get the host environment
            let host = caller.data().host();
            
            // Read the sender DID string from WASM memory
            let memory = caller.get_export("memory").and_then(|e| e.into_memory()).expect("WASM module must export memory");
            let mut sender_buffer = vec![0u8; sender_len as usize];
            if memory.read(&mut caller, sender_ptr as usize, &mut sender_buffer).is_err() {
                return -2; // Memory read error
            }
            
            // Read the recipient DID string from WASM memory
            let mut recipient_buffer = vec![0u8; recipient_len as usize];
            if memory.read(&mut caller, recipient_ptr as usize, &mut recipient_buffer).is_err() {
                return -2; // Memory read error
            }
            
            // Convert to UTF-8 strings
            match (String::from_utf8(sender_buffer), String::from_utf8(recipient_buffer)) {
                (Ok(sender_did), Ok(recipient_did)) => 
                    host.transfer_token(&sender_did, &recipient_did, amount),
                _ => -2, // Invalid UTF-8
            }
        },
    )?;

    Ok(())
} 