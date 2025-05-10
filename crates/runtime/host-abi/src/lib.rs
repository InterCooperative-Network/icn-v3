pub mod bindings;

// Export all bindings at the crate root for easy access
pub use bindings::*;

pub const ICN_HOST_ABI_VERSION: u32 = 5; // bump from 3 → 5 for token transfer 