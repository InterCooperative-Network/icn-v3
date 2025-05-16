use crate::context::RuntimeContext;
use crate::job_execution_context::JobExecutionContext;
use anyhow::{anyhow, Result};
use icn_economics::ResourceType;
use icn_identity::{Did, ScopeKey};
use host_abi::{
    HostAbiError, MeshHostAbi,
};
use icn_types::org::{CommunityId, CooperativeId};
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::{Caller, Extern, Memory as WasmtimeMemory};
use std::marker::PhantomData;
// use icn_actor_interfaces::actor_runtime::HostcallWasmError; // Temporarily commented out
// use icn_actor_interfaces::Timestamp; // Temporarily commented out
// use icn_dag_scheduler::commit::DagCommitAddress; // Temporarily commented out
// use icn_dag_scheduler::protocol::JobId; // Temporarily commented out
// use icn_stable_memory_wasm::StableMemoryError; // Temporarily commented out
// use icn_types::error::DagError; // Removed unused import

/// Concrete implementation of the host environment for WASM execution
#[derive(Clone)]
pub struct ConcreteHostEnvironment<T: Send + Sync + 'static> {
    pub ctx: Arc<Mutex<JobExecutionContext>>,
    pub rt: Arc<RuntimeContext>,
    pub caller_did: Did,
    pub is_governance: bool,
    pub coop_id: Option<CooperativeId>,
    pub community_id: Option<CommunityId>,
    _phantom: PhantomData<T>,
}

impl<T: Send + Sync + 'static> ConcreteHostEnvironment<T> {
    pub fn new(
        ctx: Arc<Mutex<JobExecutionContext>>,
        caller_did: Did,
        runtime_ctx: Arc<RuntimeContext>,
    ) -> Self {
        Self {
            ctx,
            rt: runtime_ctx,
            caller_did,
            is_governance: false,
            coop_id: None,
            community_id: None,
            _phantom: PhantomData,
        }
    }

    pub fn new_governance(
        ctx: Arc<Mutex<JobExecutionContext>>,
        caller_did: Did,
        runtime_ctx: Arc<RuntimeContext>,
    ) -> Self {
        Self {
            ctx,
            rt: runtime_ctx,
            caller_did,
            is_governance: true,
            coop_id: None,
            community_id: None,
            _phantom: PhantomData,
        }
    }

    pub fn with_organization(
        mut self,
        coop_id: Option<CooperativeId>,
        community_id: Option<CommunityId>,
    ) -> Self {
        self.coop_id = coop_id;
        self.community_id = community_id;
        self
    }

    /// Determine the accounting scope key for mana operations.
    #[allow(dead_code)] // TODO: This is likely used by mana hostcalls once fully implemented.
    fn scope_key(&self) -> ScopeKey {
        // 1) If explicit coop/community overrides exist, honour them first.
        if let Some(coop) = &self.coop_id {
            ScopeKey::Cooperative(coop.to_string())
        } else if let Some(comm) = &self.community_id {
            ScopeKey::Community(comm.to_string())
        } else if let Some(index) = self.rt.identity_index.as_ref() {
            // Pass the caller_did by reference
            index.resolve_scope_key(&self.caller_did)
        } else if let Some(fid) = self.rt.federation_id.as_ref() {
            // Fallback to federation scope if runtime context specifies it explicitly
            ScopeKey::Federation(fid.clone())
        } else {
            ScopeKey::Individual(self.caller_did.to_string())
        }
    }

    pub fn check_resource_authorization(&self, _rt_type: ResourceType, _amt: u64) -> i32 {
        // TODO: Implement actual resource authorization logic
        HostAbiError::NotSupported as i32
    }
    pub async fn record_resource_usage(&self, _rt_type: ResourceType, _amt: u64) -> i32 {
        HostAbiError::NotSupported as i32
    }
    pub fn is_governance_context(&self) -> i32 {
        if self.is_governance {
            1
        } else {
            0
        }
    }
    pub async fn mint_token(&self, _recipient_did_str: &str, _amount: u64) -> i32 {
        HostAbiError::NotSupported as i32
    }
    pub async fn transfer_token(
        &self,
        _sender_did_str: &str,
        _recipient_did_str: &str,
        _amount: u64,
    ) -> i32 {
        HostAbiError::NotSupported as i32
    }

    /// Anchor a signed execution receipt to the DAG and broadcast an announcement.
    pub async fn anchor_receipt(&self, _receipt: ()) -> Result<(), ()> {
        // Placeholder implementation. In a real scenario, this would interact with
        // the DAG store, potentially via the RuntimeContext or a dedicated service.
        Ok(())
    }

    // ---------------------- Helper memory access methods ----------------------

    /// Helper to safely obtain the linear memory exported by the guest module.
    pub fn get_memory(
        &self,
        caller: &mut Caller<'_, T>,
    ) -> Result<WasmtimeMemory, anyhow::Error> {
        match caller.get_export("memory") {
            Some(Extern::Memory(mem)) => Ok(mem),
            _ => Err(anyhow!(HostAbiError::MemoryAccessError)),
        }
    }

    /// Read a UTF-8 string from guest memory at (ptr,len).
    pub fn read_string_from_mem(
        &self,
        caller: &mut Caller<'_, T>,
        ptr: u32,
        len: u32,
    ) -> Result<String, anyhow::Error> {
        let mem = self.get_memory(caller)?;
        let mut buffer = vec![0u8; len as usize];
        mem.read(caller, ptr as usize, &mut buffer)
            .map_err(|_| anyhow!(HostAbiError::MemoryAccessError))?;
        String::from_utf8(buffer).map_err(|_| anyhow!(HostAbiError::DataEncodingError))
    }

    /// Write a UTF-8 string `s` into guest memory buffer (ptr,len).
    pub fn write_string_to_mem(
        &self,
        caller: &mut Caller<'_, T>,
        s: &str,
        ptr: u32,
        len: u32,
    ) -> Result<i32, anyhow::Error> {
        let bytes = s.as_bytes();
        if bytes.len() > len as usize {
            return Err(anyhow!(HostAbiError::BufferTooSmall));
        }
        let mem = self.get_memory(caller)?;
        mem.write(caller, ptr as usize, bytes)
            .map_err(|_| anyhow!(HostAbiError::MemoryAccessError))?;
        Ok(bytes.len() as i32)
    }

    /// Read a raw byte slice from guest memory.
    pub fn read_bytes_from_mem(
        &self,
        caller: &mut Caller<'_, T>,
        ptr: u32,
        len: u32,
    ) -> Result<Vec<u8>, anyhow::Error> {
        let mem = self.get_memory(caller)?;
        let mut buffer = vec![0u8; len as usize];
        mem.read(caller, ptr as usize, &mut buffer)
            .map_err(|_| anyhow!(HostAbiError::MemoryAccessError))?;
        Ok(buffer)
    }

    /// Write raw bytes to guest memory buffer (ptr,len).
    pub fn write_bytes_to_mem(
        &self,
        caller: &mut Caller<'_, T>,
        bytes: &[u8],
        ptr: u32,
        len: u32,
    ) -> Result<i32, anyhow::Error> {
        if bytes.len() > len as usize {
            return Err(anyhow!(HostAbiError::BufferTooSmall));
        }
        let mem = self.get_memory(caller)?;
        mem.write(caller, ptr as usize, bytes)
            .map_err(|_| anyhow!(HostAbiError::MemoryAccessError))?;
        Ok(bytes.len() as i32)
    }
}

#[async_trait::async_trait]
impl<T: Send + Sync + 'static> MeshHostAbi<T> for ConcreteHostEnvironment<T> {
    async fn host_begin_section(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        kind_ptr: u32,
        kind_len: u32,
        title_ptr: u32,
        title_len: u32,
    ) -> Result<i32, HostAbiError> {
        let kind = self.read_string_from_mem(&mut caller, kind_ptr, kind_len)?;
        let title = if title_len > 0 {
            Some(self.read_string_from_mem(&mut caller, title_ptr, title_len)?)
        } else {
            None
        };
        let mut ctx = self.ctx.lock().await;
        ctx.begin_section(kind, title)?;
        Ok(0)
    }

    async fn host_end_section(
        &self,
        _caller: wasmtime::Caller<'_, T>,
    ) -> Result<i32, HostAbiError> {
        let mut ctx = self.ctx.lock().await;
        ctx.end_section()?;
        Ok(0)
    }

    async fn host_set_property(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        key_ptr: u32,
        key_len: u32,
        value_json_ptr: u32,
        value_json_len: u32,
    ) -> Result<i32, HostAbiError> {
        let key = self.read_string_from_mem(&mut caller, key_ptr, key_len)?;
        let value_json = self.read_string_from_mem(&mut caller, value_json_ptr, value_json_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.set_property(key, value_json)?;
        Ok(0)
    }

    async fn host_anchor_data(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        path_ptr: u32,
        path_len: u32,
        data_ref_ptr: u32,
        data_ref_len: u32,
    ) -> Result<i32, HostAbiError> {
        let path = self.read_string_from_mem(&mut caller, path_ptr, path_len)?;
        let data_ref = self.read_string_from_mem(&mut caller, data_ref_ptr, data_ref_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.anchor_data(path, data_ref)?;
        Ok(0)
    }

    async fn host_generic_call(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        fn_name_ptr: u32,
        fn_name_len: u32,
        args_payload_ptr: u32,
        args_payload_len: u32,
    ) -> Result<i32, HostAbiError> {
        let fn_name = self.read_string_from_mem(&mut caller, fn_name_ptr, fn_name_len)?;
        let args_payload = self.read_string_from_mem(&mut caller, args_payload_ptr, args_payload_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.generic_call(fn_name, args_payload)?;
        Ok(0)
    }

    async fn host_create_proposal(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        id_ptr: u32,
        id_len: u32,
        title_ptr: u32,
        title_len: u32,
        version_ptr: u32,
        version_len: u32,
    ) -> Result<i32, HostAbiError> {
        let id = self.read_string_from_mem(&mut caller, id_ptr, id_len)?;
        let title = self.read_string_from_mem(&mut caller, title_ptr, title_len)?;
        let version = self.read_string_from_mem(&mut caller, version_ptr, version_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.create_proposal(id, title, version)?;
        Ok(0)
    }

    async fn host_mint_token(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        res_type_ptr: u32,
        res_type_len: u32,
        amount: i64,
        recip_ptr: u32,
        recip_len: u32,
        data_json_ptr: u32,
        data_json_len: u32,
    ) -> Result<i32, HostAbiError> {
        let res_type = self.read_string_from_mem(&mut caller, res_type_ptr, res_type_len)?;
        let recipient = if recip_len > 0 {
            Some(self.read_string_from_mem(&mut caller, recip_ptr, recip_len)?)
        } else {
            None
        };
        let data_json = if data_json_len > 0 {
            Some(self.read_string_from_mem(&mut caller, data_json_ptr, data_json_len)?)
        } else {
            None
        };
        let mut ctx = self.ctx.lock().await;
        ctx.mint_token(res_type, amount, recipient, data_json)?;
        Ok(0)
    }

    async fn host_if_condition_eval(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        condition_str_ptr: u32,
        condition_str_len: u32,
    ) -> Result<i32, HostAbiError> {
        let condition_str = self.read_string_from_mem(&mut caller, condition_str_ptr, condition_str_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.if_condition_eval(condition_str)?;
        Ok(0) // Host evaluation controls flow, no direct bool return to WASM per spec
    }

    async fn host_else_handler(
        &self,
        _caller: wasmtime::Caller<'_, T>,
    ) -> Result<i32, HostAbiError> {
        let mut ctx = self.ctx.lock().await;
        ctx.else_handler()?;
        Ok(0)
    }

    async fn host_endif_handler(
        &self,
        _caller: wasmtime::Caller<'_, T>,
    ) -> Result<i32, HostAbiError> {
        let mut ctx = self.ctx.lock().await;
        ctx.endif_handler()?;
        Ok(0)
    }

    async fn host_log_todo(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        msg_ptr: u32,
        msg_len: u32,
    ) -> Result<i32, HostAbiError> {
        let msg = self.read_string_from_mem(&mut caller, msg_ptr, msg_len)?;
        // Assuming a logging mechanism on ConcreteHostEnvironment or globally available
        // e.g., self.logger.log_todo(msg);
        tracing::warn!("[TODO FROM WASM]: {}", msg); // Or self.ctx.log_todo(msg)
        Ok(0)
    }

    async fn host_on_event(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        event_ptr: u32,
        event_len: u32,
    ) -> Result<i32, HostAbiError> {
        let event_name = self.read_string_from_mem(&mut caller, event_ptr, event_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.on_event(event_name)?;
        Ok(0)
    }

    async fn host_log_debug_deprecated(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        msg_ptr: u32,
        msg_len: u32,
    ) -> Result<i32, HostAbiError> {
        let msg = self.read_string_from_mem(&mut caller, msg_ptr, msg_len)?;
        tracing::debug!("[DEBUG_DEPRECATED FROM WASM]: {}", msg);
        Ok(0)
    }

    async fn host_range_check(
        &self,
        _caller: wasmtime::Caller<'_, T>,
        start_val: f64,
        end_val: f64,
    ) -> Result<i32, HostAbiError> {
        let mut ctx = self.ctx.lock().await;
        ctx.range_check(start_val, end_val)?;
        Ok(0)
    }

    async fn host_use_resource(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        resource_type_ptr: u32,
        resource_type_len: u32,
        amount: i64,
    ) -> Result<i32, HostAbiError> {
        let resource_type = self.read_string_from_mem(&mut caller, resource_type_ptr, resource_type_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.use_resource(resource_type, amount)?;
        Ok(0)
    }

    async fn host_transfer_token(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        token_type_ptr: u32,
        token_type_len: u32,
        amount: i64,
        sender_ptr: u32,
        sender_len: u32,
        recipient_ptr: u32,
        recipient_len: u32,
    ) -> Result<i32, HostAbiError> {
        let token_type = self.read_string_from_mem(&mut caller, token_type_ptr, token_type_len)?;
        let sender = if sender_len > 0 {
            Some(self.read_string_from_mem(&mut caller, sender_ptr, sender_len)?)
        } else {
            None
        };
        let recipient = self.read_string_from_mem(&mut caller, recipient_ptr, recipient_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.transfer_token(token_type, amount, sender, recipient)?;
        Ok(0)
    }

    async fn host_submit_mesh_job(
        &self,
        mut caller: wasmtime::Caller<'_, T>,
        cbor_payload_ptr: u32,
        cbor_payload_len: u32,
        job_id_buffer_ptr: u32,
        job_id_buffer_len: u32,
    ) -> Result<i32, HostAbiError> {
        let cbor_payload = self.read_bytes_from_mem(&mut caller, cbor_payload_ptr, cbor_payload_len)?;
        let mut ctx = self.ctx.lock().await;
        // The context method needs to be able to write back to WASM memory for the job_id
        // This might involve passing the memory, caller, or a specific write helper to it.
        let job_id_len = ctx.submit_mesh_job(
            cbor_payload, 
            // A way to write back:
            |job_id_str| self.write_string_to_mem(&mut caller, job_id_buffer_ptr, job_id_buffer_len, job_id_str)
        )?;
        Ok(job_id_len as i32)
    }
}

#[cfg(test)]
impl<T: Send + Sync + 'static> ConcreteHostEnvironment<T> {
    pub async fn test_host_account_get_mana(
        &self,
        did: &Did, // The DID whose mana is being fetched
    ) -> Result<i64, HostAbiError> {
        // rt is Arc<RuntimeContext<InMemoryManaLedger>>
        // The test shim ConcreteHostEnvironment uses InMemoryManaLedger
        match self.rt.mana_repository().get_mana_state(did).await {
            Ok(Some(state)) => Ok(state.current_mana as i64),
            Ok(None) => Ok(0), // Or perhaps an error if DID not found / no state
            Err(e) => {
                eprintln!("Test shim get_mana_state error: {:?}", e);
                Err(HostAbiError::StorageError)
            }
        }
    }

    pub async fn test_host_account_spend_mana(
        &self,
        did: &Did, // The DID of the account to spend from
        amount: u64,
    ) -> Result<i32, HostAbiError> {
        // Determine scope based on the ConcreteHostEnvironment's context
        let scope_key = self.scope_key(); // Uses self.coop_id, self.community_id, self.caller_did
        let scope_str = match scope_key {
            ScopeKey::Individual(id) => id, // Assumes Did::to_string() was used, includes "did:key:"
            ScopeKey::Cooperative(id) => format!("coop:{}", id),
            ScopeKey::Community(id) => format!("comm:{}", id),
            ScopeKey::Federation(id) => format!("fed:{}", id),
        };

        let token = ScopedResourceToken {
            resource_type: "mana".to_string(),
            scope: scope_str, // This scope is for policy rules (e.g. coop X can spend Y)
            amount: amount,
            expires_at: None,
            issuer: None,
        };

        // The 'did' parameter here is WHO is attempting the spend.
        // The 'token.scope' is ABOUT WHAT SCOPE the policy applies to.
        if let Err(e) = self.rt.policy_enforcer().check_authorization(did, &token).await {
            eprintln!("Test shim policy check error: {:?}", e);
            // TODO: Refine error mapping from policy error to HostAbiError
            return Err(HostAbiError::ResourceLimitExceeded);
        }

        match self.rt.mana_repository().spend_mana(did, amount).await {
            Ok(_) => Ok(0),
            Err(e) => {
                if let Some(mana_err) = e.downcast_ref::<ManaError>() {
                    match mana_err {
                        ManaError::InsufficientMana { .. } => {
                            return Err(HostAbiError::InsufficientBalance);
                        }
                        _ => {
                            eprintln!("Test shim spend_mana other ManaError: {:?}", mana_err);
                        }
                    }
                } else {
                    eprintln!("Test shim spend_mana unknown error: {:?}", e);
                }
                // Fallback for other ManaErrors or if downcast fails
                Err(HostAbiError::StorageError)
            }
        }
    }
}
