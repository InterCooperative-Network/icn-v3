use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use wasmtime::{
    AsContextMut, Caller, Config, Engine, Extern, Func, FuncType, Instance, Module, OptLevel,
    Store, Val, ValType,
};

/// Error types specific to the Cooperative VM
#[derive(Error, Debug)]
pub enum CoVmError {
    #[error("WASM module execution error: {0}")]
    ExecutionError(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Host function error: {0}")]
    HostFunctionError(String),

    #[error("Invalid entrypoint: {0}")]
    InvalidEntrypoint(String),

    #[error("Fuel exhausted")]
    FuelExhausted,
}

/// Metrics collected during execution
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    /// Fuel consumed during execution (a measure of computational resources)
    pub fuel_used: u64,

    /// Number of host calls made
    pub host_calls: u64,

    /// Total bytes read/written through host functions
    pub io_bytes: u64,

    /// Number of anchored CIDs
    pub anchored_cids_count: usize,

    /// Number of job submissions
    pub job_submissions_count: usize,
}

/// Resource limits for execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum fuel allocation
    pub max_fuel: u64,

    /// Maximum number of host calls
    pub max_host_calls: u32,

    /// Maximum total bytes read/written through host functions
    pub max_io_bytes: u64,

    /// Maximum number of anchored CIDs
    pub max_anchored_cids: usize,

    /// Maximum number of job submissions
    pub max_job_submissions: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_fuel: 10_000_000, // Default reasonable limit
            max_host_calls: 1000,
            max_io_bytes: 10_000_000,  // Default reasonable limit
            max_anchored_cids: 1000,   // Default reasonable limit
            max_job_submissions: 1000, // Default reasonable limit
        }
    }
}

/// Host context for WASM execution
#[derive(Debug, Clone)]
pub struct HostContext {
    /// Metrics collected during execution
    pub metrics: Arc<Mutex<ExecutionMetrics>>,

    /// Log messages from the execution
    pub logs: Arc<Mutex<Vec<String>>>,

    /// CIDs anchored during execution
    pub anchored_cids: Arc<Mutex<Vec<String>>>,

    /// Resource usage records
    pub resource_usage: Arc<Mutex<Vec<(String, u64)>>>,

    /// Job submissions from this execution
    pub job_submissions: Arc<Mutex<Vec<JobSubmission>>>,
}

/// A job submission from a WASM module
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSubmission {
    /// The WASM CID to execute
    pub wasm_cid: String,

    /// Description of the job
    pub description: String,

    /// Resource type to use
    pub resource_type: String,

    /// Amount of resources to allocate
    pub resource_amount: u64,

    /// Job priority
    pub priority: String,
}

impl Default for HostContext {
    fn default() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(ExecutionMetrics::default())),
            logs: Arc::new(Mutex::new(Vec::new())),
            anchored_cids: Arc::new(Mutex::new(Vec::new())),
            resource_usage: Arc::new(Mutex::new(Vec::new())),
            job_submissions: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// The Cooperative Virtual Machine for executing governance WASM code
#[derive(Clone)]
pub struct CoVm {
    engine: Engine,
    limits: ResourceLimits,
}

impl Default for CoVm {
    fn default() -> Self {
        Self::new(ResourceLimits::default())
    }
}

impl CoVm {
    /// Create a new CoVM with specified resource limits
    pub fn new(limits: ResourceLimits) -> Self {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.wasm_multi_memory(true);
        config.wasm_reference_types(true);
        config.cranelift_opt_level(OptLevel::Speed);
        let engine = Engine::new(&config).unwrap_or_else(|e| {
            panic!("Failed to create Wasmtime engine: {}", e);
        });
        Self { engine, limits }
    }

    /// Execute a WASM module with the provided context
    pub fn execute(&self, wasm_bytes: &[u8], context: HostContext) -> Result<HostContext> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| anyhow!("Failed to compile WASM module: {}", e))?;

        let mut store = Store::new(&self.engine, context);

        let initial_fuel = self.limits.max_fuel;
        store.set_fuel(initial_fuel)?;

        let log_func = self.create_log_function(&mut store);
        let anchor_func = self.create_anchor_function(&mut store);
        let check_auth_func = self.create_check_auth_function(&mut store);
        let record_usage_func = self.create_record_usage_function(&mut store);
        let submit_job_func = self.create_submit_job_function(&mut store);

        let instance = Instance::new(
            &mut store,
            &module,
            &[
                log_func.into(),
                anchor_func.into(),
                check_auth_func.into(),
                record_usage_func.into(),
                submit_job_func.into(),
            ],
        )
        .map_err(|e| anyhow!("Failed to instantiate WASM module: {}", e))?;

        let execution_result = self.call_entrypoint(&mut store, &instance);

        let fuel_remaining = store.get_fuel().unwrap_or(0);
        let fuel_consumed = initial_fuel.saturating_sub(fuel_remaining);

        let anchored_cids_len = store.data().anchored_cids.lock().unwrap().len();
        let job_submissions_len = store.data().job_submissions.lock().unwrap().len();

        store.data_mut().metrics.lock().unwrap().fuel_used = fuel_consumed;
        store.data_mut().metrics.lock().unwrap().anchored_cids_count = anchored_cids_len;
        store
            .data_mut()
            .metrics
            .lock()
            .unwrap()
            .job_submissions_count = job_submissions_len;

        let final_host_context = store.into_data();

        execution_result.map(|_| final_host_context)
    }

    /// Try different entrypoints to call the WASM module
    fn call_entrypoint(&self, store: &mut Store<HostContext>, instance: &Instance) -> Result<()> {
        let entrypoint = instance
            .get_typed_func::<(), ()>(&mut *store, "_start")
            .map_err(|e| anyhow!("Failed to get _start function: {}", e))?;
        entrypoint.call(store.as_context_mut(), ()).map_err(|e| {
            if e.to_string().contains("all fuel consumed") {
                CoVmError::FuelExhausted.into()
            } else {
                anyhow!("WASM execution trapped: {}", e)
            }
        })
    }

    /// Create host function for logging messages
    fn create_log_function(&self, store: &mut Store<HostContext>) -> Func {
        Func::new(
            store,
            FuncType::new(
                [ValType::I32, ValType::I32].iter().cloned(),
                [].iter().cloned(),
            ),
            |mut caller: Caller<'_, HostContext>,
             args: &[Val],
             _results: &mut [Val]|
             -> Result<()> {
                let ptr = args[0].unwrap_i32();
                let len = args[1].unwrap_i32();
                {
                    let mut metrics = caller.data_mut().metrics.lock().unwrap();
                    metrics.host_calls += 1;
                }
                let memory = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => bail!("Failed to find memory export"),
                };
                let data = memory
                    .data(&caller)
                    .get(ptr as u32 as usize..(ptr as u32 + len as u32) as usize)
                    .ok_or_else(|| anyhow!("Invalid memory access"))?;
                let message = std::str::from_utf8(data)
                    .map_err(|_| anyhow!("Invalid UTF-8 in log message"))?
                    .to_string();
                caller.data_mut().logs.lock().unwrap().push(message);
                {
                    let mut metrics = caller.data_mut().metrics.lock().unwrap();
                    metrics.io_bytes += len as u64;
                }
                Ok(())
            },
        )
    }

    /// Create host function for anchoring CIDs to DAG
    fn create_anchor_function(&self, store: &mut Store<HostContext>) -> Func {
        Func::new(
            store,
            FuncType::new(
                [ValType::I32, ValType::I32].iter().cloned(),
                [].iter().cloned(),
            ),
            |mut caller: Caller<'_, HostContext>,
             args: &[Val],
             _results: &mut [Val]|
             -> Result<()> {
                let ptr = args[0].unwrap_i32();
                let len = args[1].unwrap_i32();
                {
                    let mut metrics = caller.data_mut().metrics.lock().unwrap();
                    metrics.host_calls += 1;
                }
                let memory = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => bail!("Failed to find memory export"),
                };
                let data = memory
                    .data(&caller)
                    .get(ptr as u32 as usize..(ptr as u32 + len as u32) as usize)
                    .ok_or_else(|| anyhow!("Invalid memory access"))?;
                let cid_str = std::str::from_utf8(data)
                    .map_err(|_| anyhow!("Invalid UTF-8 in CID"))?
                    .to_string();
                caller
                    .data_mut()
                    .anchored_cids
                    .lock()
                    .unwrap()
                    .push(cid_str);
                {
                    let mut metrics = caller.data_mut().metrics.lock().unwrap();
                    metrics.io_bytes += len as u64;
                }
                Ok(())
            },
        )
    }

    /// Create host function for checking resource authorization
    fn create_check_auth_function(&self, store: &mut Store<HostContext>) -> Func {
        Func::new(
            store,
            FuncType::new(
                [ValType::I32, ValType::I32, ValType::I64].iter().cloned(),
                [ValType::I32].iter().cloned(),
            ),
            |mut caller: Caller<'_, HostContext>,
             args: &[Val],
             results: &mut [Val]|
             -> Result<()> {
                let type_ptr = args[0].unwrap_i32();
                let type_len = args[1].unwrap_i32();
                let _amount = args[2].unwrap_i64();
                {
                    let mut metrics = caller.data_mut().metrics.lock().unwrap();
                    metrics.host_calls += 1;
                }
                let memory = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => bail!("Failed to find memory export"),
                };
                let type_data = memory
                    .data(&caller)
                    .get(type_ptr as u32 as usize..(type_ptr as u32 + type_len as u32) as usize)
                    .ok_or_else(|| anyhow!("Invalid memory access"))?;
                let _resource_type = std::str::from_utf8(type_data)
                    .map_err(|_| anyhow!("Invalid UTF-8 in resource type"))?
                    .to_string();
                results[0] = Val::I32(1);
                Ok(())
            },
        )
    }

    /// Create host function for recording resource usage
    fn create_record_usage_function(&self, store: &mut Store<HostContext>) -> Func {
        Func::new(
            store,
            FuncType::new(
                [ValType::I32, ValType::I32, ValType::I64].iter().cloned(),
                [].iter().cloned(),
            ),
            |mut caller: Caller<'_, HostContext>,
             args: &[Val],
             _results: &mut [Val]|
             -> Result<()> {
                let type_ptr = args[0].unwrap_i32();
                let type_len = args[1].unwrap_i32();
                let amount = args[2].unwrap_i64();
                {
                    let mut metrics = caller.data_mut().metrics.lock().unwrap();
                    metrics.host_calls += 1;
                }
                let memory = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => bail!("Failed to find memory export"),
                };
                let type_data = memory
                    .data(&caller)
                    .get(type_ptr as u32 as usize..(type_ptr as u32 + type_len as u32) as usize)
                    .ok_or_else(|| anyhow!("Invalid memory access"))?;
                let resource_type = std::str::from_utf8(type_data)
                    .map_err(|_| anyhow!("Invalid UTF-8 in resource type"))?
                    .to_string();
                caller
                    .data_mut()
                    .resource_usage
                    .lock()
                    .unwrap()
                    .push((resource_type, amount as u64));
                {
                    let mut metrics = caller.data_mut().metrics.lock().unwrap();
                    metrics.io_bytes += type_len as u64;
                }
                Ok(())
            },
        )
    }

    /// Create host function for submitting a job
    fn create_submit_job_function(&self, store: &mut Store<HostContext>) -> Func {
        Func::new(
            store,
            FuncType::new(
                [
                    ValType::I32,
                    ValType::I32,
                    ValType::I32,
                    ValType::I32,
                    ValType::I32,
                    ValType::I32,
                    ValType::I64,
                    ValType::I32,
                    ValType::I32,
                ]
                .iter()
                .cloned(),
                [ValType::I32].iter().cloned(),
            ),
            |mut caller: Caller<'_, HostContext>,
             args: &[Val],
             results: &mut [Val]|
             -> Result<()> {
                let mut current_arg = 0;
                let wasm_cid_ptr = args[current_arg].unwrap_i32();
                current_arg += 1;
                let wasm_cid_len = args[current_arg].unwrap_i32();
                current_arg += 1;
                let desc_ptr = args[current_arg].unwrap_i32();
                current_arg += 1;
                let desc_len = args[current_arg].unwrap_i32();
                current_arg += 1;
                let rsrc_type_ptr = args[current_arg].unwrap_i32();
                current_arg += 1;
                let rsrc_type_len = args[current_arg].unwrap_i32();
                current_arg += 1;
                let rsrc_amount = args[current_arg].unwrap_i64();
                current_arg += 1;
                let priority_ptr = args[current_arg].unwrap_i32();
                current_arg += 1;
                let priority_len = args[current_arg].unwrap_i32();

                {
                    let mut metrics = caller.data_mut().metrics.lock().unwrap();
                    metrics.host_calls += 1;
                }
                let memory = match caller.get_export("memory") {
                    Some(Extern::Memory(mem)) => mem,
                    _ => bail!("Failed to find memory export"),
                };

                let wasm_cid_data = memory
                    .data(&caller)
                    .get(
                        wasm_cid_ptr as u32 as usize
                            ..(wasm_cid_ptr as u32 + wasm_cid_len as u32) as usize,
                    )
                    .ok_or_else(|| anyhow!("Invalid memory access for WASM CID"))?;
                let wasm_cid = std::str::from_utf8(wasm_cid_data)
                    .map_err(|_| anyhow!("Invalid UTF-8 in WASM CID"))?
                    .to_string();

                let desc_data = memory
                    .data(&caller)
                    .get(desc_ptr as u32 as usize..(desc_ptr as u32 + desc_len as u32) as usize)
                    .ok_or_else(|| anyhow!("Invalid memory access for description"))?;
                let description = std::str::from_utf8(desc_data)
                    .map_err(|_| anyhow!("Invalid UTF-8 in job description"))?
                    .to_string();

                let rsrc_type_data = memory
                    .data(&caller)
                    .get(
                        rsrc_type_ptr as u32 as usize
                            ..(rsrc_type_ptr as u32 + rsrc_type_len as u32) as usize,
                    )
                    .ok_or_else(|| anyhow!("Invalid memory access for resource type"))?;
                let resource_type = std::str::from_utf8(rsrc_type_data)
                    .map_err(|_| anyhow!("Invalid UTF-8 in resource type"))?
                    .to_string();

                let priority_data = memory
                    .data(&caller)
                    .get(
                        priority_ptr as u32 as usize
                            ..(priority_ptr as u32 + priority_len as u32) as usize,
                    )
                    .ok_or_else(|| anyhow!("Invalid memory access for priority"))?;
                let priority = std::str::from_utf8(priority_data)
                    .map_err(|_| anyhow!("Invalid UTF-8 in job priority"))?
                    .to_string();

                let job = JobSubmission {
                    wasm_cid,
                    description,
                    resource_type,
                    resource_amount: rsrc_amount as u64,
                    priority,
                };
                caller.data_mut().job_submissions.lock().unwrap().push(job);
                {
                    let mut metrics = caller.data_mut().metrics.lock().unwrap();
                    metrics.io_bytes +=
                        (wasm_cid_len + desc_len + rsrc_type_len + priority_len) as u64;
                }
                results[0] = Val::I32(1);
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    // use crate::job_utils::create_test_job; // To be removed
    // use crate::{CoVm, ExecutionMetrics, HostContext, ResourceLimits}; // To be removed

    // Test will be added once we have test WASM modules
}
