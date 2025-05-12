// Placeholder for the historical Wasmtime host-function glue table.
// The full code was moved out of the default build to unblock compilation.
// Enable with: `--features full_host_abi` in icn-runtime.

use anyhow::Result;
use wasmtime::{Caller, Linker, Trap, Memory};
use crate::host_environment::ConcreteHostEnvironment;
use icn_mesh_receipts::ExecutionReceipt;
use serde_cbor;

/// Minimal host_anchor_receipt implementation. Reads CBOR bytes from guest
/// memory, decodes an `ExecutionReceipt`, and calls `anchor_receipt` on the
/// host environment.  Returns `0` on success for now (CID return TBD).
async fn host_anchor_receipt(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    ptr: u32,
    len: u32,
) -> Result<u32, Trap> {
    // Access memory export
    let memory: Memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| Trap::new("memory export not found"))?;

    let mut buf = vec![0u8; len as usize];
    memory
        .read(&caller, ptr as usize, &mut buf)
        .map_err(|e| Trap::new(format!("memory read failed: {e}")))?;

    let receipt: ExecutionReceipt = serde_cbor::from_slice(&buf)
        .map_err(|e| Trap::new(format!("CBOR decode failed: {e}")))?;

    caller
        .data()
        .anchor_receipt(receipt)
        .await
        .map_err(|e| Trap::new(format!("anchor_receipt failed: {e}")))?;

    Ok(0)
}

/// Register ICN host functions (legacy/full build).
pub fn register_host_functions(linker: &mut Linker<ConcreteHostEnvironment>) -> Result<()> {
    linker.func_wrap_async("icn", "host_anchor_receipt", host_anchor_receipt)?;
    Ok(())
} 