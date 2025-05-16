// WASM Linker for ICN Runtime
// This module defines how host functions are registered with the Wasmtime Linker.

use anyhow::Result;
use wasmtime::Linker;

// Import ConcreteHostEnvironment, assuming it's at crate::host_environment
use crate::host_environment::ConcreteHostEnvironment;

// For full_host_abi, we use types and functions from linker_legacy_impl.rs
#[cfg(feature = "full_host_abi")]
pub use crate::wasm::linker_legacy_impl::register_host_functions;

#[cfg(feature = "full_host_abi")]
pub type StoreData = ConcreteHostEnvironment<()>;


// Provide default/minimal implementations when 'full_host_abi' is not enabled
#[cfg(not(feature = "full_host_abi"))]
pub fn register_host_functions<T: Send + Sync + 'static>(
    _linker: &mut Linker<T>,
) -> Result<()> {
    // Minimal or no host functions registered in non-ABI build
    // Example: might include a basic log_message or be a no-op
    Ok(())
}

#[cfg(not(feature = "full_host_abi"))]
pub type StoreData = (); // Minimal store data for non-full ABI builds
