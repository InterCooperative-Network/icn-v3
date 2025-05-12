// Placeholder for the historical Wasmtime host-function glue table.
// The full code was moved out of the default build to unblock compilation.
// Enable with: `--features full_host_abi` in icn-runtime.

use anyhow::Result;
use wasmtime::{Caller, Linker, Trap, Memory};
use crate::host_environment::ConcreteHostEnvironment;
use icn_mesh_receipts::ExecutionReceipt;
use serde_cbor;
use icn_identity::ScopeKey;

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

/// Get mana balance for a DID (0-length str = caller DID).
async fn host_account_get_mana(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    did_ptr: u32,
    did_len: u32,
) -> Result<i64, Trap> {
    let host_env_ref = caller.data();

    // Determine scope key
    let scope_key = if did_len == 0 {
        host_env_ref.scope_key()
    } else {
        let did_str = host_env_ref
            .read_string_from_mem(&mut caller, did_ptr, did_len)
            .map_err(|e| Trap::new(format!("memory read failed: {e}")))?;
        ScopeKey::Individual(did_str)
    };

    let mut mana_mgr = host_env_ref
        .rt
        .mana_manager
        .lock()
        .map_err(|_| Trap::new("mana manager poisoned"))?;

    let bal = mana_mgr.balance(&scope_key).unwrap_or(0) as i64;
    Ok(bal)
}

/// Spend mana for a DID (0-length str = caller DID).
async fn host_account_spend_mana(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    did_ptr: u32,
    did_len: u32,
    amount: u64,
) -> Result<i32, Trap> {
    let host_env_ref = caller.data();

    let scope_key = if did_len == 0 {
        host_env_ref.scope_key()
    } else {
        let did_str = host_env_ref
            .read_string_from_mem(&mut caller, did_ptr, did_len)
            .map_err(|e| Trap::new(format!("memory read failed: {e}")))?;
        ScopeKey::Individual(did_str)
    };

    let mut mana_mgr = host_env_ref
        .rt
        .mana_manager
        .lock()
        .map_err(|_| Trap::new("mana manager poisoned"))?;

    match mana_mgr.spend(&scope_key, amount) {
        Ok(_) => Ok(0),
        Err(_) => Ok(-1), // insufficient mana or unknown DID
    }
}

/// Register ICN host functions (legacy/full build).
pub fn register_host_functions(linker: &mut Linker<ConcreteHostEnvironment>) -> Result<()> {
    linker.func_wrap_async("icn", "host_anchor_receipt", host_anchor_receipt)?;
    linker.func_wrap_async("icn", "host_account_get_mana", host_account_get_mana)?;
    linker.func_wrap_async("icn", "host_account_spend_mana", host_account_spend_mana)?;
    Ok(())
} 