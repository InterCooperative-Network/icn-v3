pub mod bindings;

// Export all bindings at the crate root for easy access
pub use bindings::*;

pub const ICN_HOST_ABI_VERSION: u32 = 8; // bump from 7 → 8 for mesh job submission ABI change 