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
            // Call the actual trait method implementation
            let host = caller.data().host(); // Get immutable borrow first
            let result = host.host_submit_mesh_job(
                caller, // Pass the caller
                job_params_cbor_ptr as u32,
                job_params_cbor_len as u32,
                job_id_buffer_ptr as u32,
                job_id_buffer_len as u32,
            );
            // Wasmtime automatically converts the anyhow::Error into a Trap
            result.map_err(|e| Trap::new(e.to_string())) // Explicitly convert Anyhow Error to Trap
        },
    )?;

    // == MeshHostAbi Wrappers ==
    // Wrap functions from the MeshHostAbi trait
    let map_err = |e: anyhow::Error| Trap::new(e.to_string());

    linker.func_wrap(
        "icn_host", "host_job_get_id",
        |mut caller: Caller<'_, StoreData>, p0: u32, p1: u32|
         -> Result<i32, Trap> {
            caller.data().host().host_job_get_id(caller, p0, p1).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_job_get_initial_input_cid",
        |mut caller: Caller<'_, StoreData>, p0: u32, p1: u32|
         -> Result<i32, Trap> {
            caller.data().host().host_job_get_initial_input_cid(caller, p0, p1).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_job_is_interactive",
        |caller: Caller<'_, StoreData>|
         -> Result<i32, Trap> {
            caller.data().host().host_job_is_interactive(caller).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_workflow_get_current_stage_index",
        |caller: Caller<'_, StoreData>|
         -> Result<i32, Trap> {
            caller.data().host().host_workflow_get_current_stage_index(caller).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_workflow_get_current_stage_id",
        |mut caller: Caller<'_, StoreData>, p0: u32, p1: u32|
         -> Result<i32, Trap> {
            caller.data().host().host_workflow_get_current_stage_id(caller, p0, p1).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_workflow_get_current_stage_input_cid",
        |mut caller: Caller<'_, StoreData>, p0: u32, p1: u32, p2: u32, p3: u32|
         -> Result<i32, Trap> {
            caller.data().host().host_workflow_get_current_stage_input_cid(caller, p0, p1, p2, p3).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_job_report_progress",
        |mut caller: Caller<'_, StoreData>, p0: u8, p1: u32, p2: u32|
         -> Result<i32, Trap> {
            caller.data().host().host_job_report_progress(caller, p0, p1, p2).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_workflow_complete_current_stage",
        |mut caller: Caller<'_, StoreData>, p0: u32, p1: u32|
         -> Result<i32, Trap> {
            caller.data().host().host_workflow_complete_current_stage(caller, p0, p1).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_interactive_send_output",
        |mut caller: Caller<'_, StoreData>, p0: u32, p1: u32, p2: u32, p3: u32, p4: i32|
         -> Result<i32, Trap> {
            caller.data().host().host_interactive_send_output(caller, p0, p1, p2, p3, p4).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_interactive_receive_input",
        |mut caller: Caller<'_, StoreData>, p0: u32, p1: u32, p2: u32|
         -> Result<i32, Trap> {
            caller.data().host().host_interactive_receive_input(caller, p0, p1, p2).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_interactive_peek_input_len",
        |caller: Caller<'_, StoreData>|
         -> Result<i32, Trap> {
            caller.data().host().host_interactive_peek_input_len(caller).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_interactive_prompt_for_input",
        |mut caller: Caller<'_, StoreData>, p0: u32, p1: u32|
         -> Result<i32, Trap> {
            caller.data().host().host_interactive_prompt_for_input(caller, p0, p1).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_data_read_cid",
        |mut caller: Caller<'_, StoreData>, p0: u32, p1: u32, p2: u64, p3: u32, p4: u32|
         -> Result<i32, Trap> {
            caller.data().host().host_data_read_cid(caller, p0, p1, p2, p3, p4).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_data_write_buffer",
        |mut caller: Caller<'_, StoreData>, p0: u32, p1: u32, p2: u32, p3: u32|
         -> Result<i32, Trap> {
            caller.data().host().host_data_write_buffer(caller, p0, p1, p2, p3).map_err(map_err)
        }
    )?;
    linker.func_wrap(
        "icn_host", "host_log_message",
        |mut caller: Caller<'_, StoreData>, level_val: u32, p1: u32, p2: u32|
         -> Result<i32, Trap> {
             // Need to convert level_val (u32) to LogLevel
             // This is tricky as LogLevel is not Copy. Assuming a TryFrom implementation exists or can be added.
             // For now, let's use a placeholder match. 
             // This part might need adjustment based on LogLevel definition and conversion capabilities.
            let level = match level_val {
                 0 => host_abi::LogLevel::Error,
                 1 => host_abi::LogLevel::Warn,
                 2 => host_abi::LogLevel::Info,
                 3 => host_abi::LogLevel::Debug,
                 4 => host_abi::LogLevel::Trace,
                 _ => return Err(Trap::new(format!("Invalid log level: {}", level_val))),
            };
            caller.data().host().host_log_message(caller, level, p1, p2).map_err(map_err)
        }
    )?;
    
    Ok(())
} 