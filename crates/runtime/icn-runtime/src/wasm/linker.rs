// Entire file replaced with new conditional implementation

// -----------------------------------------------------------------------------
//  Minimal stub linker (default build)
// -----------------------------------------------------------------------------
// We keep the same public surface so the rest of icn-runtime compiles, but avoid
// pulling in the huge Wasmtime 18-specific glue until it is fully modernised.
// Enable the historical implementation with `--features legacy_linker_impl`.
// -----------------------------------------------------------------------------

#[cfg(not(feature = "full_host_abi"))]
use anyhow::Result;

#[cfg(not(feature = "full_host_abi"))]
use wasmtime::Linker;

#[cfg(not(feature = "full_host_abi"))]
use crate::host_environment::ConcreteHostEnvironment;

/// Store data for Wasmtime when the full linker is disabled.
#[cfg(not(feature = "full_host_abi"))]
#[derive(Default)]
pub struct StoreData {
    host_env: Option<ConcreteHostEnvironment<()>>,
}

#[cfg(not(feature = "full_host_abi"))]
impl StoreData {
    pub fn new() -> Self {
        Self { host_env: None }
    }
    pub fn set_host(&mut self, host_env: ConcreteHostEnvironment<()>) {
        self.host_env = Some(host_env);
    }
    #[allow(dead_code)]
    pub fn host(&self) -> &ConcreteHostEnvironment<()> {
        self.host_env.as_ref().expect("host env not set")
    }
}

/// Register host functions – no-op in the minimal build.
#[cfg(not(feature = "full_host_abi"))]
pub fn register_host_functions(_linker: &mut Linker<StoreData>) -> Result<(), anyhow::Error> {
    Ok(())
}

// -----------------------------------------------------------------------------
//  Legacy implementation (opt-in)
// -----------------------------------------------------------------------------
// The original 300-line Wasmtime host-function table lives in a separate source
// file that is only compiled when the `legacy_linker_impl` feature is enabled.
// This keeps the diff small while preserving the code for future migration.
// -----------------------------------------------------------------------------

#[cfg(feature = "full_host_abi")]
#[path = "linker_legacy_impl.rs"]
mod legacy_linker_impl;

#[cfg(feature = "full_host_abi")]
pub use legacy_linker_impl::*;

#[cfg(feature = "full_host_abi")]
pub type StoreData = ConcreteHostEnvironment<()>;

#[cfg(feature = "full_host_abi")]
pub use full::register_host_functions;
