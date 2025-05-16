pub mod linker;
pub mod linker_legacy_impl;

pub use linker::{register_host_functions, StoreData};

// linker.rs already exposes a stub when `full_host_abi` is disabled, so no
// additional inline stub is necessary here.
