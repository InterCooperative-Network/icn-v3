// Placeholder for the historical Wasmtime host-function glue table.
// The full code was moved out of the default build to unblock compilation.
// Enable with: `--features full_host_abi` in icn-runtime.

use crate::host_environment::{
    ConcreteHostEnvironment, get_memory, read_string_from_mem_ctx, // Import new helpers
    // read_bytes_from_mem_ctx, write_string_to_mem_ctx, write_bytes_to_mem_ctx // Import others if needed
};
use crate::job_execution_context::JobExecutionContext; // If needed for type context, unlikely here
use anyhow::{anyhow, Result};
use icn_identity::Did;
use std::str::FromStr;
use wasmtime::{Caller, Extern, Linker, AsContextMut, StoreContextMut, Memory as WasmtimeMemory, Trap};
use host_abi::{MeshHostAbi, LogLevel as HostAbiLogLevel, HostAbiError}; // Renamed LogLevel to avoid ambiguity with tracing::Level
use icn_types::mesh::MeshJobParams; // For host_submit_mesh_job potentially later

/// Minimal host_anchor_receipt implementation. Reads CBOR bytes from guest
/// memory, decodes an `ExecutionReceipt`, and calls `anchor_receipt` on the
/// host environment.  Returns `0` on success for now (CID return TBD).
async fn host_anchor_receipt(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    receipt_ptr: u32,
    receipt_len: u32,
) -> Result<(), Trap> {
    let memory = get_memory(&mut caller).map_err(|e| Trap::new(format!("get_memory failed for anchor_receipt: {}", e)))?;
    let mut store_context = caller.as_context_mut();

    let mut buf = vec![0u8; receipt_len as usize];
    memory
        .read(&mut store_context, receipt_ptr as usize, &mut buf)
        .map_err(|e| Trap::new(format!("memory read failed: {}", e)))?;
    
    let receipt: () = serde_cbor::from_slice(&buf).map_err(|e| Trap::new(format!("CBOR decode failed: {}", e)))?;
    caller.data().anchor_receipt(receipt).await.map_err(host_abi_error_to_trap)?;
    Ok(())
}

/// Get mana balance for a DID (0-length str = caller DID).
async fn host_account_get_mana(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>, // Mark caller as unused
    _did_ptr: u32, // Mark as unused
    _did_len: u32, // Mark as unused
) -> Result<i64, Trap> {
    // Stub implementation as per user summary
    Err(Trap::new("Deprecated: host_account_get_mana is no longer supported."))
}

/// Spend mana for a DID (0-length str = caller DID).
async fn host_account_spend_mana(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>, // Mark caller as unused
    _did_ptr: u32, // Mark as unused
    _did_len: u32, // Mark as unused
    _amount: u64, // Mark as unused
) -> Result<i32, Trap> {
    // Stub implementation as per user summary
    Err(Trap::new("Deprecated: host_account_spend_mana is no longer supported."))
}

// Helper to convert HostAbiError to Trap
fn host_abi_error_to_trap(err: HostAbiError) -> Trap {
    Trap::new(err.to_string()) // Already using Trap::new if Trap is in scope
}

// Skeleton for host_job_get_id (WASM: "get_job_id")
async fn local_get_job_id(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _job_id_buf_ptr: u32,
    _job_id_buf_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'get_job_id' not yet implemented"))
}

// Skeleton for host_job_get_initial_input_cid (WASM: "host_job_get_initial_input_cid")
async fn local_host_job_get_initial_input_cid(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _cid_buf_ptr: u32,
    _cid_buf_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_job_get_initial_input_cid' not yet implemented"))
}

// Skeleton for host_job_is_interactive (WASM: "host_job_is_interactive")
async fn local_host_job_is_interactive(_caller: Caller<'_, ConcreteHostEnvironment<()>>) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_job_is_interactive' not yet implemented"))
}

// Skeleton for host_workflow_get_current_stage_index (WASM: "host_workflow_get_current_stage_index")
async fn local_host_workflow_get_current_stage_index(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_workflow_get_current_stage_index' not yet implemented"))
}

// Skeleton for host_workflow_get_current_stage_id (WASM: "host_workflow_get_current_stage_id")
async fn local_host_workflow_get_current_stage_id(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _stage_id_buf_ptr: u32,
    _stage_id_buf_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_workflow_get_current_stage_id' not yet implemented"))
}

// Skeleton for host_workflow_get_current_stage_input_cid (WASM: "host_workflow_get_current_stage_input_cid")
async fn local_host_workflow_get_current_stage_input_cid(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _cid_buf_ptr: u32,
    _cid_buf_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_workflow_get_current_stage_input_cid' not yet implemented"))
}

// Skeleton for host_job_report_progress (WASM: "host_job_report_progress")
async fn local_host_job_report_progress(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _progress_percentage: u32,
    _status_msg_ptr: u32,
    _status_msg_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_job_report_progress' not yet implemented"))
}

// Skeleton for host_workflow_complete_current_stage (WASM: "host_workflow_complete_current_stage")
async fn local_host_workflow_complete_current_stage(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _output_cid_ptr: u32,
    _output_cid_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_workflow_complete_current_stage' not yet implemented"))
}

// Skeleton for host_interactive_send_output (WASM: "interactive_send")
async fn local_interactive_send(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _msg_ptr: u32,
    _msg_len: u32,
    _sequence_num: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'interactive_send' not yet implemented"))
}

// Skeleton for host_interactive_receive_input (WASM: "interactive_recv")
async fn local_interactive_recv(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _buffer_ptr: u32,
    _buffer_len: u32,
    _timeout_ms: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'interactive_recv' not yet implemented"))
}

// Skeleton for host_interactive_peek_input_len (WASM: "host_interactive_peek_input_len")
async fn local_host_interactive_peek_input_len(_caller: Caller<'_, ConcreteHostEnvironment<()>>) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_interactive_peek_input_len' not yet implemented"))
}

// Skeleton for host_interactive_prompt_for_input (WASM: "host_interactive_prompt_for_input")
async fn local_host_interactive_prompt_for_input(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _prompt_msg_ptr: u32,
    _prompt_msg_len: u32,
    _timeout_ms: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_interactive_prompt_for_input' not yet implemented"))
}

// Skeleton for host_data_read_cid (WASM: "read_data")
async fn local_read_data(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _data_id_ptr: u32,
    _data_id_len: u32,
    _buffer_ptr: u32,
    _buffer_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'read_data' not yet implemented"))
}

// Skeleton for host_data_write_buffer (WASM: "anchor_data")
async fn local_anchor_data(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _data_id_ptr: u32,
    _data_id_len: u32,
    _metadata_ptr: u32,
    _metadata_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'anchor_data' not yet implemented"))
}

// Skeleton for host_log_message (WASM: "log_message")
async fn local_log_message(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _level_val: i32, 
    _message_ptr: u32,
    _message_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'log_message' not yet implemented"))
}

// Skeleton for host_submit_mesh_job (WASM: "host_submit_mesh_job")
async fn local_host_submit_mesh_job_old(
    _caller: Caller<'_, ConcreteHostEnvironment<()>>,
    _job_params_cbor_ptr: u32,
    _job_params_cbor_len: u32,
    _job_id_buf_ptr: u32,
    _job_id_buf_len: u32,
) -> Result<i32, Trap> {
    Err(Trap::new("Host function 'host_submit_mesh_job' (old) not yet implemented"))
}

// --- New ABI Host Function Wrappers (MeshHostAbi) ---

async fn local_host_begin_section_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    kind_ptr: u32,
    kind_len: u32,
    title_ptr: u32,
    title_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_begin_section(caller.data(), caller, kind_ptr, kind_len, title_ptr, title_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_end_section_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_end_section(caller.data(), caller).await.map_err(host_abi_error_to_trap)
}

async fn local_host_set_property_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    key_ptr: u32,
    key_len: u32,
    value_json_ptr: u32,
    value_json_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_set_property(caller.data(), caller, key_ptr, key_len, value_json_ptr, value_json_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_anchor_data_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    path_ptr: u32,
    path_len: u32,
    data_ref_ptr: u32,
    data_ref_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_anchor_data(caller.data(), caller, path_ptr, path_len, data_ref_ptr, data_ref_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_generic_call_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    fn_name_ptr: u32,
    fn_name_len: u32,
    args_payload_ptr: u32,
    args_payload_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_generic_call(caller.data(), caller, fn_name_ptr, fn_name_len, args_payload_ptr, args_payload_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_create_proposal_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    id_ptr: u32,
    id_len: u32,
    title_ptr: u32,
    title_len: u32,
    version_ptr: u32,
    version_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_create_proposal(caller.data(), caller, id_ptr, id_len, title_ptr, title_len, version_ptr, version_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_mint_token_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    resource_type_ptr: u32,
    resource_type_len: u32,
    amount: u64, // Changed from i64 to u64 to match MeshHostAbi trait
    recipient_did_ptr: u32,
    recipient_did_len: u32,
    data_json_ptr: u32,
    data_json_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    // Ensure amount type matches trait (u64)
    MeshHostAbi::host_mint_token(caller.data(), caller, resource_type_ptr, resource_type_len, amount, recipient_did_ptr, recipient_did_len, data_json_ptr, data_json_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_if_condition_eval_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    condition_str_ptr: u32,
    condition_str_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_if_condition_eval(caller.data(), caller, condition_str_ptr, condition_str_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_else_handler_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_else_handler(caller.data(), caller).await.map_err(host_abi_error_to_trap)
}

async fn local_host_endif_handler_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_endif_handler(caller.data(), caller).await.map_err(host_abi_error_to_trap)
}

async fn local_host_log_todo_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    message_ptr: u32,
    message_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_log_todo(caller.data(), caller, message_ptr, message_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_on_event_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    event_name_ptr: u32,
    event_name_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_on_event(caller.data(), caller, event_name_ptr, event_name_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_log_debug_deprecated_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    message_ptr: u32,
    message_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_log_debug_deprecated(caller.data(), caller, message_ptr, message_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_range_check_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    value: i64, 
    min_val: i64, 
    max_val: i64,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_range_check(caller.data(), caller, value, min_val, max_val).await.map_err(host_abi_error_to_trap)
}

async fn local_host_use_resource_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    resource_type_ptr: u32,
    resource_type_len: u32,
    amount: u64, // Changed from i64 to u64 to match MeshHostAbi trait
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    // Ensure amount type matches trait (u64)
    MeshHostAbi::host_use_resource(caller.data(), caller, resource_type_ptr, resource_type_len, amount).await.map_err(host_abi_error_to_trap)
}

async fn local_host_transfer_token_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    token_type_ptr: u32,
    token_type_len: u32,
    amount: u64, // Changed from i64 to u64 to match MeshHostAbi trait
    sender_did_ptr: u32,
    sender_did_len: u32,
    recipient_did_ptr: u32,
    recipient_did_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    // Ensure amount type matches trait (u64)
    MeshHostAbi::host_transfer_token(caller.data(), caller, token_type_ptr, token_type_len, amount, sender_did_ptr, sender_did_len, recipient_did_ptr, recipient_did_len).await.map_err(host_abi_error_to_trap)
}

async fn local_host_submit_mesh_job_new(
    mut caller: Caller<'_, ConcreteHostEnvironment<()>>,
    cbor_payload_ptr: u32,
    cbor_payload_len: u32,
    job_id_buffer_ptr: u32,
    job_id_buffer_len: u32,
) -> Result<i32, Trap> {
    // Corrected call pattern for E0505
    MeshHostAbi::host_submit_mesh_job(caller.data(), caller, cbor_payload_ptr, cbor_payload_len, job_id_buffer_ptr, job_id_buffer_len).await.map_err(host_abi_error_to_trap)
}

/// Register ICN host functions (legacy/full build).
pub fn register_host_functions(linker: &mut Linker<ConcreteHostEnvironment<()>>) -> Result<()> {
    linker.func_wrap2_async("icn_host", "anchor_receipt", host_anchor_receipt)?;
    linker.func_wrap2_async("icn_host", "account_get_mana", host_account_get_mana)?;
    linker.func_wrap3_async("icn_host", "account_spend_mana", host_account_spend_mana)?;

    linker.func_wrap2_async("icn_host", "get_job_id", local_get_job_id)?;
    linker.func_wrap2_async("icn_host", "host_job_get_initial_input_cid", local_host_job_get_initial_input_cid)?;
    linker.func_wrap0_async("icn_host", "host_job_is_interactive", local_host_job_is_interactive)?;
    linker.func_wrap0_async("icn_host", "host_workflow_get_current_stage_index", local_host_workflow_get_current_stage_index)?;
    linker.func_wrap2_async("icn_host", "host_workflow_get_current_stage_id", local_host_workflow_get_current_stage_id)?;
    linker.func_wrap2_async("icn_host", "host_workflow_get_current_stage_input_cid", local_host_workflow_get_current_stage_input_cid)?;
    linker.func_wrap3_async("icn_host", "host_job_report_progress", local_host_job_report_progress)?;
    linker.func_wrap2_async("icn_host", "host_workflow_complete_current_stage", local_host_workflow_complete_current_stage)?;
    linker.func_wrap3_async("icn_host", "interactive_send", local_interactive_send)?;
    linker.func_wrap3_async("icn_host", "interactive_recv", local_interactive_recv)?;
    linker.func_wrap0_async("icn_host", "host_interactive_peek_input_len", local_host_interactive_peek_input_len)?;
    linker.func_wrap3_async("icn_host", "host_interactive_prompt_for_input", local_host_interactive_prompt_for_input)?;
    linker.func_wrap4_async("icn_host", "read_data", local_read_data)?;
    linker.func_wrap4_async("icn_host", "anchor_data", local_anchor_data)?;
    linker.func_wrap3_async("icn_host", "log_message", local_log_message)?;
    linker.func_wrap4_async("icn_host", "host_submit_mesh_job_old", local_host_submit_mesh_job_old)?;

    linker.func_wrap4_async("icn_host_new", "host_begin_section", |mut caller, k_ptr, k_len, t_ptr, t_len| Box::pin(local_host_begin_section_new(caller, k_ptr, k_len, t_ptr, t_len)))?;
    linker.func_wrap0_async("icn_host_new", "host_end_section", |mut caller| Box::pin(local_host_end_section_new(caller)))?;
    linker.func_wrap4_async("icn_host_new", "host_set_property", |mut caller, k_ptr, k_len, v_ptr, v_len| Box::pin(local_host_set_property_new(caller, k_ptr, k_len, v_ptr, v_len)))?;
    linker.func_wrap4_async("icn_host_new", "host_anchor_data", |mut caller, p_ptr, p_len, dr_ptr, dr_len| Box::pin(local_host_anchor_data_new(caller, p_ptr, p_len, dr_ptr, dr_len)))?;
    linker.func_wrap4_async("icn_host_new", "host_generic_call", |mut caller, fn_ptr, fn_len, ap_ptr, ap_len| Box::pin(local_host_generic_call_new(caller, fn_ptr, fn_len, ap_ptr, ap_len)))?;
    linker.func_wrap6_async("icn_host_new", "host_create_proposal", |mut caller, id_ptr, id_len, t_ptr, t_len, v_ptr, v_len| Box::pin(local_host_create_proposal_new(caller, id_ptr, id_len, t_ptr, t_len, v_ptr, v_len)))?;
    linker.func_wrap7_async("icn_host_new", "host_mint_token", |mut caller, rt_ptr, rt_len, amt, recip_ptr, recip_len, dj_ptr, dj_len| Box::pin(local_host_mint_token_new(caller, rt_ptr, rt_len, amt, recip_ptr, recip_len, dj_ptr, dj_len)))?;
    linker.func_wrap2_async("icn_host_new", "host_if_condition_eval", |mut caller, cond_ptr, cond_len| Box::pin(local_host_if_condition_eval_new(caller, cond_ptr, cond_len)))?;
    linker.func_wrap0_async("icn_host_new", "host_else_handler", |mut caller| Box::pin(local_host_else_handler_new(caller)))?;
    linker.func_wrap0_async("icn_host_new", "host_endif_handler", |mut caller| Box::pin(local_host_endif_handler_new(caller)))?;
    linker.func_wrap2_async("icn_host_new", "host_log_todo", |mut caller, msg_ptr, msg_len| Box::pin(local_host_log_todo_new(caller, msg_ptr, msg_len)))?;
    linker.func_wrap2_async("icn_host_new", "host_on_event", |mut caller, ev_ptr, ev_len| Box::pin(local_host_on_event_new(caller, ev_ptr, ev_len)))?;
    linker.func_wrap2_async("icn_host_new", "host_log_debug_deprecated", |mut caller, msg_ptr, msg_len| Box::pin(local_host_log_debug_deprecated_new(caller, msg_ptr, msg_len)))?;
    linker.func_wrap3_async("icn_host_new", "host_range_check", |mut caller, val, min, max| Box::pin(local_host_range_check_new(caller, val, min, max)))?;
    linker.func_wrap3_async("icn_host_new", "host_use_resource", |mut caller, rt_ptr, rt_len, amt| Box::pin(local_host_use_resource_new(caller, rt_ptr, rt_len, amt)))?;
    linker.func_wrap7_async("icn_host_new", "host_transfer_token", |mut caller, tt_ptr, tt_len, amt, s_ptr, s_len, r_ptr, r_len| Box::pin(local_host_transfer_token_new(caller, tt_ptr, tt_len, amt, s_ptr, s_len, r_ptr, r_len)))?;
    linker.func_wrap4_async("icn_host_new", "host_submit_mesh_job", |mut caller, payload_ptr, payload_len, jid_buf_ptr, jid_buf_len| Box::pin(local_host_submit_mesh_job_new(caller, payload_ptr, payload_len, jid_buf_ptr, jid_buf_len)))?;
    
    Ok(())
}
