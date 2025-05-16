use crate::context::RuntimeContext;
use crate::job_execution_context::JobExecutionContext;
use anyhow::{anyhow, Result};
use icn_economics::{ResourceType, ResourceRepository, ScopedResourceToken};
use icn_identity::{Did, ScopeKey};
use host_abi::{
    HostAbiError, MeshHostAbi,
};
use icn_types::org::{CommunityId, CooperativeId};
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::{Caller, Extern, Memory as WasmtimeMemory, AsContextMut, StoreContextMut};
use std::marker::PhantomData;
use std::str::FromStr;
// use icn_actor_interfaces::actor_runtime::HostcallWasmError; // Temporarily commented out
// use icn_actor_interfaces::Timestamp; // Temporarily commented out
// use icn_dag_scheduler::commit::DagCommitAddress; // Temporarily commented out
// use icn_dag_scheduler::protocol::JobId; // Temporarily commented out
// use icn_stable_memory_wasm::StableMemoryError; // Temporarily commented out
// use icn_types::error::DagError; // Removed unused import

#[cfg(test)]
use icn_economics::{
    mana::{ManaError, ManaLedger}, // ManaLedger might not be used directly but good for context
    PolicyEnforcer, // Corrected path as per linter suggestion
    // ScopedResourceToken,    // Struct for tokens // Removed duplicate
    // ResourceRepository,   // Trait for get_usage, record_usage // Removed duplicate
};

/// Concrete implementation of the host environment for WASM execution
#[derive(Clone)]
pub struct ConcreteHostEnvironment<T_param: Send + Sync + 'static> {
    pub ctx: Arc<Mutex<JobExecutionContext>>,
    pub rt: Arc<RuntimeContext>,
    pub caller_did: Did,
    pub is_governance: bool,
    pub coop_id: Option<CooperativeId>,
    pub community_id: Option<CommunityId>,
    _phantom: PhantomData<T_param>,
}

impl<T_param: Send + Sync + 'static> ConcreteHostEnvironment<T_param> {
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

    pub fn new_with_context(ctx: JobExecutionContext) -> Self {
        // This constructor is simplified for testing. A real scenario might need more
        // context like RuntimeContext, caller_did, etc. For ABI tests focusing on JEC
        // interaction, this should suffice.
        // We'll need a dummy RuntimeContext and Did for now.
        use std::str::FromStr; // Ensure FromStr is in scope for Did::from_str if still needed here
        let dummy_did = icn_identity::Did::from_str("did:icn:test_caller").expect("Failed to create dummy DID");
        
        // Adjust to specify the ManaLedger type if minimal_for_testing is generic
        let dummy_runtime_ctx = Arc::new(crate::context::RuntimeContext::<icn_economics::mana::InMemoryManaLedger>::minimal_for_testing());

        Self {
            ctx: Arc::new(Mutex::new(ctx)),
            rt: dummy_runtime_ctx,
            caller_did: dummy_did,
            is_governance: false,
            coop_id: None,
            community_id: None,
            _phantom: PhantomData::<T_param>,
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
    pub fn scope_key(&self) -> ScopeKey {
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

    pub fn check_resource_authorization(&self, _rt_type: ResourceType, _amt: u64) -> Result<i32, HostAbiError> {
        // TODO: Implement actual resource authorization logic
        Err(HostAbiError::NotSupported)
    }
    pub async fn record_resource_usage(&self, _rt_type: ResourceType, _amt: u64) -> Result<i32, HostAbiError> {
        Err(HostAbiError::NotSupported)
    }
    pub fn is_governance_context(&self) -> i32 {
        if self.is_governance {
            1
        } else {
            0
        }
    }
    pub async fn mint_token(&self, _recipient_did_str: &str, _amount: u64) -> Result<i32, HostAbiError> {
        Err(HostAbiError::NotSupported)
    }
    pub async fn transfer_token(&self, _sender_did_str: &str, _recipient_did_str: &str, _amount: u64) -> Result<i32, HostAbiError> {
        Err(HostAbiError::NotSupported)
    }

    /// Anchor a signed execution receipt to the DAG and broadcast an announcement.
    pub async fn anchor_receipt(&self, _receipt: ()) -> Result<(), HostAbiError> {
        // Placeholder implementation. In a real scenario, this would interact with
        // the DAG store, potentially via the RuntimeContext or a dedicated service.
        // For now, assume success or a generic error if it were to fail.
        // If this were to call something fallible: 
        // some_fallible_operation().await.map_err(|e| HostAbiError::UnknownError(e.to_string()))
        Ok(())
    }

    // ---------------------- Helper memory access methods ----------------------

    // REMOVED HELPERS FROM HERE
}

// --- Standalone Helper memory access functions ---

    /// Helper to safely obtain the linear memory exported by the guest module.
pub fn get_memory<T_param: Send + Sync + 'static>(
    caller: &mut Caller<'_, ConcreteHostEnvironment<T_param>>,
) -> Result<WasmtimeMemory, HostAbiError> {
        match caller.get_export("memory") {
            Some(Extern::Memory(mem)) => Ok(mem),
        _ => Err(HostAbiError::MemoryAccessError("Memory export not found".to_string())),
    }
}

/// Helper to read a string from WASM memory, using StoreContextMut and pre-fetched Memory
pub fn read_string_from_mem_ctx<T_param: Send + Sync + 'static>(
    store_ctx: &mut StoreContextMut<'_, ConcreteHostEnvironment<T_param>>,
    memory: &WasmtimeMemory,
        ptr: u32,
        len: u32,
) -> Result<String, HostAbiError> {
        let mut buffer = vec![0u8; len as usize];
    memory.read(store_ctx, ptr as usize, &mut buffer)
        .map_err(|e| HostAbiError::MemoryAccessError(format!("Memory read failed: {}", e)))?;
    String::from_utf8(buffer)
        .map_err(|e| HostAbiError::DataEncodingError(format!("UTF-8 conversion failed: {}", e)))
}

/// Write a UTF-8 string `s` into guest memory buffer, using StoreContextMut and pre-fetched Memory
pub fn write_string_to_mem_ctx<T_param: Send + Sync + 'static>(
    store_ctx: &mut StoreContextMut<'_, ConcreteHostEnvironment<T_param>>,
    memory: &WasmtimeMemory,
        s: &str,
        ptr: u32,
        len: u32,
) -> Result<i32, HostAbiError> {
        let bytes = s.as_bytes();
        if bytes.len() > len as usize {
        return Err(HostAbiError::BufferTooSmall("String too large for buffer".to_string()));
        }
    memory.write(store_ctx, ptr as usize, bytes)
        .map_err(|e| HostAbiError::MemoryAccessError(format!("Memory write failed: {}", e)))?;
        Ok(bytes.len() as i32)
    }

/// Read a raw byte slice from guest memory, using StoreContextMut and pre-fetched Memory
pub fn read_bytes_from_mem_ctx<T_param: Send + Sync + 'static>(
    store_ctx: &mut StoreContextMut<'_, ConcreteHostEnvironment<T_param>>,
    memory: &WasmtimeMemory,
        ptr: u32,
        len: u32,
) -> Result<Vec<u8>, HostAbiError> {
        let mut buffer = vec![0u8; len as usize];
    memory.read(store_ctx, ptr as usize, &mut buffer)
        .map_err(|e| HostAbiError::MemoryAccessError(format!("Memory read failed: {}", e)))?;
        Ok(buffer)
    }

/// Write raw bytes to guest memory buffer, using StoreContextMut and pre-fetched Memory
pub fn write_bytes_to_mem_ctx<T_param: Send + Sync + 'static>(
    store_ctx: &mut StoreContextMut<'_, ConcreteHostEnvironment<T_param>>,
    memory: &WasmtimeMemory,
        bytes: &[u8],
        ptr: u32,
        len: u32,
) -> Result<i32, HostAbiError> {
        if bytes.len() > len as usize {
        return Err(HostAbiError::BufferTooSmall("Bytes too large for buffer".to_string()));
    }
    memory.write(store_ctx, ptr as usize, bytes)
        .map_err(|e| HostAbiError::MemoryAccessError(format!("Memory write failed: {}", e)))?;
    Ok(bytes.len() as i32)
}

#[async_trait::async_trait]
impl<T_param: Send + Sync + 'static> MeshHostAbi<ConcreteHostEnvironment<T_param>> for ConcreteHostEnvironment<T_param> {
    async fn host_begin_section(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        kind_ptr: u32,
        kind_len: u32,
        title_ptr: u32,
        title_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let kind = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, kind_ptr, kind_len)?;
        let title = if title_len > 0 {
            Some(read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, title_ptr, title_len)?)
        } else {
            None
        };
        let mut ctx = self.ctx.lock().await;
        ctx.begin_section(kind, title)?;
        Ok(0)
    }

    async fn host_end_section(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
    ) -> Result<i32, HostAbiError> {
        let mut ctx = self.ctx.lock().await;
        ctx.end_section()?;
        Ok(0)
    }

    async fn host_set_property(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        key_ptr: u32,
        key_len: u32,
        value_json_ptr: u32,
        value_json_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let key = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, key_ptr, key_len)?;
        let value_json = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, value_json_ptr, value_json_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.set_property(key, value_json)?;
        Ok(0)
    }

    async fn host_anchor_data(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        path_ptr: u32,
        path_len: u32,
        data_ref_ptr: u32,
        data_ref_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let path = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, path_ptr, path_len)?;
        let data_ref = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, data_ref_ptr, data_ref_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.anchor_data(path, data_ref)?;
        Ok(0)
    }

    async fn host_generic_call(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        fn_name_ptr: u32,
        fn_name_len: u32,
        args_payload_ptr: u32,
        args_payload_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let fn_name = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, fn_name_ptr, fn_name_len)?;
        let args_payload = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, args_payload_ptr, args_payload_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.generic_call(fn_name, args_payload)?;
        Ok(0)
    }

    async fn host_create_proposal(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        id_ptr: u32,
        id_len: u32,
        title_ptr: u32,
        title_len: u32,
        version_ptr: u32,
        version_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let id = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, id_ptr, id_len)?;
        let title = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, title_ptr, title_len)?;
        let version = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, version_ptr, version_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.create_proposal(id, title, version)?;
        Ok(0)
    }

    async fn host_mint_token(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        resource_type_ptr: u32,
        resource_type_len: u32,
        amount: u64,
        recipient_did_ptr: u32,
        recipient_did_len: u32,
        data_json_ptr: u32,
        data_json_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let resource_type_str = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, resource_type_ptr, resource_type_len)?;
        let recipient_did_str = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, recipient_did_ptr, recipient_did_len)?;
        let data_json = if data_json_len > 0 {
            Some(read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, data_json_ptr, data_json_len)?)
        } else {
            None
        };
        let recipient_did = Did::from_str(&recipient_did_str)
            .map_err(|e| HostAbiError::InvalidDid(format!("Invalid recipient DID: {}, error: {}", recipient_did_str, e)))?;
        let mut ctx = self.ctx.lock().await;
        ctx.mint_token(resource_type_str, amount as i64, Some(recipient_did.to_string()), data_json)?;
        Ok(0)
    }

    async fn host_if_condition_eval(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        condition_str_ptr: u32,
        condition_str_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let condition_str = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, condition_str_ptr, condition_str_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.if_condition_eval(condition_str)?;
        Ok(0)
    }

    async fn host_else_handler(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
    ) -> Result<i32, HostAbiError> {
        let mut ctx = self.ctx.lock().await;
        ctx.else_handler()?;
        Ok(0)
    }

    async fn host_endif_handler(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
    ) -> Result<i32, HostAbiError> {
        let mut ctx = self.ctx.lock().await;
        ctx.endif_handler()?;
        Ok(0)
    }

    async fn host_log_todo(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        msg_ptr: u32,
        msg_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let msg = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, msg_ptr, msg_len)?;
        tracing::warn!("[TODO FROM WASM]: {}", msg);
        Ok(0)
    }

    async fn host_on_event(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        event_ptr: u32,
        event_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let event_name = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, event_ptr, event_len)?;
        let mut ctx = self.ctx.lock().await;
        ctx.on_event(event_name)?;
        Ok(0)
    }

    async fn host_log_debug_deprecated(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        message_ptr: u32,
        message_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let message = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, message_ptr, message_len)?;
        eprintln!("[HOST ABI DEBUG DEPRECATED]: {}", message);
        Ok(0)
    }

    async fn host_range_check(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        value: i64,
        min_val: i64,
        max_val: i64,
    ) -> Result<i32, HostAbiError> {
        if value >= min_val && value <= max_val {
            Ok(0)
        } else {
            Err(HostAbiError::InvalidParameter(format!(
                "Range check failed: value {} not in [{}, {}]",
                value, min_val, max_val
            )))
        }
    }

    async fn host_use_resource(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        resource_type_ptr: u32,
        resource_type_len: u32,
        amount: u64,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let resource_type_str = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, resource_type_ptr, resource_type_len)?;
        
        let token_resource_type_for_scoped_token = if resource_type_str == "mana" {
            "mana".to_string()
        } else {
            // For other resource types, we might still need to map to the ResourceType enum
            // or handle them differently. For now, assume if not "mana", it's an error or unsupported.
            // This part might need to be expanded based on how other resources are managed.
            // let resolved_resource_type = match resource_type_str.as_str() {
            //     "cpu" => ResourceType::Cpu,
            //     "memory" => ResourceType::Memory,
            //     _ => return Err(HostAbiError::InvalidParameter(format!("Unsupported resource type string: {}", resource_type_str))),
            // };
            // resolved_resource_type.to_string() // This would require ResourceType to impl Display or similar
            return Err(HostAbiError::InvalidParameter(format!("Unsupported resource type string for host_use_resource: {}", resource_type_str)));
        };

        let token = ScopedResourceToken {
            resource_type: token_resource_type_for_scoped_token, 
            scope: format!("{:?}", self.scope_key()),
            amount,
            expires_at: None,
            issuer: Some(self.caller_did.to_string()),
        };
        
        self.rt.mana_repository().record_usage(&self.caller_did, &token).await
            .map_err(|e| HostAbiError::ResourceManagementError(format!("Failed to record usage for resource '{}': {}", resource_type_str, e)))?;
        Ok(0)
    }

    async fn host_transfer_token(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        token_type_ptr: u32,
        token_type_len: u32,
        amount: u64,
        sender_did_ptr: u32,
        sender_did_len: u32,
        recipient_did_ptr: u32,
        recipient_did_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let token_type_str = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, token_type_ptr, token_type_len)?;
        let sender_did_str = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, sender_did_ptr, sender_did_len)?;
        let recipient_did_str = read_string_from_mem_ctx::<T_param>(&mut store_context, &memory, recipient_did_ptr, recipient_did_len)?;
        let sender_did = Did::from_str(&sender_did_str).map_err(|e| HostAbiError::InvalidDid(format!("Invalid sender DID: {}, e: {}", sender_did_str, e)))?;
        let recipient_did = Did::from_str(&recipient_did_str).map_err(|e| HostAbiError::InvalidDid(format!("Invalid recipient DID: {}, e: {}", recipient_did_str, e)))?;
        let mut ctx = self.ctx.lock().await;
        ctx.transfer_token(token_type_str, amount as i64, Some(sender_did.to_string()), recipient_did.to_string())?;
        Ok(0)
    }

    async fn host_submit_mesh_job(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment<T_param>>,
        cbor_payload_ptr: u32,
        cbor_payload_len: u32,
        job_id_buffer_ptr: u32,
        job_id_buffer_len: u32,
    ) -> Result<i32, HostAbiError> {
        let memory = get_memory::<T_param>(&mut caller)?;
        let mut store_context = caller.as_context_mut();
        let cbor_payload = read_bytes_from_mem_ctx::<T_param>(&mut store_context, &memory, cbor_payload_ptr, cbor_payload_len)?;
        let mut ctx = self.ctx.lock().await;
        let job_id_len = ctx.submit_mesh_job(
            cbor_payload, 
            |job_id_str: &str| write_string_to_mem_ctx::<T_param>(&mut store_context, &memory, job_id_str, job_id_buffer_ptr, job_id_buffer_len)
        )?;
        Ok(job_id_len as i32)
    }
}

#[cfg(test)]
impl<T_param: Send + Sync + 'static> ConcreteHostEnvironment<T_param> {
    pub async fn test_host_account_get_mana(
        &self,
        did: &Did, // The DID whose mana is being fetched
    ) -> Result<i64, HostAbiError> {
        let scope_key = self.scope_key(); // Assuming this exists and returns ScopeKey
        // Use the ResourceRepository::get_usage method for mana
        match self.rt.mana_repository().get_usage(did, "mana", &format!("{:?}", scope_key)).await {
            Ok(mana_amount) => Ok(mana_amount as i64),
            Err(_e) => {
                eprintln!("Test shim get_usage for mana error: {:?}", _e);
                Err(HostAbiError::StorageError("Failed to get mana usage in test".to_string()))
            }
        }
    }

    pub async fn test_host_account_spend_mana(
        &self,
        did: &Did, // The DID of the account to spend from
        amount: u64,
    ) -> Result<i32, HostAbiError> {
        let scope_key = self.scope_key();
        let scope_str = format!("{:?}", scope_key); // Use the ScopeKey's debug representation

        let token = ScopedResourceToken {
            resource_type: "mana".to_string(),
            scope: scope_str,
            amount: amount,
            expires_at: None,
            issuer: None,
        };

        // Check authorization using PolicyEnforcer trait method
        match self.rt.policy_enforcer().check_authorization(did, &token).await {
            Ok(true) => { /* Authorized, proceed */ }
            Ok(false) => {
                eprintln!("Test shim policy check: Not authorized (check_authorization returned false)");
                return Err(HostAbiError::NotPermitted);
            }
            Err(e) => {
                eprintln!("Test shim policy check_authorization error: {:?}", e);
                // Map ResourceAuthorizationError to HostAbiError
                // This requires ResourceAuthorizationError to be convertible or matched
                // For now, a generic error:
                return Err(HostAbiError::NotPermitted);
            }
        }

        // Spend mana using ResourceRepository::record_usage
        match self.rt.mana_repository().record_usage(did, &token).await {
            Ok(_) => Ok(0), // Success
            Err(e) => {
                // Try to downcast to anyhow, then potentially to ManaError if wrapped by ManaRepositoryAdapter
                if let Some(mana_err) = e.downcast_ref::<icn_economics::mana::ManaError>() { // Fully qualify ManaError
                    match mana_err {
                        icn_economics::mana::ManaError::InsufficientMana { .. } => {
                            return Err(HostAbiError::InsufficientBalance);
                        }
                        // Other ManaError variants if any
                    }
                } else {
                    eprintln!("Test shim record_usage (spend_mana) unknown error: {:?}", e);
                }
                Err(HostAbiError::StorageError("Failed to spend mana in test due to unknown repository error".to_string())) // Fallback error
            }
        }
    }
}
