use crate::context::RuntimeContext;
use crate::job_execution_context::JobExecutionContext;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use icn_core_vm::HostContext as CoreVmHostContext;
use icn_economics::mana::ManaLedger;
use icn_economics::ResourceType;
use icn_identity::{Did, KeyPair as IcnKeyPair};
use icn_types::mesh::JobStatus;
use icn_mesh_receipts::{verify_embedded_signature, ExecutionReceipt};
use host_abi::{
    HostAbiError, LogLevel, MeshHostAbi, ReceivedInputInfo, ICN_HOST_ABI_VERSION,
};
use icn_types::org::{CommunityId, CooperativeId};
use icn_types::runtime_receipt::RuntimeExecutionMetrics;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use wasmtime::{Caller, Extern, Memory as WasmtimeMemory};

/// Errors that can occur during receipt anchoring
#[derive(Debug, Error)]
pub enum AnchorError {
    #[error("Executor mismatch: receipt's executor ({0}) does not match caller ({1})")]
    ExecutorMismatch(String, String),

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("CID generation error: {0}")]
    CidError(String),

    #[error("DAG store error: {0}")]
    DagStoreError(String),

    #[error("Missing federation ID")]
    MissingFederationId,
}

/// Concrete implementation of the host environment for WASM execution
#[derive(Clone)]
pub struct ConcreteHostEnvironment {
    pub ctx: Arc<Mutex<JobExecutionContext>>,
    pub rt: Arc<RuntimeContext>,
    pub caller_did: Did,
    pub is_governance: bool,
    pub coop_id: Option<CooperativeId>,
    pub community_id: Option<CommunityId>,
}

impl ConcreteHostEnvironment {
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
    fn scope_key(&self) -> ScopeKey {
        // 1) If explicit coop/community overrides exist, honour them first.
        if let Some(coop) = &self.coop_id {
            ScopeKey::Cooperative(coop.to_string())
        } else if let Some(comm) = &self.community_id {
            ScopeKey::Community(comm.to_string())
        } else if let Some(index) = &self.rt.identity_index {
            index.resolve_scope_key(&self.caller_did)
        } else if let Some(fid) = &self.rt.federation_id {
            // Fallback to federation scope if runtime context specifies it explicitly
            ScopeKey::Federation(fid.to_string())
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
    pub async fn anchor_receipt(&self, _receipt: ExecutionReceipt) -> Result<(), AnchorError> {
        // Placeholder implementation. In a real scenario, this would interact with
        // the DAG store, potentially via the RuntimeContext or a dedicated service.
        Ok(())
    }

    // ---------------------- Helper memory access methods ----------------------

    /// Helper to safely obtain the linear memory exported by the guest module.
    pub fn get_memory(
        &self,
        caller: &mut Caller<'_, ConcreteHostEnvironment>,
    ) -> Result<WasmtimeMemory, anyhow::Error> {
        match caller.get_export("memory") {
            Some(Extern::Memory(mem)) => Ok(mem),
            _ => Err(anyhow!(HostAbiError::MemoryAccessError)),
        }
    }

    /// Read a UTF-8 string from guest memory at (ptr,len).
    pub fn read_string_from_mem(
        &self,
        caller: &mut Caller<'_, ConcreteHostEnvironment>,
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
        caller: &mut Caller<'_, ConcreteHostEnvironment>,
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
        caller: &mut Caller<'_, ConcreteHostEnvironment>,
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
        caller: &mut Caller<'_, ConcreteHostEnvironment>,
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

#[cfg(feature = "full_host_abi")]
impl MeshHostAbi<ConcreteHostEnvironment> for ConcreteHostEnvironment {
    // **I. Job & Workflow Information **
    fn host_job_get_id(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        job_id_buf_ptr: u32,
        job_id_buf_len: u32,
    ) -> Result<i32, anyhow::Error> {
        let host_env = caller.data();
        let ctx = host_env
            .ctx
            .lock()
            .map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        let job_id_str = ctx.job_id.to_string();
        self.write_string_to_mem(&mut caller, &job_id_str, job_id_buf_ptr, job_id_buf_len)
    }

    fn host_job_get_initial_input_cid(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        cid_buf_ptr: u32,
        cid_buf_len: u32,
    ) -> Result<i32, anyhow::Error> {
        let host_env = caller.data();
        let ctx = host_env
            .ctx
            .lock()
            .map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        if let Some(cid) = &ctx.job_params.input_data_cid {
            let cid_str = cid.to_string();
            self.write_string_to_mem(&mut caller, &cid_str, cid_buf_ptr, cid_buf_len)
        } else {
            Ok(0) // No input CID specified
        }
    }

    fn host_job_is_interactive(
        &self,
        caller: Caller<'_, ConcreteHostEnvironment>,
    ) -> Result<i32, anyhow::Error> {
        let host_env = caller.data();
        let ctx = host_env
            .ctx
            .lock()
            .map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        Ok(if ctx.job_params.is_interactive { 1 } else { 0 })
    }

    fn host_workflow_get_current_stage_index(
        &self,
        caller: Caller<'_, ConcreteHostEnvironment>,
    ) -> Result<i32, anyhow::Error> {
        let host_env = caller.data();
        let ctx = host_env
            .ctx
            .lock()
            .map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        Ok(ctx.current_stage_index.unwrap_or(-1) as i32)
    }

    fn host_workflow_get_current_stage_id(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        stage_id_buf_ptr: u32,
        stage_id_buf_len: u32,
    ) -> Result<i32, anyhow::Error> {
        let host_env = caller.data();
        let ctx = host_env
            .ctx
            .lock()
            .map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        if let Some(stage_index) = ctx.current_stage_index {
            if let Some(workflow) = &ctx.job_params.workflow_type.as_workflow() {
                if let Some(stage) = workflow.stages.get(stage_index as usize) {
                    if let Some(id) = &stage.id {
                        return self.write_string_to_mem(&mut caller, id, stage_id_buf_ptr, stage_id_buf_len);
                    }
                }
            }
        }
        Ok(0) // No stage ID found or not applicable
    }

    fn host_workflow_get_current_stage_input_cid(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        input_key_ptr: u32,
        input_key_len: u32,
        cid_buf_ptr: u32,
        cid_buf_len: u32,
    ) -> Result<i32, anyhow::Error> {
        Ok(HostAbiError::NotSupported as i32)
    }

    // **II. Status & Progress Reporting **
    fn host_job_report_progress(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        percentage: u8,
        status_message_ptr: u32,
        status_message_len: u32,
    ) -> Result<i32, anyhow::Error> {
        Ok(HostAbiError::NotSupported as i32)
    }

    fn host_workflow_complete_current_stage(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        output_cid_ptr: u32,
        output_cid_len: u32,
    ) -> Result<i32, anyhow::Error> {
        Ok(HostAbiError::NotSupported as i32)
    }

    // **III. Interactivity **
    fn host_interactive_send_output(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        payload_ptr: u32,
        payload_len: u32,
        output_key_ptr: u32,
        output_key_len: u32,
        is_final_chunk: i32,
    ) -> Result<i32, anyhow::Error> {
        Ok(HostAbiError::NotSupported as i32)
    }

    fn host_interactive_receive_input(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        buffer_ptr: u32,
        buffer_len: u32,
        timeout_ms: u32,
    ) -> Result<i32, anyhow::Error> {
        Ok(HostAbiError::NotSupported as i32)
    }

    fn host_interactive_peek_input_len(
        &self,
        caller: Caller<'_, ConcreteHostEnvironment>,
    ) -> Result<i32, anyhow::Error> {
        Ok(HostAbiError::NotSupported as i32)
    }

    fn host_interactive_prompt_for_input(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        prompt_cid_ptr: u32,
        prompt_cid_len: u32,
    ) -> Result<i32, anyhow::Error> {
        Ok(HostAbiError::NotSupported as i32)
    }

    // **IV. Data Handling & Storage **
    fn host_data_read_cid(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        cid_ptr: u32,
        cid_len: u32,
        offset: u64,
        buffer_ptr: u32,
        buffer_len: u32,
    ) -> Result<i32, anyhow::Error> {
        Ok(HostAbiError::NotSupported as i32)
    }

    fn host_data_write_buffer(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        data_ptr: u32,
        data_len: u32,
        cid_buf_ptr: u32,
        cid_buf_len: u32,
    ) -> Result<i32, anyhow::Error> {
        Ok(HostAbiError::NotSupported as i32)
    }

    // **V. Logging **
    fn host_log_message(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        level: LogLevel,
        message_ptr: u32,
        message_len: u32,
    ) -> Result<i32, anyhow::Error> {
        Ok(HostAbiError::NotSupported as i32)
    }

    // ---------------- Mana stubs ----------------

    fn host_account_get_mana(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        did_ptr: u32,
        did_len: u32,
    ) -> Result<i64, anyhow::Error> {
        // If explicit DID is provided we use individual scope for that DID; otherwise default scope_key()
        let scope_key = if did_len == 0 {
            self.scope_key()
        } else {
            let did = self.read_string_from_mem(&mut caller, did_ptr, did_len)?;
            ScopeKey::Individual(did)
        };

        let mut mana_mgr = self
            .rt
            .mana_manager
            .lock()
            .map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        let balance_opt = mana_mgr.balance(&scope_key);
        Ok(balance_opt.unwrap_or(0) as i64)
    }

    fn host_account_spend_mana(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        did_ptr: u32,
        did_len: u32,
        amount: u64,
    ) -> Result<i32, anyhow::Error> {
        let scope_key = if did_len == 0 {
            self.scope_key()
        } else {
            let did = self.read_string_from_mem(&mut caller, did_ptr, did_len)?;
            ScopeKey::Individual(did)
        };

        let mut mana_mgr = self
            .rt
            .mana_manager
            .lock()
            .map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        match mana_mgr.spend(&scope_key, amount) {
            Ok(_) => Ok(0),
            Err(_) => Ok(HostAbiError::ResourceLimitExceeded as i32),
        }
    }
}

#[cfg(not(feature = "full_host_abi"))]
impl MeshHostAbi<ConcreteHostEnvironment> for ConcreteHostEnvironment {
    fn host_job_get_id(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _ptr: u32,
        _len: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_job_get_initial_input_cid(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _ptr: u32,
        _len: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_job_is_interactive(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_workflow_get_current_stage_index(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_workflow_get_current_stage_id(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _ptr: u32,
        _len: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_workflow_get_current_stage_input_cid(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _key_ptr: u32,
        _key_len: u32,
        _cid_ptr: u32,
        _cid_len: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_job_report_progress(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _pct: u8,
        _msg_ptr: u32,
        _msg_len: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_workflow_complete_current_stage(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _cid_ptr: u32,
        _cid_len: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_interactive_send_output(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _payload_ptr: u32,
        _payload_len: u32,
        _key_ptr: u32,
        _key_len: u32,
        _is_final: i32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_interactive_receive_input(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _buffer_ptr: u32,
        _buffer_len: u32,
        _timeout_ms: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_interactive_peek_input_len(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_interactive_prompt_for_input(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _ptr: u32,
        _len: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_data_read_cid(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _cid_ptr: u32,
        _cid_len: u32,
        _offset: u64,
        _buffer_ptr: u32,
        _buffer_len: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_data_write_buffer(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _data_ptr: u32,
        _data_len: u32,
        _cid_buf_ptr: u32,
        _cid_buf_len: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_log_message(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _level: LogLevel,
        _ptr: u32,
        _len: u32,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }

    fn host_account_get_mana(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _did_ptr: u32,
        _did_len: u32,
    ) -> Result<i64, anyhow::Error> { Ok(HostAbiError::NotSupported as i32 as i64) }

    fn host_account_spend_mana(
        &self,
        _caller: Caller<'_, ConcreteHostEnvironment>,
        _did_ptr: u32,
        _did_len: u32,
        _amount: u64,
    ) -> Result<i32, anyhow::Error> { Ok(HostAbiError::NotSupported as i32) }
}
