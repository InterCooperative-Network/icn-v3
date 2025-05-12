use crate::context::RuntimeContext;
use icn_economics::ResourceType;
use icn_economics::mana::ScopeKey;
use icn_identity::Did;
use icn_mesh_receipts::{ExecutionReceipt, verify_embedded_signature, SignError as ReceiptSignError};
use icn_types::org::{CooperativeId, CommunityId};
use icn_mesh_protocol::{JobInteractiveInputV1, JobInteractiveOutputV1, P2PJobStatus, INLINE_PAYLOAD_MAX_SIZE, MAX_INTERACTIVE_INPUT_BUFFER_PEEK};
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
use anyhow::{anyhow, Error as AnyhowError};
use cid::Cid;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use tokio::sync::mpsc::{Sender, Receiver};
use icn_types::dag::{DagEventType, DagNodeBuilder};
use icn_types::dag_store::DagStore;

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

    /// Determine the accounting scope key for mana operations.
    fn scope_key(&self) -> ScopeKey {
        if let Some(coop) = &self.coop_id {
            ScopeKey::Cooperative(coop.to_string())
        } else if let Some(comm) = &self.community_id {
            ScopeKey::Community(comm.to_string())
        } else if let Some(fid) = &self.rt.federation_id {
            // Fallback to federation scope if runtime context specifies it explicitly
            ScopeKey::Federation(fid.to_string())
        } else {
            ScopeKey::Individual(self.caller_did.to_string())
        }
    }

    pub fn check_resource_authorization(&self, rt_type: ResourceType, amt: u64) -> i32 { HostAbiError::NotSupported as i32 }
    pub async fn record_resource_usage(&self, _rt_type: ResourceType, _amt: u64) -> i32 { HostAbiError::NotSupported as i32 }
    pub fn is_governance_context(&self) -> i32 { if self.is_governance { 1 } else { 0 } }
    pub async fn mint_token(&self, _recipient_did_str: &str, _amount: u64) -> i32 { HostAbiError::NotSupported as i32 }
    pub async fn transfer_token(&self, _sender_did_str: &str, _recipient_did_str: &str, _amount: u64) -> i32 { HostAbiError::NotSupported as i32 }

    /// Anchor a signed execution receipt to the DAG and broadcast an announcement.
    pub async fn anchor_receipt(&self, mut receipt: ExecutionReceipt) -> Result<(), AnchorError> {
        // 1) Ensure executor matches the caller
        if receipt.executor != self.caller_did {
            return Err(AnchorError::ExecutorMismatch(receipt.executor.to_string(), self.caller_did.to_string()));
        }

        // 2) Verify embedded signature (if present)
        if !receipt.signature.is_empty() {
            verify_embedded_signature(&receipt).map_err(|e| AnchorError::InvalidSignature(e.to_string()))?;
        } else {
            return Err(AnchorError::InvalidSignature("Missing signature".into()));
        }

        // 3) Serialize to CBOR and persist via DagStore
        let _cbor_bytes = serde_cbor::to_vec(&receipt)
            .map_err(|e| AnchorError::SerializationError(e.to_string()))?;

        // --- Build DagNode from receipt ---
        // Determine scope ID (e.g., "receipt/<federation>")
        let federation_id = self
            .rt
            .federation_id
            .clone()
            .ok_or(AnchorError::MissingFederationId)?;
        let scope_id = format!("receipt/{}", federation_id);

        // Build the DAG node using JSON for human-readable payload
        let receipt_json = serde_json::to_string(&receipt)
            .map_err(|e| AnchorError::SerializationError(e.to_string()))?;

        let dag_node = DagNodeBuilder::new()
            .content(receipt_json)
            .event_type(DagEventType::Receipt)
            .timestamp(receipt.execution_end_time)
            .scope_id(scope_id)
            .build()
            .map_err(|e| AnchorError::SerializationError(e.to_string()))?;

        // Insert into the shared receipt store and get CID for confirmation/logging
        let cid = dag_node
            .cid()
            .map_err(|e| AnchorError::CidError(e.to_string()))?;

        self
            .rt
            .receipt_store
            .insert(dag_node)
            .await
            .map_err(|e| AnchorError::DagStoreError(e.to_string()))?;

        tracing::info!(target: "anchor_receipt", "Anchored execution receipt as DAG node with CID: {}", cid);

        Ok(())
    }

    // ---------------------- Helper memory access methods ----------------------

    /// Helper to safely obtain the linear memory exported by the guest module.
    pub fn get_memory(&self, caller: &mut Caller<'_, ConcreteHostEnvironment>) -> Result<WasmtimeMemory, anyhow::Error> {
        match caller.get_export("memory") {
            Some(Extern::Memory(mem)) => Ok(mem),
            _ => Err(anyhow!(HostAbiError::MemoryAccessError)),
        }
    }

    /// Read a UTF-8 string from guest memory at (ptr,len).
    pub fn read_string_from_mem(&self, caller: &mut Caller<'_, ConcreteHostEnvironment>, ptr: u32, len: u32) -> Result<String, anyhow::Error> {
        let mem = self.get_memory(caller)?;
        let mut buffer = vec![0u8; len as usize];
        mem.read(caller, ptr as usize, &mut buffer)
            .map_err(|_| anyhow!(HostAbiError::MemoryAccessError))?;
        String::from_utf8(buffer).map_err(|_| anyhow!(HostAbiError::DataEncodingError))
    }

    /// Write a UTF-8 string `s` into guest memory buffer (ptr,len).
    pub fn write_string_to_mem(&self, caller: &mut Caller<'_, ConcreteHostEnvironment>, s: &str, ptr: u32, len: u32) -> Result<i32, anyhow::Error> {
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
    pub fn read_bytes_from_mem(&self, caller: &mut Caller<'_, ConcreteHostEnvironment>, ptr: u32, len: u32) -> Result<Vec<u8>, anyhow::Error> {
        let mem = self.get_memory(caller)?;
        let mut buffer = vec![0u8; len as usize];
        mem.read(caller, ptr as usize, &mut buffer)
            .map_err(|_| anyhow!(HostAbiError::MemoryAccessError))?;
        Ok(buffer)
    }

    /// Write raw bytes to guest memory buffer (ptr,len).
    pub fn write_bytes_to_mem(&self, caller: &mut Caller<'_, ConcreteHostEnvironment>, bytes: &[u8], ptr: u32, len: u32) -> Result<i32, anyhow::Error> {
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
    fn host_job_get_id(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, job_id_buf_ptr: u32, job_id_buf_len: u32) -> Result<i32, AnyhowError> {
        let host_env = caller.data();
        let ctx = host_env.ctx.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        let job_id_str = ctx.job_id.to_string();
        self.write_string_to_mem(&mut caller, &job_id_str, job_id_buf_ptr, job_id_buf_len)
    }

    fn host_job_get_initial_input_cid(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, cid_buf_ptr: u32, cid_buf_len: u32) -> Result<i32, AnyhowError> {
        let host_env = caller.data();
        let ctx = host_env.ctx.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        if let Some(cid) = &ctx.job_params.input_data_cid {
            let cid_str = cid.to_string();
            self.write_string_to_mem(&mut caller, &cid_str, cid_buf_ptr, cid_buf_len)
        } else {
            Ok(0) // No input CID specified
        }
    }

    fn host_job_is_interactive(&self, caller: Caller<'_, ConcreteHostEnvironment>) -> Result<i32, AnyhowError> {
        let host_env = caller.data();
        let ctx = host_env.ctx.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        Ok(ctx.job_params.is_interactive as i32)
    }

    fn host_workflow_get_current_stage_index(&self, caller: Caller<'_, ConcreteHostEnvironment>) -> Result<i32, AnyhowError> {
        let host_env = caller.data();
        let ctx = host_env.ctx.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        if let Some(index) = ctx.current_stage_index {
            Ok(index as i32)
        } else {
            Ok(-1) // Not a multi-stage workflow or index not set
        }
    }

    fn host_workflow_get_current_stage_id(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        stage_id_buf_ptr: u32,
        stage_id_buf_len: u32,
    ) -> Result<i32, AnyhowError> {
        let host_env = caller.data();
        let ctx = host_env.ctx.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        
        if let Some(index) = ctx.current_stage_index {
            if let Some(workflow) = &ctx.job_params.stages {
                if let Some(stage) = workflow.get(index) {
                    if let Some(id) = &stage.stage_id {
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
    ) -> Result<i32, AnyhowError> {
         let host_env = caller.data();
         let ctx_mutex = Arc::clone(&host_env.ctx); // Clone Arc for locking
         let mut ctx_guard = ctx_mutex.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;
         let runtime_ctx_clone = Arc::clone(&host_env.rt);
         
         let input_key_opt = if input_key_len > 0 {
             Some(self.read_string_from_mem(&mut caller, input_key_ptr, input_key_len)?)
         } else {
             None
         };

         let resolved_cid_res = ctx_guard.resolve_current_stage_input(runtime_ctx_clone.dag_store(), input_key_opt.as_deref());

         match resolved_cid_res {
             Ok(Some(cid)) => {
                let cid_str = cid.to_string();
                self.write_string_to_mem(&mut caller, &cid_str, cid_buf_ptr, cid_buf_len)
             }
             Ok(None) => Ok(0), // Resolved to no input or stage/workflow not applicable
             Err(e) => Err(anyhow!(e)), // Convert JobContextError to anyhow::Error
         }
    }

    // **II. Status & Progress Reporting **
    fn host_job_report_progress(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        percentage: u8,
        status_message_ptr: u32,
        status_message_len: u32,
    ) -> Result<i32, AnyhowError> {
        let host_env = caller.data();
        let ctx_mutex = Arc::clone(&host_env.ctx);
        let mut ctx_guard = ctx_mutex.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;

        if percentage > 100 {
            return Err(anyhow!(HostAbiError::InvalidArguments));
        }

        let status_text = self.read_string_from_mem(&mut caller, status_message_ptr, status_message_len)?;

        // Update internal state
        ctx_guard.progress_percent = Some(percentage);
        ctx_guard.status_message = Some(status_text.clone());

        // Try to send P2P update (best effort, might fail if channel closed)
        if let Some(sender) = &ctx_guard.status_sender {
            let update = P2PJobStatus::Running { percentage, status_message: status_text };
            let msg = MeshProtocolMessage::JobStatusUpdateV1 {
                job_id: ctx_guard.job_id.clone(),
                status: update,
            };
            // Use try_send for non-blocking, ignore QueueFull or ChannelClosed errors
            let _ = sender.try_send(msg).map_err(|e| {
                if e.is_full() {
                     HostAbiError::QueueFull
                } else {
                     HostAbiError::ChannelClosed
                }
            });
        }

        Ok(0) // Success
    }

    fn host_workflow_complete_current_stage(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        output_cid_ptr: u32,
        output_cid_len: u32,
    ) -> Result<i32, AnyhowError> {
        let host_env = caller.data();
        let ctx_mutex = Arc::clone(&host_env.ctx);
        let mut ctx_guard = ctx_mutex.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;

        let output_cid_opt = if output_cid_len > 0 {
            let cid_str = self.read_string_from_mem(&mut caller, output_cid_ptr, output_cid_len)?;
            Some(Cid::from_str(&cid_str).map_err(|_| anyhow!(HostAbiError::InvalidCIDFormat))?)
        } else {
            None
        };

        let res = ctx_guard.complete_current_stage(output_cid_opt);
        match res {
            Ok(_) => Ok(0),
            Err(e) => Err(anyhow!(e)), // Convert JobContextError
        }
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
    ) -> Result<i32, AnyhowError> {
        let host_env = caller.data();
        let ctx_mutex = Arc::clone(&host_env.ctx);
        let ctx_guard = ctx_mutex.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;

        if !ctx_guard.job_params.is_interactive {
            return Err(anyhow!(HostAbiError::NotPermitted));
        }
        if !ctx_guard.permissions.can_send_output {
            return Err(anyhow!(HostAbiError::NotPermitted));
        }

        let output_key = if output_key_len > 0 {
            Some(self.read_string_from_mem(&mut caller, output_key_ptr, output_key_len)?)
        } else {
            None
        };

        let payload = self.read_bytes_from_mem(&mut caller, payload_ptr, payload_len)?;

        let output_msg = JobInteractiveOutputV1 {
            job_id: ctx_guard.job_id.clone(),
            output_key,
            payload, // This will be handled (inline/CID) by the sender logic
            is_final_chunk: is_final_chunk != 0,
        };

        let proto_msg = MeshProtocolMessage::JobInteractiveOutputV1(output_msg);

        if let Some(sender) = &ctx_guard.status_sender {
            // Send might block if channel full, or fail if closed
            sender.try_send(proto_msg).map_err(|e| {
                anyhow!(if e.is_full() { HostAbiError::QueueFull } else { HostAbiError::ChannelClosed })
            })?;
            Ok(0)
        } else {
            Err(anyhow!(HostAbiError::ChannelClosed)) // Sender not available
        }
    }

    fn host_interactive_receive_input(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        buffer_ptr: u32,
        buffer_len: u32,
        timeout_ms: u32,
    ) -> Result<i32, AnyhowError> {
        let host_env = caller.data();
        let ctx_mutex = Arc::clone(&host_env.ctx);
        let runtime = host_env.runtime_handle.clone(); // Clone runtime handle

        runtime.block_on(async {
            let mut ctx_guard = ctx_mutex.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;

            if !ctx_guard.job_params.is_interactive || !ctx_guard.permissions.can_receive_input {
                 return Err(anyhow!(HostAbiError::NotPermitted));
            }

            if ctx_guard.input_receiver.is_none() {
                 return Err(anyhow!(HostAbiError::ChannelClosed));
            }
            let receiver = ctx_guard.input_receiver.as_mut().unwrap(); // Safe due to check above

            let recv_result = if timeout_ms == 0 {
                // Non-blocking
                receiver.try_recv().map_err(|e| match e {
                    tokio::sync::mpsc::error::TryRecvError::Empty => HostAbiError::Timeout, // Use Timeout for empty non-blocking
                    tokio::sync::mpsc::error::TryRecvError::Disconnected => HostAbiError::ChannelClosed,
                })
            } else if timeout_ms == u32::MAX {
                // Blocking indefinitely
                receiver.recv().await.ok_or(HostAbiError::ChannelClosed)
            } else {
                // Blocking with timeout
                 tokio::time::timeout(Duration::from_millis(timeout_ms as u64), receiver.recv())
                    .await
                    .map_err(|_| HostAbiError::Timeout)? // Timeout occurred
                    .ok_or(HostAbiError::ChannelClosed) // Channel closed while waiting
            };

            match recv_result {
                Ok(input_msg) => {
                    // Determine if payload is inline or CID
                    let (input_type, data_bytes) = if input_msg.payload.len() <= INLINE_PAYLOAD_MAX_SIZE {
                        (ReceivedInputType::InlineData, input_msg.payload)
                    } else {
                        // Payload too large, need to store and get CID
                         let dag_store = host_env.runtime_ctx.dag_store();
                         let cid = dag_store.write_dag_node_async(&input_msg.payload).await
                                .map_err(|e| anyhow!(HostAbiError::StorageError).context(e))?; // Convert DagStoreError
                         (ReceivedInputType::Cid, cid.to_string().into_bytes())
                    };

                    let info = ReceivedInputInfo {
                        input_type,
                        data_len: data_bytes.len() as u32,
                    };

                    let info_bytes = unsafe {
                        let ptr = &info as *const ReceivedInputInfo as *const u8;
                        std::slice::from_raw_parts(ptr, std::mem::size_of::<ReceivedInputInfo>())
                    };

                    let total_len = info_bytes.len() + data_bytes.len();
                    if total_len > buffer_len as usize {
                         // TODO: Requeue the message? For now, return BufferTooSmall
                         return Err(anyhow!(HostAbiError::BufferTooSmall));
                    }

                    // Write info struct then data/CID bytes
                    let mem = self.get_memory(&mut caller)?;
                    mem.write(&mut caller, buffer_ptr as usize, info_bytes)
                        .map_err(|_| anyhow!(HostAbiError::MemoryAccessError))?;
                    mem.write(&mut caller, (buffer_ptr as usize) + info_bytes.len(), &data_bytes)
                        .map_err(|_| anyhow!(HostAbiError::MemoryAccessError))?;

                    Ok(total_len as i32)
                }
                Err(HostAbiError::Timeout) => Ok(0), // Timeout or non-blocking empty is not an error, returns 0
                Err(e) => Err(anyhow!(e)), // Other errors (ChannelClosed etc.)
            }
        })
    }

    fn host_interactive_peek_input_len(&self, caller: Caller<'_, ConcreteHostEnvironment>) -> Result<i32, AnyhowError> {
        let host_env = caller.data();
        let ctx_mutex = Arc::clone(&host_env.ctx);
        let runtime = host_env.runtime_handle.clone();

        runtime.block_on(async {
            let mut ctx_guard = ctx_mutex.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;

            if !ctx_guard.job_params.is_interactive || !ctx_guard.permissions.can_receive_input {
                return Err(anyhow!(HostAbiError::NotPermitted));
            }

            if let Some(receiver) = &mut ctx_guard.input_receiver {
                 if let Some(input_msg) = receiver.try_peek() { // Use try_peek
                    let data_len = if input_msg.payload.len() <= INLINE_PAYLOAD_MAX_SIZE {
                        input_msg.payload.len()
                    } else {
                         // Need CID length - estimate or calculate precisely if needed
                         // For simplicity, estimate based on standard CIDv1 Base32 length (around 59 chars)
                         60 // Approximate length for CID string
                    };
                    let total_len = std::mem::size_of::<ReceivedInputInfo>() + data_len;
                    Ok(total_len as i32)
                 } else {
                     Ok(0) // No message available
                 }
            } else {
                 Err(anyhow!(HostAbiError::ChannelClosed))
            }
        })
    }

    fn host_interactive_prompt_for_input(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        prompt_cid_ptr: u32,
        prompt_cid_len: u32,
    ) -> Result<i32, AnyhowError> {
         let host_env = caller.data();
         let ctx_mutex = Arc::clone(&host_env.ctx);
         let mut ctx_guard = ctx_mutex.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;

         if !ctx_guard.job_params.is_interactive {
             return Err(anyhow!(HostAbiError::NotPermitted));
         }

         let prompt_cid_opt = if prompt_cid_len > 0 {
             let cid_str = self.read_string_from_mem(&mut caller, prompt_cid_ptr, prompt_cid_len)?;
             Some(Cid::from_str(&cid_str).map_err(|_| anyhow!(HostAbiError::InvalidCIDFormat))?)
         } else {
             None
         };

         // Update job state
         ctx_guard.is_awaiting_input = true;
         // Maybe store prompt_cid_opt in context if needed later

         // Send status update
         if let Some(sender) = &ctx_guard.status_sender {
             let update = P2PJobStatus::PendingUserInput { prompt_cid: prompt_cid_opt };
             let msg = MeshProtocolMessage::JobStatusUpdateV1 {
                 job_id: ctx_guard.job_id.clone(),
                 status: update,
             };
             let _ = sender.try_send(msg).map_err(|e| {
                 if e.is_full() { HostAbiError::QueueFull } else { HostAbiError::ChannelClosed }
             });
         }

         Ok(0)
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
    ) -> Result<i32, AnyhowError> {
         let host_env = caller.data();
         let dag_store = host_env.runtime_ctx.dag_store();
         let cid_str = self.read_string_from_mem(&mut caller, cid_ptr, cid_len)?;
         let cid = Cid::from_str(&cid_str).map_err(|_| anyhow!(HostAbiError::InvalidCIDFormat))?;

         // TODO: Check permissions if necessary (e.g., based on job context)

         let data_res = host_env.runtime_handle.block_on(async {
             dag_store.read_dag_node_async(&cid).await
         });

         match data_res {
             Ok(Some(data)) => {
                 let read_start = offset as usize;
                 if read_start >= data.len() {
                     return Ok(0); // Offset is beyond the data length
                 }
                 let read_end = (read_start + buffer_len as usize).min(data.len());
                 let bytes_to_read = read_end - read_start;
                 if bytes_to_read > 0 {
                     let slice_to_read = &data[read_start..read_end];
                     let mem = self.get_memory(&mut caller)?;
                     mem.write(&mut caller, buffer_ptr as usize, slice_to_read)
                         .map_err(|_| anyhow!(HostAbiError::MemoryAccessError))?;
                     Ok(bytes_to_read as i32)
                 } else {
                     Ok(0)
                 }
             }
             Ok(None) => Err(anyhow!(HostAbiError::NotFound)),
             Err(e) => Err(anyhow!(HostAbiError::StorageError).context(e)), // Wrap DagStoreError
         }
    }

    fn host_data_write_buffer(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        data_ptr: u32,
        data_len: u32,
        cid_buf_ptr: u32,
        cid_buf_len: u32,
    ) -> Result<i32, AnyhowError> {
         let host_env = caller.data();
         let dag_store = host_env.runtime_ctx.dag_store();
         let data_to_write = self.read_bytes_from_mem(&mut caller, data_ptr, data_len)?;

         // TODO: Check permissions if necessary
         // TODO: Check resource limits (data size)

         let cid_res = host_env.runtime_handle.block_on(async {
             dag_store.write_dag_node_async(&data_to_write).await
         });

         match cid_res {
             Ok(cid) => {
                 let cid_str = cid.to_string();
                 self.write_string_to_mem(&mut caller, &cid_str, cid_buf_ptr, cid_buf_len)
             }
             Err(e) => Err(anyhow!(HostAbiError::StorageError).context(e)), // Wrap DagStoreError
         }
    }

    // **V. Logging **
    fn host_log_message(
        &self,
        mut caller: Caller<'_, ConcreteHostEnvironment>,
        level: LogLevel,
        message_ptr: u32,
        message_len: u32,
    ) -> Result<i32, AnyhowError> {
        let message = self.read_string_from_mem(&mut caller, message_ptr, message_len)?;
        
        // Use tracing crate macros
        match level {
            LogLevel::Error => tracing::error!(target: "wasm_guest", "{}", message),
            LogLevel::Warn => tracing::warn!(target: "wasm_guest", "{}", message),
            LogLevel::Info => tracing::info!(target: "wasm_guest", "{}", message),
            LogLevel::Debug => tracing::debug!(target: "wasm_guest", "{}", message),
            LogLevel::Trace => tracing::trace!(target: "wasm_guest", "{}", message),
        }
        Ok(0)
    }

    // ---------------- Mana stubs ----------------

    fn host_account_get_mana(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, did_ptr: u32, did_len: u32) -> Result<i64, AnyhowError> {
        // If explicit DID is provided we use individual scope for that DID; otherwise default scope_key()
        let scope_key = if did_len == 0 {
            self.scope_key()
        } else {
            let did = self.read_string_from_mem(&mut caller, did_ptr, did_len)?;
            ScopeKey::Individual(did)
        };

        let mut mana_mgr = self.rt.mana_manager.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        let balance_opt = mana_mgr.balance(&scope_key);
        Ok(balance_opt.unwrap_or(0) as i64)
    }

    fn host_account_spend_mana(&self, mut caller: Caller<'_, ConcreteHostEnvironment>, did_ptr: u32, did_len: u32, amount: u64) -> Result<i32, AnyhowError> {
        let scope_key = if did_len == 0 {
            self.scope_key()
        } else {
            let did = self.read_string_from_mem(&mut caller, did_ptr, did_len)?;
            ScopeKey::Individual(did)
        };

        let mut mana_mgr = self.rt.mana_manager.lock().map_err(|_| anyhow!(HostAbiError::UnknownError))?;
        match mana_mgr.spend(&scope_key, amount) {
            Ok(_) => Ok(0),
            Err(_) => Ok(HostAbiError::ResourceLimitExceeded as i32),
        }
    }
}

#[cfg(not(feature = "full_host_abi"))]
impl MeshHostAbi<ConcreteHostEnvironment> for ConcreteHostEnvironment {
    fn host_job_get_id(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _ptr: u32, _len: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_job_get_initial_input_cid(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _ptr: u32, _len: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_job_is_interactive(&self, _caller: Caller<'_, ConcreteHostEnvironment>) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_workflow_get_current_stage_index(&self, _caller: Caller<'_, ConcreteHostEnvironment>) -> Result<i32, AnyhowError> { Ok(-1) }

    fn host_workflow_get_current_stage_id(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _ptr: u32, _len: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_workflow_get_current_stage_input_cid(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _key_ptr: u32, _key_len: u32, _cid_ptr: u32, _cid_len: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_job_report_progress(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _pct: u8, _msg_ptr: u32, _msg_len: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_workflow_complete_current_stage(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _cid_ptr: u32, _cid_len: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_interactive_send_output(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _payload_ptr: u32, _payload_len: u32, _key_ptr: u32, _key_len: u32, _is_final: i32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_interactive_receive_input(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _buffer_ptr: u32, _buffer_len: u32, _timeout_ms: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_interactive_peek_input_len(&self, _caller: Caller<'_, ConcreteHostEnvironment>) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_interactive_prompt_for_input(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _ptr: u32, _len: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_data_read_cid(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _cid_ptr: u32, _cid_len: u32, _offset: u64, _buffer_ptr: u32, _buffer_len: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_data_write_buffer(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _data_ptr: u32, _data_len: u32, _cid_buf_ptr: u32, _cid_buf_len: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_log_message(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _level: LogLevel, _ptr: u32, _len: u32) -> Result<i32, AnyhowError> { Ok(0) }

    fn host_account_get_mana(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _did_ptr: u32, _did_len: u32) -> Result<i64, AnyhowError> { Ok(0) }

    fn host_account_spend_mana(&self, _caller: Caller<'_, ConcreteHostEnvironment>, _did_ptr: u32, _did_len: u32, _amount: u64) -> Result<i32, AnyhowError> { Ok(0) }
} 