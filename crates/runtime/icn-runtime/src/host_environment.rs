use crate::context::RuntimeContext;
use icn_economics::ResourceType;
use icn_identity::Did;
use icn_mesh_receipts::{ExecutionReceipt, verify_embedded_signature, SignError as ReceiptSignError};
use icn_types::dag::ReceiptNode;
use icn_types::dag_store::DagStore;
use icn_types::org::{CooperativeId, CommunityId};
use icn_mesh_protocol::{JobInteractiveInputV1, JobInteractiveOutputV1, MeshProtocolMessage, P2PJobStatus, INLINE_PAYLOAD_MAX_SIZE, MAX_INTERACTIVE_INPUT_BUFFER_PEEK};
use serde::{Serialize, Deserialize};
use serde_cbor;
use std::sync::{Arc, Mutex};
use std::str::FromStr;
use anyhow::Result;
use thiserror::Error;
use host_abi::*;
use crate::job_execution_context::{JobExecutionContext, JobPermissions};
use icn_types::mesh::{MeshJobParams, StageInputSource};
use std::time::{Duration, Instant};
use wasmtime::{Caller, Trap, Memory as WasmtimeMemory, Extern};
use tracing;
use std::convert::TryFrom;

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
    
    pub fn new_governance(ctx: Arc<Mutex<JobExecutionContext>>, caller_did: Did, runtime_ctx: Arc<RuntimeContext>) -> Self {
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

    pub fn check_resource_authorization(&self, rt_type: ResourceType, amt: u64) -> i32 { HostAbiError::NotSupported as i32 }
    pub fn record_resource_usage(&self, rt_type: ResourceType, amt: u64) -> i32 { HostAbiError::NotSupported as i32 }
    pub fn is_governance_context(&self) -> i32 { if self.is_governance { 1 } else { 0 } }
    pub fn mint_token(&self, recipient_did_str: &str, amount: u64) -> i32 { HostAbiError::NotSupported as i32 }
    pub fn transfer_token(&self, sender_did_str: &str, recipient_did_str: &str, amount: u64) -> i32 { HostAbiError::NotSupported as i32 }

    pub async fn anchor_receipt(&self, mut receipt: ExecutionReceipt) -> Result<(), AnchorError> { Ok(()) }
}

// ABI Implementation using Wasmtime
impl MeshHostAbi<ConcreteHostEnvironment> for ConcreteHostEnvironment {
    fn host_job_get_id(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, job_id_buf_ptr: u32, job_id_buf_len: u32) -> Result<i32, Trap> {
        let job_id_bytes = {
            let host_env = caller.data();
            let ctx = host_env.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;
            ctx.job_id.as_bytes().to_vec()
        };

        if job_id_buf_len < job_id_bytes.len() as u32 {
            return Err(Trap::new(HostAbiError::BufferTooSmall.to_string()));
        }
        
        let memory = match caller.get_export("memory") {
            Some(Extern::Memory(mem)) => mem,
            _ => return Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
        };

        match memory.write(&mut caller, job_id_buf_ptr as usize, &job_id_bytes) {
            Ok(_) => Ok(job_id_bytes.len() as i32),
            Err(_) => Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
        }
    }

    fn host_job_get_initial_input_cid(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, cid_buf_ptr: u32, cid_buf_len: u32) -> Result<i32, Trap> {
        let input_cid_bytes_opt = {
            let host_env = caller.data();
            let ctx = host_env.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;
            ctx.job_params.input_data_cid.as_ref().map(|s| s.clone().into_bytes())
        };

        if let Some(input_cid_bytes) = input_cid_bytes_opt {
            if cid_buf_len < input_cid_bytes.len() as u32 {
                return Err(Trap::new(HostAbiError::BufferTooSmall.to_string()));
            }
            
            let memory = match caller.get_export("memory") {
                Some(Extern::Memory(mem)) => mem,
                _ => return Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
            };

            match memory.write(&mut caller, cid_buf_ptr as usize, &input_cid_bytes) {
                Ok(_) => Ok(input_cid_bytes.len() as i32),
                Err(_) => Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
            }
        } else {
            Ok(0) // Return 0 if no input CID
        }
    }

    fn host_job_is_interactive(&self, caller: Caller<'_, ConcreteHostEnvironment>) -> Result<i32, Trap> {
        let host_env = caller.data();
        let ctx = host_env.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;
        if ctx.job_params.is_interactive { Ok(1) } else { Ok(0) }
    }

    fn host_workflow_get_current_stage_index(&self, caller: Caller<'_, ConcreteHostEnvironment>) -> Result<i32, Trap> {
        let host_env = caller.data();
        let ctx = host_env.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;
        Ok(ctx.current_stage_index.map_or(-1, |idx| idx as i32))
    }

    fn host_workflow_get_current_stage_id(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, stage_id_buf_ptr: u32, stage_id_buf_len: u32) -> Result<i32, Trap> {
        let stage_id_bytes_opt = {
            let host_env = caller.data();
            let ctx = host_env.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;
            ctx.current_stage_index
                .and_then(|idx| ctx.job_params.stages.as_ref()?.get(idx as usize))
                .map(|stage| stage.stage_id.as_bytes().to_vec())
        };

        if let Some(stage_id_bytes) = stage_id_bytes_opt {
             if stage_id_buf_len < stage_id_bytes.len() as u32 {
                return Err(Trap::new(HostAbiError::BufferTooSmall.to_string()));
             }
            let memory = match caller.get_export("memory") {
                Some(Extern::Memory(mem)) => mem,
                _ => return Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
            };
            match memory.write(&mut caller, stage_id_buf_ptr as usize, &stage_id_bytes) {
                Ok(_) => Ok(stage_id_bytes.len() as i32),
                Err(_) => Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
            }
        } else {
            Ok(0) // No current stage index, no stages, or stage not found
        }
    }
    
    fn host_workflow_get_current_stage_input_cid(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        input_key_ptr: u32,
        input_key_len: u32,
        cid_buf_ptr: u32,
        cid_buf_len: u32,
    ) -> Result<i32, Trap> {
         tracing::debug!("host_workflow_get_current_stage_input_cid called (Not Implemented)");
         // TODO: Implement logic to read input_key, check current stage input_source,
         // find previous stage output or job input, resolve CID, write to buffer.
         Err(Trap::new(HostAbiError::NotSupported.to_string()))
    }

    fn host_job_report_progress(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, percentage: u8, status_message_ptr: u32, status_message_len: u32) -> Result<i32, Trap> {
        let (host_env_caller_did, mut ctx) = {
             let host_env = caller.data();
             let ctx_guard = host_env.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;
             (host_env.caller_did.clone(), host_env.ctx.clone())
        };
        let mut ctx_guard = ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;

        let status_text = if status_message_len > 0 {
            let memory = match caller.get_export("memory") {
                Some(Extern::Memory(mem)) => mem,
                _ => return Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
            };
            let mut buffer = vec![0u8; status_message_len as usize];
            memory.read(&caller, status_message_ptr as usize, &mut buffer)
                .map_err(|_| Trap::new(HostAbiError::MemoryAccessError.to_string()))?;
            String::from_utf8(buffer).map_err(|_| Trap::new(HostAbiError::InvalidUTF8String.to_string()))?
        } else {
            String::new()
        };
        
        tracing::debug!("[ABI] host_job_report_progress: {}%, message: '{}'", percentage, status_text);
        
        if let P2PJobStatus::Running { progress_percent, status_message, .. } = &mut ctx_guard.current_status {
            *progress_percent = Some(percentage);
            *status_message = Some(status_text);
        } else {
            ctx_guard.current_status = P2PJobStatus::Running {
                node_id: host_env_caller_did,
                current_stage_index: ctx_guard.current_stage_index,
                current_stage_id: ctx_guard.current_stage_id.clone(),
                progress_percent: Some(percentage),
                status_message: Some(status_text),
            };
        }
        Ok(HostAbiError::Success as i32)
    }

    fn host_workflow_complete_current_stage(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, output_cid_ptr: u32, output_cid_len: u32) -> Result<i32, Trap> {
        let mut ctx_guard = {
            let host_env = caller.data();
            host_env.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?
        };

        let output_cid_str = if output_cid_len > 0 {
            let memory = match caller.get_export("memory") {
                Some(Extern::Memory(mem)) => mem,
                _ => return Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
            };
            let mut buffer = vec![0u8; output_cid_len as usize];
            memory.read(&caller, output_cid_ptr as usize, &mut buffer)
                .map_err(|_| Trap::new(HostAbiError::MemoryAccessError.to_string()))?;
            String::from_utf8(buffer).map_err(|_| Trap::new(HostAbiError::InvalidUTF8String.to_string()))?
        } else {
            String::new() 
        };

        tracing::debug!("[ABI] host_workflow_complete_current_stage called. Output CID: '{}'", output_cid_str);
        Err(Trap::new(HostAbiError::NotSupported.to_string()))
    }

    fn host_interactive_send_output(
        &self, 
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        payload_ptr: u32, 
        payload_len: u32, 
        output_key_ptr: u32, 
        output_key_len: u32, 
        is_final_chunk: i32
    ) -> Result<i32, Trap> {
        let (can_send, is_interactive, job_id, originator_did, caller_did, mut ctx_mutex) = {
            let host_env = caller.data();
            let ctx_guard = host_env.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;
            (
                ctx_guard.permissions.can_send_interactive_output,
                ctx_guard.job_params.is_interactive,
                ctx_guard.job_id.clone(),
                ctx_guard.originator_did.clone(),
                host_env.caller_did.clone(),
                host_env.ctx.clone()
            )
        };

        if !can_send { return Err(Trap::new(HostAbiError::NotPermitted.to_string())); }
        if !is_interactive { return Err(Trap::new(HostAbiError::InvalidState.to_string())); }

        let memory = match caller.get_export("memory") {
            Some(Extern::Memory(mem)) => mem,
            _ => return Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
        };

        let mut payload_data_buffer = vec![0u8; payload_len as usize];
        memory.read(&caller, payload_ptr as usize, &mut payload_data_buffer)
            .map_err(|_| Trap::new(HostAbiError::MemoryAccessError.to_string()))?;

        let output_key = if output_key_len > 0 {
            let mut key_buffer = vec![0u8; output_key_len as usize];
            memory.read(&caller, output_key_ptr as usize, &mut key_buffer)
                .map_err(|_| Trap::new(HostAbiError::MemoryAccessError.to_string()))?;
            String::from_utf8(key_buffer).map_err(|_| Trap::new(HostAbiError::InvalidUTF8String.to_string()))?
        } else {
            "default".to_string()
        };
        
        let sequence_num = {
            let mut ctx_guard = ctx_mutex.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;
            ctx_guard.interactive_output_sequence_num += 1;
            ctx_guard.interactive_output_sequence_num
        };

        let message = JobInteractiveOutputV1 {
            sequence_num,
            data: payload_data_buffer,
            output_key,
            is_final_chunk: is_final_chunk == 1,
        };

        tracing::debug!("[ABI] host_interactive_send_output: Enqueuing message (seq: {}) for job {}. (P2P send not implemented in sync ABI)", message.sequence_num, job_id);
        Ok(HostAbiError::Success as i32)
    }

    fn host_interactive_receive_input(
        &self, 
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        buffer_ptr: u32, 
        buffer_len: u32, 
        timeout_ms: u32
    ) -> Result<i32, Trap> {
        let host_env_data = caller.data().clone();
        let start_time = Instant::now();

        loop {
            let mut ctx_guard = host_env_data.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;

            if !ctx_guard.job_params.is_interactive {
                return Err(Trap::new(HostAbiError::InvalidState.to_string()));
            }
            if !matches!(ctx_guard.current_status, P2PJobStatus::Running {..} | P2PJobStatus::PendingUserInput {..}) {
                return Err(Trap::new(HostAbiError::InvalidState.to_string()));
            }

            if let Some(input_msg) = ctx_guard.interactive_input_queue.pop_front() {
                let data_bytes = input_msg.data;
                let data_len = data_bytes.len() as u32;

                if buffer_len < data_len {
                    ctx_guard.interactive_input_queue.push_front(input_msg);
                    return Err(Trap::new(HostAbiError::BufferTooSmall.to_string()));
                }

                let memory = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => {
                        ctx_guard.interactive_input_queue.push_front(input_msg);
                        return Err(Trap::new(HostAbiError::MemoryAccessError.to_string()));
                    }
                };
                
                memory.write(&mut caller, buffer_ptr as usize, &data_bytes)
                    .map_err(|_| Trap::new(HostAbiError::MemoryAccessError.to_string()))?;

                if matches!(ctx_guard.current_status, P2PJobStatus::PendingUserInput {..}) {
                     ctx_guard.current_status = P2PJobStatus::Running {
                        node_id: host_env_data.caller_did.clone(),
                        current_stage_index: ctx_guard.current_stage_index,
                        current_stage_id: ctx_guard.current_stage_id.clone(),
                        progress_percent: Some(ctx_guard.job_params.stages.as_ref().map_or(50, |s| if s.is_empty() {50} else { (ctx_guard.current_stage_index.unwrap_or(0) * 100 / s.len().max(1) as u32) as u8 } ) ),
                        status_message: Some("Input received, resuming operation.".to_string()),
                    };
                }
                return Ok(data_len as i32);
            }
            
            drop(ctx_guard);

            if timeout_ms == 0 { return Ok(0); }

            if start_time.elapsed() >= Duration::from_millis(timeout_ms as u64) {
                return Err(Trap::new(HostAbiError::Timeout.to_string())); 
            }
            
            return Ok(0);
        }
    }

    fn host_interactive_peek_input_len(&self, caller: Caller<'_, ConcreteHostEnvironment>) -> Result<i32, Trap> {
        let host_env = caller.data();
        let ctx = host_env.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;

        if let Some(input_msg) = ctx.interactive_input_queue.front() {
            let data_len = input_msg.data.len() as i32;
            if data_len > MAX_INTERACTIVE_INPUT_BUFFER_PEEK as i32 {
                 Ok(MAX_INTERACTIVE_INPUT_BUFFER_PEEK as i32)
            } else {
                Ok(data_len)
            }
        } else {
            Ok(0)
        }
    }

    fn host_interactive_prompt_for_input(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, prompt_cid_ptr: u32, prompt_cid_len: u32) -> Result<i32, Trap> {
        let (is_interactive, job_id, caller_did, mut ctx_mutex) = {
             let host_env = caller.data();
             let ctx_guard = host_env.ctx.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;
             (
                ctx_guard.job_params.is_interactive,
                ctx_guard.job_id.clone(),
                host_env.caller_did.clone(),
                host_env.ctx.clone(),
             )
        };

        if !is_interactive {
            return Err(Trap::new(HostAbiError::NotPermitted.to_string()));
        }

        let prompt_message = if prompt_cid_len > 0 {
            let memory = match caller.get_export("memory") {
                Some(Extern::Memory(mem)) => mem,
                _ => return Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
            };
            let mut buffer = vec![0u8; prompt_cid_len as usize];
            memory.read(&caller, prompt_cid_ptr as usize, &mut buffer)
                .map_err(|_| Trap::new(HostAbiError::MemoryAccessError.to_string()))?;
            String::from_utf8(buffer).map_err(|_| Trap::new(HostAbiError::InvalidUTF8String.to_string()))?
        } else {
            "Awaiting input".to_string()
        };
        
        {
            let mut ctx_guard = ctx_mutex.lock().map_err(|_| Trap::new(HostAbiError::UnknownError.to_string()))?;
            ctx_guard.current_status = P2PJobStatus::PendingUserInput {
                node_id: caller_did.clone(),
                current_stage_index: ctx_guard.current_stage_index,
                current_stage_id: ctx_guard.current_stage_id.clone(), 
                status_message: Some(prompt_message.clone()),
            };
        }
        
        tracing::debug!("[ABI] host_interactive_prompt_for_input: Job {} now PendingUserInput. Prompt: '{}'", job_id, prompt_message);
        Ok(HostAbiError::Success as i32)
    }

    fn host_data_read_cid(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, cid_ptr: u32, cid_len: u32, offset: u64, buffer_ptr: u32, buffer_len: u32) -> Result<i32, Trap> {
        tracing::warn!("host_data_read_cid: Not implemented in sync ABI due to async storage requirement.");
        Err(Trap::new(HostAbiError::NotSupported.to_string()))
    }

    fn host_data_write_buffer(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, data_ptr: u32, data_len: u32, cid_buf_ptr: u32, cid_buf_len: u32) -> Result<i32, Trap> {
        tracing::warn!("host_data_write_buffer: Not implemented in sync ABI due to async storage requirement.");
        Err(Trap::new(HostAbiError::NotSupported.to_string()))
    }

    fn host_log_message(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, level: LogLevel, message_ptr: u32, message_len: u32) -> Result<i32, Trap> {
        let job_id_str = {
            caller.data().ctx.lock().map(|ctx| ctx.job_id.to_string()).unwrap_or_else(|_| "<unknown_job>".to_string())
        };
        
        if message_len == 0 {
            tracing::debug!(job_id = %job_id_str, wasm_module_log = "[Empty log message received]");
            return Ok(HostAbiError::Success as i32); 
        }

        let memory = match caller.get_export("memory") {
            Some(Extern::Memory(mem)) => mem,
            _ => return Err(Trap::new(HostAbiError::MemoryAccessError.to_string())),
        };
        let mut msg_bytes = vec![0u8; message_len as usize];
        memory.read(&caller, message_ptr as usize, &mut msg_bytes)
            .map_err(|_| Trap::new(HostAbiError::MemoryAccessError.to_string()))?;
        
        let message = String::from_utf8_lossy(&msg_bytes);

        match level {
            LogLevel::Error => tracing::error!(job_id = %job_id_str, wasm_module_log = %message),
            LogLevel::Warn => tracing::warn!(job_id = %job_id_str, wasm_module_log = %message),
            LogLevel::Info => tracing::info!(job_id = %job_id_str, wasm_module_log = %message),
            LogLevel::Debug => tracing::debug!(job_id = %job_id_str, wasm_module_log = %message),
            LogLevel::Trace => tracing::trace!(job_id = %job_id_str, wasm_module_log = %message),
        }
        Ok(HostAbiError::Success as i32)
    }
} 