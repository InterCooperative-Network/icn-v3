use crate::context::RuntimeContext;
use icn_economics::ResourceType;
use icn_identity::Did;
use icn_mesh_receipts::{ExecutionReceipt, verify_embedded_signature, SignError as ReceiptSignError};
use icn_types::dag::ReceiptNode;
use icn_types::dag_store::DagStore;
use icn_types::org::{CooperativeId, CommunityId};
use icn_mesh_protocol::{JobInteractiveInputV1, JobInteractiveOutputV1, MeshProtocolMessage, P2PJobStatus, INLINE_PAYLOAD_MAX_SIZE, MAX_INTERACTIVE_INPUT_BUFFER_PEEK};
use serde::{Serialize};
use serde_cbor;
use std::sync::{Arc, Mutex};
use std::str::FromStr;
use anyhow::Result;
use thiserror::Error;
use host_abi::*;
use crate::job_execution_context::{JobExecutionContext, JobPermissions};
use icn_types::mesh::MeshJobParams;
use std::time::{Duration, Instant};
use wasmer::{Memory, WasmerEnv as WasmerEnvTrait, FunctionEnv, FunctionEnvMut, WasmPtr, Array, MemoryView};
use wasmer_derive::WasmerEnv;
use tracing;

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
#[derive(WasmerEnv, Clone)]
pub struct ConcreteHostEnvironment {
    #[wasmer(export)]
    pub ctx: Arc<Mutex<JobExecutionContext>>,
    #[wasmer(export)]
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

// ABI Implementation using Wasmer
impl MeshHostAbi for ConcreteHostEnvironment {
    // Helper to get memory view
    fn get_memory_view<'a>(&self, env: &'a FunctionEnvMut<Self>) -> Result<MemoryView<'a, u8>, HostAbiError> {
        env.data().rt.memory.as_ref()
            .ok_or(HostAbiError::MemoryAccessError)?
            .view(env)
    }

    fn host_job_get_id(&self, env: &mut FunctionEnvMut<Self>, job_id_buf_ptr: u32, job_id_buf_len: u32) -> i32 {
        let ctx = match env.data().ctx.lock() { Ok(l) => l, Err(_) => return HostAbiError::UnknownError as i32 };
        let job_id_bytes = ctx.job_id.as_bytes();
        if job_id_buf_len < job_id_bytes.len() as u32 { return HostAbiError::BufferTooSmall as i32; }
        
        match self.get_memory_view(env) {
            Ok(memory_view) => {
                let wasm_ptr = WasmPtr::<u8, Array>::new(job_id_buf_ptr);
                match wasm_ptr.slice(&memory_view, job_id_bytes.len() as u32) {
                    Ok(mut dest_slice) => {
                        match dest_slice.write_slice(job_id_bytes) {
                            Ok(_) => job_id_bytes.len() as i32,
                            Err(_) => HostAbiError::MemoryAccessError as i32,
                        }
                    }
                    Err(_) => HostAbiError::MemoryAccessError as i32, // Error getting slice
                }
            }
            Err(e) => e as i32
        }
    }

    fn host_job_get_initial_input_cid(&self, env: &mut FunctionEnvMut<Self>, cid_buf_ptr: u32, cid_buf_len: u32) -> i32 {
        let ctx = match env.data().ctx.lock() { Ok(l) => l, Err(_) => return HostAbiError::UnknownError as i32 };
        if let Some(input_cid) = &ctx.job_params.input_data_cid {
            let input_cid_bytes = input_cid.as_bytes();
            if cid_buf_len < input_cid_bytes.len() as u32 { return HostAbiError::BufferTooSmall as i32; }
            match self.get_memory_view(env) {
                 Ok(memory_view) => {
                    let wasm_ptr = WasmPtr::<u8, Array>::new(cid_buf_ptr);
                    match wasm_ptr.slice(&memory_view, input_cid_bytes.len() as u32) {
                        Ok(mut dest_slice) => {
                            match dest_slice.write_slice(input_cid_bytes) {
                                Ok(_) => input_cid_bytes.len() as i32,
                                Err(_) => HostAbiError::MemoryAccessError as i32,
                            }
                        }
                        Err(_) => HostAbiError::MemoryAccessError as i32,
                    }
                }
                Err(e) => e as i32
            }
        } else { 0 }
    }

    fn host_job_is_interactive(&self, env: &mut FunctionEnvMut<Self>) -> i32 {
        let ctx = match env.data().ctx.lock() { Ok(l) => l, Err(_) => return HostAbiError::UnknownError as i32 };
        if ctx.job_params.is_interactive { 1 } else { 0 }
    }

    fn host_workflow_get_current_stage_index(&self, env: &mut FunctionEnvMut<Self>) -> i32 {
        let ctx = match env.data().ctx.lock() { Ok(l) => l, Err(_) => return HostAbiError::UnknownError as i32 };
        ctx.current_stage_index.map_or(-1, |idx| idx as i32)
    }

    fn host_workflow_get_current_stage_id(&self, env: &mut FunctionEnvMut<Self>, stage_id_buf_ptr: u32, stage_id_buf_len: u32) -> i32 {
    /// Check resource authorization
    pub fn check_resource_authorization(&self, rt_type: ResourceType, amt: u64) -> i32 {
        if let Some(economics) = &self.rt.economics {
            economics.authorize(
                &self.caller_did, 
                self.coop_id.as_ref(), 
                self.community_id.as_ref(), 
                rt_type, 
                amt
            )
        } else {
            -1
        }
    }

    /// Record resource usage
    pub fn record_resource_usage(&self, rt_type: ResourceType, amt: u64) -> i32 {
        if let Some(economics) = &self.rt.economics {
            0
        } else {
            -1
        }
    }
    
    /// Check if the current execution is in a governance context
    pub fn is_governance_context(&self) -> i32 {
        if self.is_governance {
            1
        } else {
            0
        }
    }
    
    /// Mint tokens for a specific DID, only allowed in governance context
    pub fn mint_token(&self, recipient_did_str: &str, amount: u64) -> i32 {
        if !self.is_governance {
            return -1;
        }
        
        let recipient_did = match Did::from_str(recipient_did_str) {
            Ok(did) => did,
            Err(_) => return -2,
        };
        
        if let Some(economics) = &self.rt.economics {
            0
        } else {
            -1
        }
    }
    
    /// Transfer tokens from sender to recipient
    /// Returns:
    /// - 0 on success
    /// - -1 on insufficient funds
    /// - -2 on invalid DID
    pub fn transfer_token(&self, sender_did_str: &str, recipient_did_str: &str, amount: u64) -> i32 {
        let sender_did = match Did::from_str(sender_did_str) {
            Ok(did) => did,
            Err(_) => return -2,
        };
        
        let recipient_did = match Did::from_str(recipient_did_str) {
            Ok(did) => did,
            Err(_) => return -2,
        };
        
        if let Some(economics) = &self.rt.economics {
            0
        } else {
            -1
        }
    }

    /// Anchor a serialized ExecutionReceipt into the DAG.
    pub async fn anchor_receipt(&self, mut receipt: ExecutionReceipt) -> Result<(), AnchorError> {
        if receipt.executor != self.caller_did {
            return Err(AnchorError::ExecutorMismatch(
                receipt.executor.to_string(),
                self.caller_did.to_string()
            ));
        }
        
        if receipt.coop_id.is_none() && self.coop_id.is_some() {
            receipt.coop_id = self.coop_id.clone();
        }
        
        if receipt.community_id.is_none() && self.community_id.is_some() {
            receipt.community_id = self.community_id.clone();
        }
        
        if receipt.signature.is_empty() {
            return Err(AnchorError::InvalidSignature("Receipt has no signature.".to_string()));
        }

        match verify_embedded_signature(&receipt) {
            Ok(true) => {
                tracing::debug!("Receipt signature verified successfully for executor {}", receipt.executor);
            }
            Ok(false) => {
                return Err(AnchorError::InvalidSignature("Receipt signature verification failed.".to_string()));
            }
            Err(e) => {
                return Err(AnchorError::InvalidSignature(format!("Error during signature verification: {}", e)));
            }
        }
        
        let receipt_cid = receipt.cid().map_err(|e| AnchorError::CidError(e.to_string()))?;
        
        let federation_id = self.rt.federation_id.clone().ok_or(AnchorError::MissingFederationId)?;
        
        let receipt_cbor = serde_cbor::to_vec(&receipt).map_err(|e| AnchorError::SerializationError(e.to_string()))?;
        
        let receipt_node = ReceiptNode::new(receipt_cid.clone(), receipt_cbor, federation_id);
        
        let dag_node = icn_types::dag::DagNodeBuilder::new()
            .content(serde_json::to_string(&receipt_node).map_err(|e| AnchorError::SerializationError(e.to_string()))?)
            .event_type(icn_types::dag::DagEventType::Receipt)
            .scope_id(format!("receipt/{}", receipt_cid))
            .timestamp(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as u64)
            .build()
            .map_err(|e| AnchorError::DagStoreError(e.to_string()))?;
        
        self.rt.dag_store.insert(dag_node).await.map_err(|e| AnchorError::DagStoreError(e.to_string()))?;
        
        tracing::info!("Anchored receipt for job: {}, executor: {}, receipt CID: {}", 
            receipt.job_id, receipt.executor, receipt_cid);
        
        Ok(())
    }
}

// This is how you'd implement the ABI trait for the environment.
// The `env: FunctionEnvMut<Self>` gives access to `ConcreteHostEnvironment` and WASM memory.
impl MeshHostAbi for ConcreteHostEnvironment {
    // Get memory helper
    fn get_memory<'a>(&self, env: &'a FunctionEnvMut<Self>) -> Result<MemoryView<'a, u8>, HostAbiError> {
        env.data().rt.memory.as_ref()
            .ok_or(HostAbiError::MemoryAccessError)?
            .view(env)
            .ok_or(HostAbiError::MemoryAccessError)
    }

    fn host_job_get_id(&self, env: &mut FunctionEnvMut<Self>, job_id_buf_ptr: u32, job_id_buf_len: u32) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        let job_id_bytes = ctx.job_id.as_bytes();
        if job_id_buf_len < job_id_bytes.len() as u32 { return HostAbiError::BufferTooSmall as i32; }
        
        match self.get_memory(env) {
            Ok(memory_view) => {
                let wasm_ptr = WasmPtr::<u8, Array>::new(job_id_buf_ptr);
                match wasm_ptr.slice(&memory_view, job_id_bytes.len() as u32) {
                    Ok(mut dest_slice) => {
                        dest_slice.write_slice(job_id_bytes).map_err(|_| HostAbiError::MemoryAccessError)?;
                        job_id_bytes.len() as i32
                    }
                    Err(_) => HostAbiError::InvalidArguments as i32,
                }
            }
            Err(e) => e as i32
        }
    }

    fn host_job_get_initial_input_cid(&self, env: &mut FunctionEnvMut<Self>, cid_buf_ptr: u32, cid_buf_len: u32) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        if let Some(input_cid) = &ctx.job_params.input_data_cid {
            let input_cid_bytes = input_cid.as_bytes();
            if cid_buf_len < input_cid_bytes.len() as u32 { return HostAbiError::BufferTooSmall as i32; }
            match self.get_memory(env) {
                 Ok(memory_view) => {
                    let wasm_ptr = WasmPtr::<u8, Array>::new(cid_buf_ptr);
                    match wasm_ptr.slice(&memory_view, input_cid_bytes.len() as u32) {
                        Ok(mut dest_slice) => {
                            dest_slice.write_slice(input_cid_bytes).map_err(|_| HostAbiError::MemoryAccessError)?;
                            input_cid_bytes.len() as i32
                        }
                        Err(_) => HostAbiError::InvalidArguments as i32,
                    }
                }
                Err(e) => e as i32
            }
        } else { 0 }
    }

    fn host_job_is_interactive(&self) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        if ctx.job_params.is_interactive { 1 } else { 0 }
    }

    fn host_workflow_get_current_stage_index(&self) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        ctx.current_stage_index.map_or(-1, |idx| idx as i32)
    }

    fn host_workflow_get_current_stage_id(&self, env: &mut FunctionEnvMut<Self>, stage_id_buf_ptr: u32, stage_id_buf_len: u32) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        if let Some(stage_id) = &ctx.current_stage_id {
            let stage_id_bytes = stage_id.as_bytes();
            if stage_id_buf_len < stage_id_bytes.len() as u32 { return HostAbiError::BufferTooSmall as i32; }
            match self.get_memory(env) {
                Ok(memory_view) => {
                    let wasm_ptr = WasmPtr::<u8, Array>::new(stage_id_buf_ptr);
                    match wasm_ptr.slice(&memory_view, stage_id_bytes.len() as u32) {
                        Ok(mut dest_slice) => {
                            dest_slice.write_slice(stage_id_bytes).map_err(|_| HostAbiError::MemoryAccessError)?;
                            stage_id_bytes.len() as i32
                        }
                        Err(_) => HostAbiError::InvalidArguments as i32,
                    }
                }
                Err(e) => e as i32
            }
        } else { 0 }
    }

    fn host_workflow_get_current_stage_input_cid(&self, input_key_ptr: u32, input_key_len: u32, cid_buf_ptr: u32, cid_buf_len: u32) -> i32 {
        // Complex logic: needs to access ctx.job_params.stages, current_stage_index,
        // potentially read input_key from WASM memory, resolve StageInputSource.
        println!("[ABI] host_workflow_get_current_stage_input_cid called. Needs full implementation.");
        HostAbiError::NotSupported as i32
    }

    // **II. Status & Progress Reporting **
    fn host_job_report_progress(&self, percentage: u8, status_message_ptr: u32, status_message_len: u32) -> i32 {
        // Needs to read status_message from WASM, update ctx.current_status, potentially send P2P update.
        let mut ctx = self.ctx.lock().unwrap();
        let message = format!("Progress: {}% (message ptr/len {}/{})", percentage, status_message_ptr, status_message_len);
        
        println!("[ABI] host_job_report_progress: {}", message);
        // Update JobExecutionContext status (simplified)
        if let P2PJobStatus::Running { progress_percent, status_message, .. } = &mut ctx.current_status {
            *progress_percent = Some(percentage);
            *status_message = Some(message); // In reality, read from WASM
        } else {
            // Or update to Running if it wasn't already
        }
        // Potentially trigger p2p_service.send_job_status_update(...)
        HostAbiError::Success as i32
    }

    fn host_workflow_complete_current_stage(&self, output_cid_ptr: u32, output_cid_len: u32) -> i32 {
        println!("[ABI] host_workflow_complete_current_stage called. Needs full implementation.");
        HostAbiError::NotSupported as i32
    }

    // **III. Interactivity **
    fn host_interactive_send_output(
        &self, 
        env: &mut FunctionEnvMut<Self>,
        payload_ptr: u32, 
        payload_len: u32, 
        output_key_ptr: u32, 
        output_key_len: u32, 
        is_final_chunk: i32
    ) -> i32 {
        let mut ctx = self.ctx.lock().unwrap();
        if !ctx.permissions.can_send_interactive_output { return HostAbiError::NotPermitted as i32; }
        if !ctx.job_params.is_interactive { return HostAbiError::InvalidState as i32; }

        let payload_data = match self.get_memory(env) {
            Ok(mem) => WasmPtr::<u8, Array>::new(payload_ptr).read_vec(&mem, payload_len).map_err(|_| HostAbiError::MemoryAccessError)?,
            Err(e) => return e as i32,
        };
        let output_key = if output_key_len > 0 {
             match self.get_memory(env) {
                 Ok(mem) => Some(WasmPtr::<u8, Array>::new(output_key_ptr).read_utf8_string(&mem, output_key_len).map_err(|_| HostAbiError::MemoryAccessError)?),
                 Err(e) => return e as i32,
             }
        } else { None };

        let storage_service = self.rt.storage_service.clone();
        let p2p_service = self.rt.p2p_service.clone();

        let (payload_cid, payload_inline) = if payload_len as usize > INLINE_PAYLOAD_MAX_SIZE {
            if let Some(storage) = storage_service {
                return HostAbiError::NotSupported as i32;
            } else {
                return HostAbiError::StorageError as i32;
            }
        } else {
            (None, Some(payload_data))
        };

        ctx.interactive_output_sequence_num += 1;
        let message = JobInteractiveOutputV1 {
            job_id: ctx.job_id.clone(),
            executor_did: "did:ethr:executor_node_placeholder".to_string(),
            target_originator_did: ctx.originator_did.clone(),
            sequence_num: ctx.interactive_output_sequence_num,
            payload_cid,
            payload_inline,
            is_final_chunk: is_final_chunk == 1,
            output_key,
        };

        if let Some(p2p) = p2p_service {
            HostAbiError::NotSupported as i32
        } else {
            HostAbiError::NetworkError as i32
        }
    }

    fn host_interactive_receive_input(
        &self, 
        env: &mut FunctionEnvMut<Self>,
        buffer_ptr: u32, 
        buffer_len: u32, 
        timeout_ms: u32
    ) -> i32 {
        let start_time = Instant::now();
        loop {
            let mut ctx = self.ctx.lock().unwrap();

            if !ctx.job_params.is_interactive {
                return HostAbiError::InvalidState as i32;
            }
            if !matches!(ctx.current_status, P2PJobStatus::Running {..} | P2PJobStatus::PendingUserInput {..}) {
                 // Only allow receiving input if running or specifically pending input
                return HostAbiError::InvalidState as i32;
            }

            if let Some(input_msg) = ctx.interactive_input_queue.pop_front() {
                let mut total_written = 0;
                let received_info_size = std::mem::size_of::<ReceivedInputInfo>() as u32;

                let (input_type, data_for_wasm, actual_data_len) = if let Some(cid) = input_msg.payload_cid {
                    (ReceivedInputType::Cid, cid.into_bytes(), cid.len() as u32)
                } else if let Some(inline_data) = input_msg.payload_inline {
                    (ReceivedInputType::InlineData, inline_data, inline_data.len() as u32)
                } else {
                    // Empty input message, should ideally not happen if properly constructed
                    return HostAbiError::DataEncodingError as i32; 
                };

                if buffer_len < received_info_size + actual_data_len {
                    ctx.interactive_input_queue.push_front(input_msg); // Re-queue
                    return HostAbiError::BufferTooSmall as i32;
                }

                let info = ReceivedInputInfo {
                    input_type,
                    data_len: actual_data_len,
                };

                let info_bytes = serde_cbor::to_vec(&info).map_err(|e| HostAbiError::SerializationError)?;
                let data_bytes = serde_cbor::to_vec(&data_for_wasm).map_err(|e| HostAbiError::SerializationError)?;

                match self.get_memory(env) {
                    Ok(memory_view) => {
                        let wasm_ptr_info = WasmPtr::<u8, Array>::new(buffer_ptr);
                        let wasm_ptr_data = WasmPtr::<u8, Array>::new(buffer_ptr + info_bytes.len() as u32);
                        
                        match wasm_ptr_info.slice(&memory_view, info_bytes.len() as u32) {
                            Ok(mut dest_info) => {
                                dest_info.write_slice(&info_bytes).map_err(|_| HostAbiError::MemoryAccessError)?;
                                total_written += info_bytes.len() as u32;
                            }
                            Err(_) => return HostAbiError::InvalidArguments as i32,
                        }
                        match wasm_ptr_data.slice(&memory_view, data_bytes.len() as u32) {
                            Ok(mut dest_data) => {
                                dest_data.write_slice(&data_bytes).map_err(|_| HostAbiError::MemoryAccessError)?;
                                total_written += data_bytes.len() as u32;
                            }
                            Err(_) => return HostAbiError::InvalidArguments as i32,
                        }

                        // If job was PendingUserInput, transition it back to Running after consuming input
                        if matches!(ctx.current_status, P2PJobStatus::PendingUserInput {..}) {
                             ctx.current_status = P2PJobStatus::Running {
                                node_id: "did:ethr:executor_node_placeholder".to_string(), // This node's DID
                                current_stage_index: ctx.current_stage_index,
                                current_stage_id: ctx.current_stage_id.clone(),
                                progress_percent: Some(ctx.job_params.stages.as_ref().map_or(50, |s| if s.is_empty() {50} else { (ctx.current_stage_index.unwrap_or(0) * 100 / s.len() as u32) as u8 } )),
                                status_message: Some("Input received, resuming operation.".to_string()),
                            };
                            // Potentially send P2P status update
                        }

                        return total_written as i32;
                    }
                    Err(e) => return e as i32
                }
            }
            
            // Drop lock before sleep/yield to avoid deadlock
            drop(ctx);

            if timeout_ms == 0 { // Non-blocking call
                return 0; // No input, no error
            }

            if start_time.elapsed() >= Duration::from_millis(timeout_ms as u64) {
                return HostAbiError::Timeout as i32; // Or 0 if timeout means "no input yet"
            }

            // In a real async environment, this is where we would await on a Waker or channel.
            // For this synchronous sketch, we simulate a short sleep and retry.
            std::thread::sleep(Duration::from_millis(50)); // Simulate yielding/polling
        }
    }

    fn host_interactive_peek_input_len(&self) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        if let Some(input_msg) = ctx.interactive_input_queue.front() {
            let data_len = input_msg.payload_cid.as_ref().map_or(0, |cid| cid.len())
                         + input_msg.payload_inline.as_ref().map_or(0, |data| data.len());
            let total_size = std::mem::size_of::<ReceivedInputInfo>() + data_len;
            if total_size > MAX_INTERACTIVE_INPUT_BUFFER_PEEK {
                 MAX_INTERACTIVE_INPUT_BUFFER_PEEK as i32 // Cap reported size
            } else {
                total_size as i32
            }
        } else {
            0 // No input available
        }
    }

    fn host_interactive_prompt_for_input(&self, prompt_cid_ptr: u32, prompt_cid_len: u32) -> i32 {
        let mut ctx = self.ctx.lock().unwrap();
        if !ctx.job_params.is_interactive {
            return HostAbiError::NotPermitted as i32;
        }

        let prompt_cid = if prompt_cid_len > 0 { Some(format!("prompt_cid_ptr_{}_{}", prompt_cid_ptr, prompt_cid_len)) } else { None };

        ctx.current_status = P2PJobStatus::PendingUserInput {
            node_id: "did:ethr:executor_node_placeholder".to_string(), // This node's DID
            prompt_cid,
            // stage_index/id might be relevant if prompt is stage-specific
            current_stage_index: ctx.current_stage_index,
            current_stage_id: ctx.current_stage_id.clone(), 
        };
        // TODO: Send P2P JobStatusUpdateV1 message to originator
        println!("[ABI] host_interactive_prompt_for_input: Job {} now PendingUserInput.", ctx.job_id);
        HostAbiError::Success as i32
    }

    // **IV. Data Handling & Storage **
    fn host_data_read_cid(&self, cid_ptr: u32, cid_len: u32, offset: u64, buffer_ptr: u32, buffer_len: u32) -> i32 {
        // Needs to read CID string from WASM, check permissions in ctx, call storage_service.retrieve_data,
        // then write data to WASM buffer, handling offset and buffer_len.
        println!("[ABI] host_data_read_cid called. Needs full implementation with memory access and permission checks.");
        HostAbiError::NotSupported as i32
    }

    fn host_data_write_buffer(&self, data_ptr: u32, data_len: u32, cid_buf_ptr: u32, cid_buf_len: u32) -> i32 {
        // Needs to read data from WASM, check permissions, call storage_service.store_data,
        // then write resulting CID to WASM cid_buf_ptr.
        println!("[ABI] host_data_write_buffer called. Needs full implementation with memory access and permission checks.");
        HostAbiError::NotSupported as i32
    }

    // **V. Logging **
    fn host_log_message(&self, env: &mut FunctionEnvMut<Self>, level: LogLevel, message_ptr: u32, message_len: u32) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        if level as u32 > ctx.permissions.max_log_level_allowed as u32 {
            return HostAbiError::NotPermitted as i32; // Log level too verbose for job's permissions
        }
        
        let log_message = match self.get_memory(env) {
             Ok(mem) => WasmPtr::<u8, Array>::new(message_ptr).read_utf8_string(&mem, message_len).unwrap_or_else(|_| "<invalid UTF8>".to_string()),
             Err(_) => "<memory read error>".to_string(),
        };

        // Use tracing or log crate
        match level {
            LogLevel::Error => tracing::error!(target: "wasm_log", job_id=%ctx.job_id, "{}", log_message),
            LogLevel::Warn => tracing::warn!(target: "wasm_log", job_id=%ctx.job_id, "{}", log_message),
            LogLevel::Info => tracing::info!(target: "wasm_log", job_id=%ctx.job_id, "{}", log_message),
            LogLevel::Debug => tracing::debug!(target: "wasm_log", job_id=%ctx.job_id, "{}", log_message),
            LogLevel::Trace => tracing::trace!(target: "wasm_log", job_id=%ctx.job_id, "{}", log_message),
        }
        HostAbiError::Success as i32
    }
} 