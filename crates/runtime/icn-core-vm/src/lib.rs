use anyhow::{anyhow, Result};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use wasmtime::{Engine, Func, Instance, Module, Store};

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
}

/// Metrics collected during execution
#[derive(Debug, Default, Clone)]
pub struct ExecutionMetrics {
    /// Fuel consumed during execution (a measure of computational resources)
    pub fuel_used: u64,
    
    /// Number of host calls made
    pub host_calls: u64,
    
    /// Total bytes read/written through host functions
    pub io_bytes: u64,
}

/// Resource limits for execution
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum fuel allocation
    pub max_fuel: u64,
    
    /// Maximum memory pages
    pub max_memory_pages: u32,
    
    /// Maximum number of host calls
    pub max_host_calls: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_fuel: 10_000_000, // Default reasonable limit
            max_memory_pages: 100, // ~6.4MB
            max_host_calls: 1000,
        }
    }
}

/// Host context for WASM execution
pub struct HostContext {
    /// Metrics collected during execution
    pub metrics: Arc<Mutex<ExecutionMetrics>>,
    
    /// Log messages from the execution
    pub logs: Arc<Mutex<Vec<String>>>,
    
    /// CIDs anchored during execution
    pub anchored_cids: Arc<Mutex<Vec<String>>>,
    
    /// Resource usage records
    pub resource_usage: Arc<Mutex<Vec<(String, u64)>>>,
}

impl Default for HostContext {
    fn default() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(ExecutionMetrics::default())),
            logs: Arc::new(Mutex::new(Vec::new())),
            anchored_cids: Arc::new(Mutex::new(Vec::new())),
            resource_usage: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// The Cooperative Virtual Machine for executing governance WASM code
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
        // Configure the wasmtime engine with metering
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        config.wasm_multi_memory(true);
        config.wasm_reference_types(true);
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        
        let engine = Engine::new(&config).expect("Failed to create WASM engine");
        
        Self { engine, limits }
    }
    
    /// Execute a WASM module with the provided context
    pub fn execute(&self, wasm_bytes: &[u8], context: &mut HostContext) -> Result<()> {
        // Compile the WASM module
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| anyhow!("Failed to compile WASM module: {}", e))?;
        
        // Create a store with fuel metering
        let mut store = Store::new(&self.engine, context);
        store.add_fuel(self.limits.max_fuel)
            .map_err(|e| anyhow!("Failed to add fuel to store: {}", e))?;
        
        // Register host functions
        let log_func = self.create_log_function(&mut store)?;
        let anchor_func = self.create_anchor_function(&mut store)?;
        let check_auth_func = self.create_check_auth_function(&mut store)?;
        let record_usage_func = self.create_record_usage_function(&mut store)?;
        
        // Instantiate the module with host functions
        let instance = Instance::new(
            &mut store, 
            &module, 
            &[
                log_func.into(),
                anchor_func.into(),
                check_auth_func.into(),
                record_usage_func.into(),
            ],
        ).map_err(|e| anyhow!("Failed to instantiate WASM module: {}", e))?;
        
        // Try to find and call the entrypoint
        let result = self.call_entrypoint(&mut store, &instance);
        
        // Record metrics
        if let Ok(fuel_consumed) = store.fuel_consumed() {
            let mut metrics = context.metrics.lock().unwrap();
            metrics.fuel_used = fuel_consumed;
        }
        
        result
    }
    
    /// Try different entrypoints to call the WASM module
    fn call_entrypoint(&self, store: &mut Store<&mut HostContext>, instance: &Instance) -> Result<()> {
        // Try _start (standard WASI entrypoint)
        if let Ok(start) = instance.get_typed_func::<(), ()>(store, "_start") {
            return start.call(store, ())
                .map_err(|e| CoVmError::ExecutionError(e.to_string()).into());
        }
        
        // Try run (common simple entrypoint)
        if let Ok(run) = instance.get_typed_func::<(), ()>(store, "run") {
            return run.call(store, ())
                .map_err(|e| CoVmError::ExecutionError(e.to_string()).into());
        }
        
        // Try main (traditional entrypoint)
        if let Ok(main) = instance.get_typed_func::<(), ()>(store, "main") {
            return main.call(store, ())
                .map_err(|e| CoVmError::ExecutionError(e.to_string()).into());
        }
        
        Err(CoVmError::InvalidEntrypoint("No valid entrypoint found (_start, run, or main)".to_string()).into())
    }
    
    /// Create host function for logging messages
    fn create_log_function(&self, store: &mut Store<&mut HostContext>) -> Result<Func> {
        let log_func = Func::wrap(
            store,
            |mut caller: wasmtime::Caller<'_, &mut HostContext>, ptr: i32, len: i32| -> Result<(), wasmtime::Trap> {
                // Increment host call counter
                {
                    let mut metrics = caller.data().metrics.lock().unwrap();
                    metrics.host_calls += 1;
                }
                
                // Read memory from WASM
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return Err(wasmtime::Trap::new("Failed to find memory export")),
                };
                
                let data = memory.data(&caller)
                    .get(ptr as u32 as usize..(ptr as u32 + len as u32) as usize)
                    .ok_or_else(|| wasmtime::Trap::new("Invalid memory access"))?;
                
                // Convert to string
                let message = match std::str::from_utf8(data) {
                    Ok(s) => s.to_string(),
                    Err(_) => return Err(wasmtime::Trap::new("Invalid UTF-8 in log message")),
                };
                
                // Store log message
                caller.data().logs.lock().unwrap().push(message);
                
                // Update IO metrics
                {
                    let mut metrics = caller.data().metrics.lock().unwrap();
                    metrics.io_bytes += len as u64;
                }
                
                Ok(())
            },
        );
        
        Ok(log_func)
    }
    
    /// Create host function for anchoring CIDs to DAG
    fn create_anchor_function(&self, store: &mut Store<&mut HostContext>) -> Result<Func> {
        let anchor_func = Func::wrap(
            store,
            |mut caller: wasmtime::Caller<'_, &mut HostContext>, ptr: i32, len: i32| -> Result<(), wasmtime::Trap> {
                // Increment host call counter
                {
                    let mut metrics = caller.data().metrics.lock().unwrap();
                    metrics.host_calls += 1;
                }
                
                // Read memory from WASM
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return Err(wasmtime::Trap::new("Failed to find memory export")),
                };
                
                let data = memory.data(&caller)
                    .get(ptr as u32 as usize..(ptr as u32 + len as u32) as usize)
                    .ok_or_else(|| wasmtime::Trap::new("Invalid memory access"))?;
                
                // Convert to string (CID)
                let cid = match std::str::from_utf8(data) {
                    Ok(s) => s.to_string(),
                    Err(_) => return Err(wasmtime::Trap::new("Invalid UTF-8 in CID")),
                };
                
                // Store anchored CID
                caller.data().anchored_cids.lock().unwrap().push(cid);
                
                // Update IO metrics
                {
                    let mut metrics = caller.data().metrics.lock().unwrap();
                    metrics.io_bytes += len as u64;
                }
                
                Ok(())
            },
        );
        
        Ok(anchor_func)
    }
    
    /// Create host function for checking resource authorization
    fn create_check_auth_function(&self, store: &mut Store<&mut HostContext>) -> Result<Func> {
        let check_auth_func = Func::wrap(
            store,
            |mut caller: wasmtime::Caller<'_, &mut HostContext>, 
              type_ptr: i32, type_len: i32, 
              amount: i64| -> Result<i32, wasmtime::Trap> {
                // Increment host call counter
                {
                    let mut metrics = caller.data().metrics.lock().unwrap();
                    metrics.host_calls += 1;
                }
                
                // Read memory from WASM
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return Err(wasmtime::Trap::new("Failed to find memory export")),
                };
                
                let data = memory.data(&caller)
                    .get(type_ptr as u32 as usize..(type_ptr as u32 + type_len as u32) as usize)
                    .ok_or_else(|| wasmtime::Trap::new("Invalid memory access"))?;
                
                // Convert to string (resource type)
                let resource_type = match std::str::from_utf8(data) {
                    Ok(s) => s.to_string(),
                    Err(_) => return Err(wasmtime::Trap::new("Invalid UTF-8 in resource type")),
                };
                
                // Update IO metrics
                {
                    let mut metrics = caller.data().metrics.lock().unwrap();
                    metrics.io_bytes += type_len as u64;
                }
                
                // Simple mock auth check - in real implementation this would check against governance rules
                // 1 = authorized, 0 = not authorized
                Ok(1)
            },
        );
        
        Ok(check_auth_func)
    }
    
    /// Create host function for recording resource usage
    fn create_record_usage_function(&self, store: &mut Store<&mut HostContext>) -> Result<Func> {
        let record_usage_func = Func::wrap(
            store,
            |mut caller: wasmtime::Caller<'_, &mut HostContext>, 
              type_ptr: i32, type_len: i32, 
              amount: i64| -> Result<(), wasmtime::Trap> {
                // Increment host call counter
                {
                    let mut metrics = caller.data().metrics.lock().unwrap();
                    metrics.host_calls += 1;
                }
                
                // Read memory from WASM
                let memory = match caller.get_export("memory") {
                    Some(wasmtime::Extern::Memory(mem)) => mem,
                    _ => return Err(wasmtime::Trap::new("Failed to find memory export")),
                };
                
                let data = memory.data(&caller)
                    .get(type_ptr as u32 as usize..(type_ptr as u32 + type_len as u32) as usize)
                    .ok_or_else(|| wasmtime::Trap::new("Invalid memory access"))?;
                
                // Convert to string (resource type)
                let resource_type = match std::str::from_utf8(data) {
                    Ok(s) => s.to_string(),
                    Err(_) => return Err(wasmtime::Trap::new("Invalid UTF-8 in resource type")),
                };
                
                // Record resource usage
                caller.data().resource_usage.lock().unwrap().push((resource_type, amount as u64));
                
                // Update IO metrics
                {
                    let mut metrics = caller.data().metrics.lock().unwrap();
                    metrics.io_bytes += type_len as u64;
                }
                
                Ok(())
            },
        );
        
        Ok(record_usage_func)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Test will be added once we have test WASM modules
} 