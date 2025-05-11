use crate::context::RuntimeContext;
use icn_economics::ResourceType;
use icn_identity::Did;
use icn_mesh_receipts::{ExecutionReceipt, verify_embedded_signature, SignError as ReceiptSignError};
use icn_types::dag::ReceiptNode;
use icn_types::dag_store::DagStore;
use icn_types::org::{CooperativeId, CommunityId};
use serde_cbor;
use std::sync::Arc;
use std::str::FromStr;
use anyhow::Result;
use thiserror::Error;
use host_abi::*;
use crate::job_execution_context::{JobExecutionContext, JobPermissions};
use icn_types::mesh::{JobId, MeshJobParams};
use planetary_mesh::protocol::{JobInteractiveInputV1, JobInteractiveOutputV1, MeshProtocolMessage};
use planetary_mesh::JobStatus as P2PJobStatus;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wasmer::{Memory, WasmerEnv, FunctionEnvMut, WasmPtr, Array};

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
pub struct ConcreteHostEnvironment {
    /// Per‐job execution context (for WASM calls)
    pub ctx: Arc<Mutex<JobExecutionContext>>,
    /// Global runtime context, including the pending_mesh_jobs queue
    pub rt: Arc<RuntimeContext>,
    
    /// DID of the caller
    pub caller_did: Did,
    
    /// Whether this execution is happening in a governance context
    pub is_governance: bool,
    
    /// Optional cooperative ID for this execution context
    pub coop_id: Option<CooperativeId>,
    
    /// Optional community ID for this execution context
    pub community_id: Option<CommunityId>,
    
    /// In a real system, these would be Arcs to actual service implementations
    pub p2p_service: Arc<dyn P2pService>,
    pub storage_service: Arc<dyn StorageService>,
}

impl ConcreteHostEnvironment {
    /// Create a new host environment with the given context and caller
    pub fn new(
        ctx: Arc<Mutex<JobExecutionContext>>,
        p2p_service: Arc<dyn P2pService>,
        storage_service: Arc<dyn StorageService>,
        caller_did: Did,
        runtime_ctx: Arc<RuntimeContext>,
    ) -> Self {
        Self {
            ctx,
            rt: runtime_ctx,
            p2p_service,
            storage_service,
            caller_did,
            is_governance: false,
            coop_id: None,
            community_id: None,
        }
    }
    
    /// Create a new host environment with governance context
    pub fn new_governance(ctx: Arc<Mutex<JobExecutionContext>>, caller_did: Did, runtime_ctx: Arc<RuntimeContext>) -> Self {
        Self {
            ctx,
            rt: runtime_ctx,
            caller_did,
            is_governance: true,
            coop_id: None,
            community_id: None,
            p2p_service: Arc::new(|target_did: Did, message: MeshProtocolMessage| -> Result<(), String> {
                Err(String::from("Governance context does not support P2P communication"))
            }),
            storage_service: Arc::new(|data: &[u8]| -> Result<String, String> {
                Err(String::from("Governance context does not support storage"))
            }),
        }
    }
    
    /// Create a new host environment with organization context
    pub fn with_organization(
        mut self,
        coop_id: Option<CooperativeId>,
        community_id: Option<CommunityId>,
    ) -> Self {
        self.coop_id = coop_id;
        self.community_id = community_id;
        self
    }

    /// Check resource authorization
    pub fn check_resource_authorization(&self, rt: ResourceType, amt: u64) -> i32 {
        self.ctx.lock().unwrap().economics.authorize(&self.caller_did, self.coop_id.as_ref(), self.community_id.as_ref(), rt, amt)
    }

    /// Record resource usage
    pub async fn record_resource_usage(&self, rt: ResourceType, amt: u64) -> i32 {
        self.ctx.lock().unwrap().economics.record(
            &self.caller_did,
            self.coop_id.as_ref(),
            self.community_id.as_ref(),
            rt,
            amt,
            &self.ctx.lock().unwrap().resource_ledger
        ).await
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
    pub async fn mint_token(&self, recipient_did_str: &str, amount: u64) -> i32 {
        // Only allow minting in a governance context
        if !self.is_governance {
            return -1; // Not authorized
        }
        
        // Parse the recipient DID
        let recipient_did = match Did::from_str(recipient_did_str) {
            Ok(did) => did,
            Err(_) => return -2, // Invalid DID
        };
        
        // Record the minted tokens as a negative usage (increases allowance)
        self.ctx.lock().unwrap().economics.mint(
            &recipient_did,
            self.coop_id.as_ref(),
            self.community_id.as_ref(),
            ResourceType::Token,
            amount,
            &self.ctx.lock().unwrap().resource_ledger
        ).await
    }
    
    /// Transfer tokens from sender to recipient
    /// Returns:
    /// - 0 on success
    /// - -1 on insufficient funds
    /// - -2 on invalid DID
    pub async fn transfer_token(&self, sender_did_str: &str, recipient_did_str: &str, amount: u64) -> i32 {
        // Parse the sender DID
        let sender_did = match Did::from_str(sender_did_str) {
            Ok(did) => did,
            Err(_) => return -2, // Invalid sender DID
        };
        
        // Parse the recipient DID
        let recipient_did = match Did::from_str(recipient_did_str) {
            Ok(did) => did,
            Err(_) => return -2, // Invalid recipient DID
        };
        
        // Transfer tokens between DIDs, using the same org context for both sender and recipient
        self.ctx.lock().unwrap().economics.transfer(
            &sender_did,
            self.coop_id.as_ref(),
            self.community_id.as_ref(),
            &recipient_did,
            self.coop_id.as_ref(),
            self.community_id.as_ref(),
            ResourceType::Token,
            amount,
            &self.ctx.lock().unwrap().resource_ledger
        ).await
    }

    /// Anchor a serialized ExecutionReceipt into the DAG.
    pub async fn anchor_receipt(&self, mut receipt: ExecutionReceipt) -> Result<(), AnchorError> {
        // 1. Verify the receipt is from the caller
        if receipt.executor != self.caller_did {
            return Err(AnchorError::ExecutorMismatch(
                receipt.executor.to_string(),
                self.caller_did.to_string()
            ));
        }
        
        // 2. Add organizational context if not already set
        if receipt.coop_id.is_none() && self.coop_id.is_some() {
            receipt.coop_id = self.coop_id.clone();
        }
        
        if receipt.community_id.is_none() && self.community_id.is_some() {
            receipt.community_id = self.community_id.clone();
        }
        
        // 3. Verify the receipt signature
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
        
        // TODO: Economic recording step (Phase 3/4)
        // If the receipt contains verified resource usage, this could trigger an update
        // in the icn-economics ledger.

        // 4. Generate CID for the (now verified and signed) receipt
        let receipt_cid = receipt.cid()
            .map_err(|e| AnchorError::CidError(e.to_string()))?;
        
        // 5. Get federation ID
        let federation_id = self.ctx.lock().unwrap().federation_id.clone()
            .ok_or(AnchorError::MissingFederationId)?;
        
        // 6. Serialize receipt to CBOR
        let receipt_cbor = serde_cbor::to_vec(&receipt)
            .map_err(|e| AnchorError::SerializationError(e.to_string()))?;
        
        // 7. Create a ReceiptNode
        let receipt_node = ReceiptNode::new(
            receipt_cid, 
            receipt_cbor, 
            federation_id
        );
        
        // 8. Create a DAG node from the receipt node
        let dag_node = icn_types::dag::DagNodeBuilder::new()
            .content(serde_json::to_string(&receipt_node)
                .map_err(|e| AnchorError::SerializationError(e.to_string()))?)
            .event_type(icn_types::dag::DagEventType::Receipt)
            .scope_id(format!("receipt/{}", receipt_cid))
            .timestamp(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as u64)
            .build()
            .map_err(|e| AnchorError::DagStoreError(e.to_string()))?;
        
        // 9. Insert into receipt store
        self.ctx.lock().unwrap().receipt_store.insert(dag_node)
            .await
            .map_err(|e| AnchorError::DagStoreError(e.to_string()))?;
        
        // Log success
        tracing::info!("Anchored receipt for job: {}, executor: {}, receipt CID: {}", 
            receipt.job_id, receipt.executor, receipt_cid);
        
        Ok(())
    }
}

// This is how you'd implement the ABI trait for the environment.
// The `env: FunctionEnvMut<Self>` gives access to `ConcreteHostEnvironment` and WASM memory.
impl MeshHostAbi for ConcreteHostEnvironment {
    // **I. Job & Workflow Information **
    fn host_job_get_id(&self, job_id_buf_ptr: u32, job_id_buf_len: u32) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        if job_id_buf_len < ctx.job_id.len() as u32 {
            return HostAbiError::BufferTooSmall as i32;
        }
        let job_id_bytes = ctx.job_id.as_bytes();
        let wasm_ptr = WasmPtr::<u8, Array>::new(job_id_buf_ptr);
        match wasm_ptr.get_slice_mut(env.data().memory, job_id_bytes.len() as u32) {
            Some(mut dest_slice) => {
                dest_slice.copy_from_slice(job_id_bytes);
                job_id_bytes.len() as i32
            }
            None => HostAbiError::InvalidArguments as i32,
        }
    }

    fn host_job_get_initial_input_cid(&self, cid_buf_ptr: u32, cid_buf_len: u32) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        if let Some(input_cid) = &ctx.job_params.input_data_cid {
            if cid_buf_len < input_cid.len() as u32 {
                return HostAbiError::BufferTooSmall as i32;
            }
            let input_cid_bytes = input_cid.as_bytes();
            let wasm_ptr = WasmPtr::<u8, Array>::new(cid_buf_ptr);
            match wasm_ptr.get_slice_mut(env.data().memory, input_cid_bytes.len() as u32) {
                Some(mut dest_slice) => {
                    dest_slice.copy_from_slice(input_cid_bytes);
                    input_cid_bytes.len() as i32
                }
                None => HostAbiError::InvalidArguments as i32,
            }
        } else {
            0
        }
    }

    fn host_job_is_interactive(&self) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        if ctx.job_params.is_interactive { 1 } else { 0 }
    }

    fn host_workflow_get_current_stage_index(&self) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        ctx.current_stage_index.map_or(-1, |idx| idx as i32)
    }

    fn host_workflow_get_current_stage_id(&self, stage_id_buf_ptr: u32, stage_id_buf_len: u32) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        if let Some(stage_id) = &ctx.current_stage_id {
            if stage_id_buf_len < stage_id.len() as u32 {
                return HostAbiError::BufferTooSmall as i32;
            }
            let stage_id_bytes = stage_id.as_bytes();
            let wasm_ptr = WasmPtr::<u8, Array>::new(stage_id_buf_ptr);
            match wasm_ptr.get_slice_mut(env.data().memory, stage_id_bytes.len() as u32) {
                Some(mut dest_slice) => {
                    dest_slice.copy_from_slice(stage_id_bytes);
                    stage_id_bytes.len() as i32
                }
                None => HostAbiError::InvalidArguments as i32,
            }
        } else {
            0
        }
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
        payload_ptr: u32, 
        payload_len: u32, 
        output_key_ptr: u32, 
        output_key_len: u32, 
        is_final_chunk: i32
    ) -> i32 {
        let mut ctx = self.ctx.lock().unwrap();

        if !ctx.permissions.can_send_interactive_output {
            return HostAbiError::NotPermitted as i32;
        }
        if !ctx.job_params.is_interactive {
            return HostAbiError::InvalidState as i32; // Cannot send on non-interactive job
        }

        let payload_data = vec![0u8; payload_len as usize]; // Dummy payload
        let output_key = if output_key_len > 0 { Some(format!("key_ptr_{}_{}", output_key_ptr, output_key_len)) } else { None };

        let (payload_cid, payload_inline) = if payload_len as usize > INLINE_PAYLOAD_MAX_SIZE {
            match self.storage_service.store_data(&payload_data) {
                Ok(cid) => (Some(cid), None),
                Err(_) => return HostAbiError::StorageError as i32,
            }
        } else {
            (None, Some(payload_data.to_vec()))
        };

        ctx.interactive_output_sequence_num += 1;
        let message = JobInteractiveOutputV1 {
            job_id: ctx.job_id.clone(),
            executor_did: "did:ethr:executor_node_placeholder".to_string(), // Should be this node's DID
            target_originator_did: ctx.originator_did.clone(),
            sequence_num: ctx.interactive_output_sequence_num,
            payload_cid,
            payload_inline,
            is_final_chunk: is_final_chunk == 1,
            output_key,
        };

        match self.p2p_service.send_p2p_message(ctx.originator_did.clone(), MeshProtocolMessage::JobInteractiveOutputV1(message)) {
            Ok(_) => HostAbiError::Success as i32,
            Err(_) => HostAbiError::NetworkError as i32,
        }
    }

    fn host_interactive_receive_input(
        &self, 
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

                let wasm_ptr = WasmPtr::<u8, Array>::new(buffer_ptr);
                match wasm_ptr.get_slice_mut(env.data().memory, info_bytes.len() as u32) {
                    Some(mut dest_slice) => {
                        dest_slice.copy_from_slice(&info_bytes);
                        total_written += info_bytes.len() as u32;
                    }
                    None => return HostAbiError::InvalidArguments as i32,
                }

                match wasm_ptr.get_slice_mut(env.data().memory, data_bytes.len() as u32) {
                    Some(mut dest_slice) => {
                        dest_slice.copy_from_slice(&data_bytes);
                        total_written += data_bytes.len() as u32;
                    }
                    None => return HostAbiError::InvalidArguments as i32,
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
    fn host_log_message(&self, level: LogLevel, message_ptr: u32, message_len: u32) -> i32 {
        let ctx = self.ctx.lock().unwrap();
        if level as u32 > ctx.permissions.max_log_level_allowed as u32 {
            return HostAbiError::NotPermitted as i32; // Log level too verbose for job's permissions
        }
        let message = format!("WASM_LOG L{:?} (ptr/len {}/{}): ...", level, message_ptr, message_len);
        println!("{}", message); // Host logs it
        HostAbiError::Success as i32
    }
} 