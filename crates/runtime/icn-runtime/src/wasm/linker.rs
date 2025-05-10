use anyhow::Result;
use host_abi::*;
use icn_economics::ResourceType;
use icn_mesh_receipts::ExecutionReceipt;
use icn_types::mesh::{MeshJob, MeshJobParams};
use wasmtime::{Caller, Linker, Memory, ValType, Trap, Val, Extern};
use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Store data for the WASM engine, contains the host environment
pub struct StoreData {
    host_env: Option<RefCell<crate::host_environment::ConcreteHostEnvironment>>,
}

impl StoreData {
    /// Create a new store data instance
    pub fn new() -> Self {
        Self { host_env: None }
    }

    /// Set the host environment
    pub fn set_host(&mut self, host_env: crate::host_environment::ConcreteHostEnvironment) {
        self.host_env = Some(RefCell::new(host_env));
    }

    /// Get a reference to the host environment
    pub fn host(&self) -> std::cell::Ref<crate::host_environment::ConcreteHostEnvironment> {
        self.host_env.as_ref().expect("Host environment not set").borrow()
    }
    
    /// Get a mutable reference to the host environment
    pub fn host_mut(&self) -> std::cell::RefMut<crate::host_environment::ConcreteHostEnvironment> {
        self.host_env.as_ref().expect("Host environment not set").borrow_mut()
    }
}

/// Register all host functions for the economics module
pub fn register_host_functions(linker: &mut Linker<StoreData>) -> Result<(), anyhow::Error> {
    // Register the resource authorization check function
    linker.func_wrap(
        "icn_host", 
        "host_check_resource_authorization",
        |caller: Caller<'_, StoreData>, resource_type: u32, amount: u64| -> i32 {
            let rt: ResourceType = resource_type.into();
            caller.data().host().check_resource_authorization(rt, amount)
        },
    )?;

    // Register the resource usage recording function
    linker.func_wrap(
        "icn_host", 
        "host_record_resource_usage",
        |caller: Caller<'_, StoreData>, resource_type: u32, amount: u64| -> i32 {
            let rt: ResourceType = resource_type.into();
            let host_env = caller.data().host();
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(host_env.record_resource_usage(rt, amount))
        },
    )?;
    
    // Register the governance context check function
    linker.func_wrap(
        "icn_host",
        "host_is_governance_context",
        |caller: Caller<'_, StoreData>| -> i32 {
            caller.data().host().is_governance_context()
        },
    )?;
    
    // Register the mint token function
    linker.func_wrap(
        "icn_host",
        "host_mint_token",
        |mut caller: Caller<'_, StoreData>, recipient_ptr: i32, recipient_len: i32, amount: u64| -> i32 {
            // Read the recipient DID string from WASM memory
            let memory = caller.get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("WASM module must export memory");
            
            let mut buffer = vec![0u8; recipient_len as usize];
            if memory.read(&mut caller, recipient_ptr as usize, &mut buffer).is_err() {
                return -2; // Memory read error
            }
            
            // Convert to UTF-8 string
            match String::from_utf8(buffer) {
                Ok(recipient_did) => {
                    let host_env = caller.data().host_mut();
                    let runtime = tokio::runtime::Runtime::new().unwrap();
                    runtime.block_on(host_env.mint_token(&recipient_did, amount))
                },
                Err(_) => -2, // Invalid UTF-8
            }
        },
    )?;
    
    // Register the transfer token function
    linker.func_wrap(
        "icn_host",
        "host_transfer_token",
        |mut caller: Caller<'_, StoreData>, sender_ptr: i32, sender_len: i32, 
                                       recipient_ptr: i32, recipient_len: i32, amount: u64| -> i32 {
            // Get the memory
            let memory = caller.get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("WASM module must export memory");
            
            // Read the sender DID string
            let mut sender_buffer = vec![0u8; sender_len as usize];
            if memory.read(&mut caller, sender_ptr as usize, &mut sender_buffer).is_err() {
                return -2; // Memory read error
            }
            
            // Read the recipient DID string
            let mut recipient_buffer = vec![0u8; recipient_len as usize];
            if memory.read(&mut caller, recipient_ptr as usize, &mut recipient_buffer).is_err() {
                return -2; // Memory read error
            }
            
            // Convert to UTF-8 strings
            match (String::from_utf8(sender_buffer), String::from_utf8(recipient_buffer)) {
                (Ok(sender_did), Ok(recipient_did)) => {
                    let host_env = caller.data().host_mut();
                    let runtime = tokio::runtime::Runtime::new().unwrap();
                    runtime.block_on(host_env.transfer_token(&sender_did, &recipient_did, amount))
                },
                _ => -2, // Invalid UTF-8
            }
        },
    )?;
    
    // Register the anchor receipt function
    linker.func_wrap(
        "icn_host", 
        "host_anchor_receipt", 
        |mut caller: Caller<'_, StoreData>, ptr: u32, len: u32| -> i32 {
            // Get the memory
            let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                Some(mem) => mem,
                None => return -3, // Memory not found
            };
            
            // Read the receipt bytes from memory
            let data = memory.data(&caller);
            let start = ptr as usize;
            let end = start + len as usize;
            
            // Check bounds
            if end > data.len() {
                return -2; // Out of bounds
            }
            
            let bytes = &data[start..end];
            
            // Deserialize the receipt
            let receipt: ExecutionReceipt = match serde_cbor::from_slice(bytes) {
                Ok(r) => r,
                Err(_) => return -1, // Deserialization error
            };
            
            // We'll use a blocking approach here since wasmtime doesn't support async calls
            // In a production system, this should be handled differently with proper async
            let host_env = caller.data().host_mut();
            let runtime = tokio::runtime::Runtime::new().unwrap();
            
            match runtime.block_on(host_env.anchor_receipt(receipt)) {
                Ok(()) => 0, // Success
                Err(e) => {
                    use crate::host_environment::AnchorError;
                    match e {
                        AnchorError::ExecutorMismatch(_, _) => -10, // Executor mismatch
                        AnchorError::InvalidSignature(_) => -11,     // Invalid signature
                        AnchorError::SerializationError(_) => -12,   // Serialization error
                        AnchorError::CidError(_) => -13,             // CID error
                        AnchorError::DagStoreError(_) => -14,        // DAG store error
                        AnchorError::MissingFederationId => -15,     // Missing federation ID
                    }
                }
            }
        },
    )?;
    
    // Register the submit mesh job function
    linker.func_wrap(
        "icn_host",
        "host_submit_mesh_job",
        |mut caller: Caller<'_, StoreData>, 
            job_params_cbor_ptr: i32, 
            job_params_cbor_len: i32,
            job_id_buffer_ptr: i32,
            job_id_buffer_len: i32
        | -> Result<i32, Trap> {
                
            let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                Some(mem) => mem,
                None => return Ok(-3), // Error: Memory not found
            };

            let job_params: MeshJobParams = {
                let data = memory.data(&caller);
                let start = job_params_cbor_ptr as usize;
                let end = start + job_params_cbor_len as usize;
                if end > data.len() {
                    return Ok(-21); // Error: Payload out of bounds
                }
                let payload_bytes = &data[start..end];
                match serde_cbor::from_slice(payload_bytes) {
                    Ok(params) => params,
                    Err(_) => return Ok(-22), // Error: Payload deserialization failed
                }
            };

            let originator_did = caller.data().host().caller_did.clone();

            let job_id_str = format!("job_{}", Uuid::new_v4());

            let submission_timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Trap::new("System time error"))?
                .as_secs();

            let mesh_job = MeshJob {
                job_id: job_id_str.clone(),
                params: job_params,
                originator_did,
                submission_timestamp,
            };
            
            caller.data().host_mut().ctx.pending_mesh_jobs.lock().unwrap().push_back(mesh_job);
            
            // Write JobId string to guest-provided buffer
            let job_id_bytes = job_id_str.as_bytes();
            let job_id_actual_len = job_id_bytes.len();

            if job_id_buffer_len == 0 {
                return Ok(-31); // Error: Output buffer length is zero
            }
            // As per ABI: Return 0 if job_id_buffer_len is too small. Let's define minimal as at least 1 char.
            // A more robust check might be if it can fit "job_" + even a short UUID part.
            // For now, if buffer_len is less than actual_len, we might truncate or return error.
            // Let's try to write what fits. If nothing fits (e.g. buffer_len < minimal_id_len), return 0.
            // A typical JobId like "job_uuid" is long. Let's say min 10 chars for a very basic ID.
            const MIN_JOB_ID_WRITE_LEN: usize = 1; 
            if job_id_buffer_len < MIN_JOB_ID_WRITE_LEN as i32 {
                 return Ok(0); // Buffer too small to write anything meaningful
            }

            let len_to_write = std::cmp::min(job_id_actual_len, job_id_buffer_len as usize);

            if len_to_write == 0 { // Should be caught by MIN_JOB_ID_WRITE_LEN check if that's > 0
                return Ok(0); // Cannot write anything
            }

            match memory.write(&mut caller, job_id_buffer_ptr as usize, &job_id_bytes[..len_to_write]) {
                Ok(_) => Ok(len_to_write as i32), // Success: return actual bytes written
                Err(_) => Ok(-32), // Error: Failed to write JobId to WASM memory buffer
            }
        },
    )?;

    Ok(())
} 