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

    Ok(())
} 