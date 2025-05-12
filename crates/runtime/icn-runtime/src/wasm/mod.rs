#[cfg(feature = "full_host_abi")]
pub mod linker;

#[cfg(feature = "full_host_abi")]
pub use linker::register_host_functions;

// Minimal stub when full_host_abi is disabled
#[cfg(not(feature = "full_host_abi"))]
pub fn register_host_functions<T>(_linker: &mut wasmtime::Linker<T>) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(feature = "full_host_abi"))]
pub mod linker {
    use super::*;
    use crate::host_environment::ConcreteHostEnvironment;

    /// Minimal stub store data when host ABI is disabled
    #[derive(Default)]
    pub struct StoreData {
        host_env: Option<ConcreteHostEnvironment>,
    }

    impl StoreData {
        pub fn new() -> Self { Self { host_env: None } }
        pub fn set_host(&mut self, host_env: ConcreteHostEnvironment) { self.host_env = Some(host_env); }
        #[allow(dead_code)]
        pub fn host(&self) -> &ConcreteHostEnvironment { self.host_env.as_ref().expect("host env not set") }
    }
} 