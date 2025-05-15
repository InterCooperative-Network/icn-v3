// Placeholder for the historical Wasmtime host-function glue table.
// The full code was moved out of the default build to unblock compilation.
// Enable with: `--features full_host_abi` in icn-runtime.

use crate::host_environment::ConcreteHostEnvironment;
use anyhow::Result;
use icn_identity::ScopeKey;
use icn_mesh_receipts::ExecutionReceipt;
use serde_cbor;
use wasmtime::{Caller, Linker, Memory, Trap};
use anyhow::anyhow;
use host_abi::{MeshHostAbi, LogLevel as HostAbiLogLevel}; // Renamed LogLevel to avoid conflict if linker_legacy_impl has its own
use icn_types::mesh::MeshJobParams; // For host_submit_mesh_job potentially later

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

    let receipt: ExecutionReceipt =
        serde_cbor::from_slice(&buf).map_err(|e| Trap::new(format!("CBOR decode failed: {e}")))?;

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

// Helper to convert AnyhowError to Trap
fn anyhow_to_trap(err: anyhow::Error) -> Trap {
    Trap::new(format!("Host function error: {}", err))
}

// Skeleton for host_job_get_id (WASM: "get_job_id")
async fn local_get_job_id(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    job_id_buf_ptr: u32,
    job_id_buf_len: u32,
) -> Result<i32, Trap> {
    // Example of how it would call the actual trait method:
    // let host_env = caller.data().clone();
    // host_env.host_job_get_id(caller, job_id_buf_ptr, job_id_buf_len).await.map_err(anyhow_to_trap)
    Err(Trap::new("Host function 'get_job_id' not yet implemented"))
}

// Skeleton for host_job_get_initial_input_cid (WASM: "host_job_get_initial_input_cid")
async fn local_host_job_get_initial_input_cid(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    cid_buf_ptr: u32,
    cid_buf_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_job_get_initial_input_cid' not yet implemented"))
}

// Skeleton for host_job_is_interactive (WASM: "host_job_is_interactive")
async fn local_host_job_is_interactive(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_job_is_interactive' not yet implemented"))
}

// Skeleton for host_workflow_get_current_stage_index (WASM: "host_workflow_get_current_stage_index")
async fn local_host_workflow_get_current_stage_index(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_workflow_get_current_stage_index' not yet implemented"))
}

// Skeleton for host_workflow_get_current_stage_id (WASM: "host_workflow_get_current_stage_id")
async fn local_host_workflow_get_current_stage_id(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    stage_id_buf_ptr: u32,
    stage_id_buf_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_workflow_get_current_stage_id' not yet implemented"))
}

// Skeleton for host_workflow_get_current_stage_input_cid (WASM: "host_workflow_get_current_stage_input_cid")
async fn local_host_workflow_get_current_stage_input_cid(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    input_key_ptr: u32,
    input_key_len: u32,
    cid_buf_ptr: u32,
    cid_buf_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_workflow_get_current_stage_input_cid' not yet implemented"))
}

// Skeleton for host_job_report_progress (WASM: "host_job_report_progress")
async fn local_host_job_report_progress(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    percentage: u32, // u8 in trait, u32 for Wasm
    status_message_ptr: u32,
    status_message_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_job_report_progress' not yet implemented"))
}

// Skeleton for host_workflow_complete_current_stage (WASM: "host_workflow_complete_current_stage")
async fn local_host_workflow_complete_current_stage(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    output_cid_ptr: u32,
    output_cid_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_workflow_complete_current_stage' not yet implemented"))
}

// Skeleton for host_interactive_send_output (WASM: "interactive_send")
async fn local_interactive_send(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    payload_ptr: u32,
    payload_len: u32,
    output_key_ptr: u32,
    output_key_len: u32,
    is_final_chunk: i32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'interactive_send' not yet implemented"))
}

// Skeleton for host_interactive_receive_input (WASM: "interactive_recv")
async fn local_interactive_recv(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    buffer_ptr: u32,
    buffer_len: u32,
    timeout_ms: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'interactive_recv' not yet implemented"))
}

// Skeleton for host_interactive_peek_input_len (WASM: "host_interactive_peek_input_len")
async fn local_host_interactive_peek_input_len(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_interactive_peek_input_len' not yet implemented"))
}

// Skeleton for host_interactive_prompt_for_input (WASM: "host_interactive_prompt_for_input")
async fn local_host_interactive_prompt_for_input(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    prompt_cid_ptr: u32,
    prompt_cid_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_interactive_prompt_for_input' not yet implemented"))
}

// Skeleton for host_data_read_cid (WASM: "read_data")
async fn local_read_data(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    cid_ptr: u32,
    cid_len: u32,
    offset: u64,
    buffer_ptr: u32,
    buffer_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'read_data' not yet implemented"))
}

// Skeleton for host_data_write_buffer (WASM: "anchor_data")
async fn local_anchor_data(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    data_ptr: u32,
    data_len: u32,
    cid_buf_ptr: u32,
    cid_buf_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'anchor_data' not yet implemented"))
}

// Skeleton for host_log_message (WASM: "log_message")
async fn local_log_message(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    level: u32, // LogLevel in trait (repr(u32)), u32 from Wasm
    message_ptr: u32,
    message_len: u32,
) -> Result<i32, Trap> {
    // In a real implementation, you might convert level to HostAbiLogLevel:
    // let _log_level = unsafe { std::mem::transmute::<u32, HostAbiLogLevel>(level) };
    Err(Trap::new("Host function 'log_message' not yet implemented"))
}

// Skeleton for host_submit_mesh_job (WASM: "host_submit_mesh_job")
async fn local_host_submit_mesh_job(
    mut caller: Caller<'_, ConcreteHostEnvironment>,
    job_data_ptr: u32,
    job_data_len: u32,
) -> Result<u64, Trap> {
    Err(Trap::new("Host function 'host_submit_mesh_job' not yet implemented"))
}

// --- Skeletons for functions from emit.rs blueprint not directly in MeshHostAbi ---

// Skeleton for begin_section (WASM: "begin_section")
async fn local_begin_section(
    mut _caller: Caller<'_, ConcreteHostEnvironment>,
    _name_ptr: u32,
    _name_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'begin_section' not yet implemented"))
}

// Skeleton for end_section (WASM: "end_section")
async fn local_end_section(
    mut _caller: Caller<'_, ConcreteHostEnvironment>,
    _name_ptr: u32,
    _name_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'end_section' not yet implemented"))
}

// Skeleton for create_proposal (WASM: "create_proposal")
async fn local_create_proposal(
    mut _caller: Caller<'_, ConcreteHostEnvironment>,
    _data_ptr: u32,
    _data_len: u32,
    _cid_buf_ptr: u32,
    _cid_buf_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'create_proposal' not yet implemented"))
}

// Skeleton for create_stage (WASM: "create_stage")
async fn local_create_stage(
    mut _caller: Caller<'_, ConcreteHostEnvironment>,
    _data_ptr: u32,
    _data_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'create_stage' not yet implemented"))
}

// Skeleton for set_property (WASM: "set_property")
async fn local_set_property(
    mut _caller: Caller<'_, ConcreteHostEnvironment>,
    _key_ptr: u32,
    _key_len: u32,
    _value_ptr: u32,
    _value_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'set_property' not yet implemented"))
}

// Skeleton for range_check (WASM: "range_check")
async fn local_range_check(
    mut _caller: Caller<'_, ConcreteHostEnvironment>,
    _value: i64,
    _min_val: i64,
    _max_val: i64,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'range_check' not yet implemented"))
}

// Skeleton for mint_token (WASM: "mint_token")
async fn local_mint_token(
    mut _caller: Caller<'_, ConcreteHostEnvironment>,
    _did_ptr: u32,
    _did_len: u32,
    _amount: u64,
    _token_type_ptr: u32,
    _token_type_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'mint_token' not yet implemented"))
}

// Skeleton for transfer_token (WASM: "transfer_token")
async fn local_transfer_token(
    mut _caller: Caller<'_, ConcreteHostEnvironment>,
    _sender_did_ptr: u32,
    _sender_did_len: u32,
    _recipient_did_ptr: u32,
    _recipient_did_len: u32,
    _amount: u64,
    _token_type_ptr: u32,
    _token_type_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'transfer_token' not yet implemented"))
}

// Skeleton for sleep_ms (WASM: "sleep_ms")
async fn local_sleep_ms(
    mut _caller: Caller<'_, ConcreteHostEnvironment>,
    _duration_ms: u32,
) -> Result<i32, Trap> { // Returning i32 for success status
    Err(Trap::new("Host function 'sleep_ms' not yet implemented"))
}

/// Register ICN host functions (legacy/full build).
pub fn register_host_functions(linker: &mut Linker<ConcreteHostEnvironment>) -> Result<()> {
    linker.func_wrap_async("icn", "host_anchor_receipt", host_anchor_receipt)?;
    linker.func_wrap_async("icn", "host_account_get_mana", host_account_get_mana)?;
    linker.func_wrap_async("icn", "host_account_spend_mana", host_account_spend_mana)?;

    linker.func_wrap_async("icn", "get_job_id", local_get_job_id)?;
    linker.func_wrap_async("icn", "host_job_get_initial_input_cid", local_host_job_get_initial_input_cid)?;
    linker.func_wrap_async("icn", "host_job_is_interactive", local_host_job_is_interactive)?;
    linker.func_wrap_async("icn", "host_workflow_get_current_stage_index", local_host_workflow_get_current_stage_index)?;
    linker.func_wrap_async("icn", "host_workflow_get_current_stage_id", local_host_workflow_get_current_stage_id)?;
    linker.func_wrap_async("icn", "host_workflow_get_current_stage_input_cid", local_host_workflow_get_current_stage_input_cid)?;
    linker.func_wrap_async("icn", "host_job_report_progress", local_host_job_report_progress)?;
    linker.func_wrap_async("icn", "host_workflow_complete_current_stage", local_host_workflow_complete_current_stage)?;
    linker.func_wrap_async("icn", "interactive_send", local_interactive_send)?;
    linker.func_wrap_async("icn", "interactive_recv", local_interactive_recv)?;
    linker.func_wrap_async("icn", "host_interactive_peek_input_len", local_host_interactive_peek_input_len)?;
    linker.func_wrap_async("icn", "host_interactive_prompt_for_input", local_host_interactive_prompt_for_input)?;
    linker.func_wrap_async("icn", "read_data", local_read_data)?;
    linker.func_wrap_async("icn", "anchor_data", local_anchor_data)?;
    linker.func_wrap_async("icn", "log_message", local_log_message)?;
    linker.func_wrap_async("icn", "host_submit_mesh_job", local_host_submit_mesh_job)?;

    linker.func_wrap_async("icn", "begin_section", local_begin_section)?;
    linker.func_wrap_async("icn", "end_section", local_end_section)?;
    linker.func_wrap_async("icn", "create_proposal", local_create_proposal)?;
    linker.func_wrap_async("icn", "create_stage", local_create_stage)?;
    linker.func_wrap_async("icn", "set_property", local_set_property)?;
    linker.func_wrap_async("icn", "range_check", local_range_check)?;
    linker.func_wrap_async("icn", "mint_token", local_mint_token)?;
    linker.func_wrap_async("icn", "transfer_token", local_transfer_token)?;
    linker.func_wrap_async("icn", "sleep_ms", local_sleep_ms)?;

    Ok(())
}
