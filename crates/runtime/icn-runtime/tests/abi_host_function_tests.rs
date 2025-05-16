"""use anyhow::anyhow;
use icn_runtime::host_environment::ConcreteHostEnvironment;
use icn_runtime::job_execution_context::JobExecutionContext; // Make sure this is pub
use icn_runtime::wasm::register_host_functions; // Path to register_host_functions
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::{Config, Engine, Linker, Module, Store, Instance, Value, Memory, TypedFunc};

// Helper to write string to wasm memory for tests
fn write_string_to_wasm_mem(
    store: &mut Store<Arc<Mutex<ConcreteHostEnvironment<()>>>>,
    instance: &Instance,
    text: &str,
    ptr: u32,
    max_len: u32,
) -> anyhow::Result<u32> {
    let memory = instance
        .get_memory(store, "memory")
        .ok_or_else(|| anyhow!("memory export not found"))?;
    let bytes = text.as_bytes();
    if bytes.len() > max_len as usize {
        return Err(anyhow!("String '{}' too long for buffer", text));
    }
    memory.write(store, ptr as usize, bytes)?;
    Ok(bytes.len() as u32)
}

// Helper to read string from wasm memory for tests
fn read_string_from_wasm_mem(
    store: &mut Store<Arc<Mutex<ConcreteHostEnvironment<()>>>>,
    instance: &Instance,
    ptr: u32,
    len: u32,
) -> anyhow::Result<String> {
    let memory = instance
        .get_memory(store, "memory")
        .ok_or_else(|| anyhow!("memory export not found"))?;
    let mut buffer = vec![0u8; len as usize];
    memory.read(store, ptr as usize, &mut buffer)?;
    String::from_utf8(buffer).map_err(|e| anyhow!("Failed to read UTF-8 string from WASM: {}", e))
}


#[tokio::test]
async fn test_host_begin_section_via_linker() -> anyhow::Result<()> {
    let wat = r#"
        (module
            (import "icn_host" "host_begin_section"
                (func $host_begin_section (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 10) "test_kind")      ;; Place "test_kind" at memory offset 10
            (data (i32.const 100) "Test Title")    ;; Place "Test Title" at memory offset 100
            (func (export "call_host_begin_section") (result i32)
                i32.const 10      ;; kind_ptr
                i32.const 9       ;; kind_len ("test_kind".len())
                i32.const 100     ;; title_ptr
                i32.const 10      ;; title_len ("Test Title".len())
                call $host_begin_section
            )
        )
    "#;

    let engine = Engine::new(Config::new().async_support(true))?;
    let module = Module::new(&engine, wat)?;
    let mut linker = Linker::new(&engine);

    // Ensure JobExecutionContext and ConcreteHostEnvironment are accessible and Default/new works
    let job_ctx = JobExecutionContext::default();
    let host_env_instance = ConcreteHostEnvironment::<()>::new_with_context(job_ctx);
    let host_env_arc = Arc::new(Mutex::new(host_env_instance));

    register_host_functions(&mut linker)?;

    let mut store = Store::new(&engine, host_env_arc.clone());
    let instance = linker.instantiate_async(&mut store, &module).await?;
    
    let call_func = instance
        .get_func(&mut store, "call_host_begin_section")
        .ok_or_else(|| anyhow!("missing export call_host_begin_section"))?
        .typed::<(), i32, _>(&mut store)?;

    let result = call_func.call_async(&mut store, ()).await?;
    assert_eq!(result, 0, "host_begin_section should return 0 on success");

    // Verify the host context mutated
    let host_env_guard = host_env_arc.lock().await;
    let ctx_guard = host_env_guard.ctx.lock().await; // Lock the inner JobExecutionContext
    
    assert_eq!(ctx_guard.section_stack.len(), 1, "Section stack should have one entry");
    assert_eq!(ctx_guard.section_stack[0].kind, "test_kind");
    assert_eq!(ctx_guard.section_stack[0].title, Some("Test Title".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_host_set_property_via_linker() -> anyhow::Result<()> {
    let wat = r#"
        (module
            (import "icn_host" "host_begin_section" (func $host_begin_section (param i32 i32 i32 i32) (result i32)))
            (import "icn_host" "host_set_property" (func $host_set_property (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "prop_key")
            (data (i32.const 32) "prop_value_json")
            (data (i32.const 64) "section_kind") ;; For begin_section

            (func (export "call_host_set_property") (result i32)
                ;; First, begin a section
                i32.const 64    ;; kind_ptr for section_kind
                i32.const 12    ;; kind_len for section_kind
                i32.const 0     ;; title_ptr (empty for this test)
                i32.const 0     ;; title_len (empty for this test)
                call $host_begin_section
                drop            ;; Discard result of begin_section

                ;; Now, set property
                i32.const 0     ;; key_ptr
                i32.const 8     ;; key_len ("prop_key")
                i32.const 32    ;; value_json_ptr
                i32.const 15    ;; value_json_len ("prop_value_json")
                call $host_set_property
            )
        )
    "#;

    let engine = Engine::new(Config::new().async_support(true))?;
    let module = Module::new(&engine, wat)?;
    let mut linker = Linker::new(&engine);

    let job_ctx = JobExecutionContext::default();
    let host_env_instance = ConcreteHostEnvironment::<()>::new_with_context(job_ctx);
    let host_env_arc = Arc::new(Mutex::new(host_env_instance));

    register_host_functions(&mut linker)?;

    let mut store = Store::new(&engine, host_env_arc.clone());
    let instance = linker.instantiate_async(&mut store, &module).await?;

    let call_func = instance
        .get_func(&mut store, "call_host_set_property")
        .ok_or_else(|| anyhow!("missing export call_host_set_property"))?
        .typed::<(), i32, _>(&mut store)?;

    let result = call_func.call_async(&mut store, ()).await?;
    assert_eq!(result, 0, "host_set_property should return 0 on success");
    
    let host_env_guard = host_env_arc.lock().await;
    let ctx_guard = host_env_guard.ctx.lock().await;

    assert_eq!(ctx_guard.section_stack.len(), 1, "Section stack should have one entry after begin_section");
    assert!(ctx_guard.section_stack[0].properties.contains_key("prop_key"), "Properties should contain 'prop_key'");
    assert_eq!(ctx_guard.section_stack[0].properties.get("prop_key"), Some(&"prop_value_json".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_host_submit_mesh_job_via_linker() -> anyhow::Result<()> {
    let wat = r#"
        (module
            (import "icn_host" "host_submit_mesh_job" 
                (func $host_submit_mesh_job (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1) ;; 1 page = 64KiB
            ;; CBOR payload will be written here by the test harness
            (data (i32.const 0) "dummy_cbor_payload_data_to_be_overwritten") 
            ;; Job ID buffer (output)
            (global $job_id_buf_ptr (mut i32) (i32.const 1024)) ;; Place buffer at 1KB offset
            (global $job_id_buf_len i32 (i32.const 64))         ;; Buffer length for job ID

            (func (export "call_host_submit_mesh_job") (param $payload_ptr i32) (param $payload_len i32) (result i32)
                local.get $payload_ptr
                local.get $payload_len
                global.get $job_id_buf_ptr
                global.get $job_id_buf_len
                call $host_submit_mesh_job
            )
        )
    "#;

    let engine = Engine::new(Config::new().async_support(true))?;
    let module = Module::new(&engine, wat)?;
    let mut linker = Linker::new(&engine);

    let job_ctx = JobExecutionContext::default();
    let host_env_instance = ConcreteHostEnvironment::<()>::new_with_context(job_ctx);
    let host_env_arc = Arc::new(Mutex::new(host_env_instance));

    register_host_functions(&mut linker)?;

    let mut store = Store::new(&engine, host_env_arc.clone());
    let instance = linker.instantiate_async(&mut store, &module).await?;

    // Prepare CBOR payload and write it to WASM memory
    let cbor_payload_data = vec![0x01, 0x02, 0x03, 0x04, 0x05]; // Dummy CBOR data
    let payload_ptr_in_wasm: u32 = 0; // Matches (data (i32.const 0) ...)
    let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| anyhow!("Memory not found"))?;
    memory.write(&mut store, payload_ptr_in_wasm as usize, &cbor_payload_data)?;


    let call_func = instance
        .get_func(&mut store, "call_host_submit_mesh_job")
        .ok_or_else(|| anyhow!("missing export call_host_submit_mesh_job"))?
        .typed::<(u32, u32), i32, _>(&mut store)?;

    // Pass the pointer and length of the CBOR payload we just wrote
    let job_id_actual_len = call_func.call_async(&mut store, (payload_ptr_in_wasm, cbor_payload_data.len() as u32)).await?;
    
    assert!(job_id_actual_len > 0, "host_submit_mesh_job should return positive length on success");

    // Verify the job ID written back to WASM memory
    let job_id_buf_ptr_val = instance
        .get_global(&mut store, "job_id_buf_ptr")
        .ok_or_else(|| anyhow!("Global job_id_buf_ptr not found"))?
        .get(&mut store)
        .i32()
        .ok_or_else(|| anyhow!("Global job_id_buf_ptr not i32"))? as u32;

    let returned_job_id = read_string_from_wasm_mem(&mut store, &instance, job_id_buf_ptr_val, job_id_actual_len as u32).await?;
    
    assert_eq!(returned_job_id, "dummy_mesh_job_123", "The dummy job ID was not written back correctly.");

    Ok(())
}

// TODO: Add RuntimeContext::minimal_for_testing() if it doesn't exist.
// Example:
// In crates/runtime/icn-runtime/src/context.rs (or similar)
/*
impl RuntimeContext<icn_economics::mana::InMemoryManaLedger> { // Assuming InMemoryManaLedger for testing
    pub fn minimal_for_testing() -> Self {
        // Create the most minimal RuntimeContext possible for tests.
        // This might involve creating dummy KeyPairs, empty indexes, etc.
        // The exact implementation depends on RuntimeContextBuilder and its requirements.
        use icn_identity::KeyPair;
        use std::sync::Arc;
        use crate::mana::InMemoryManaLedger; // Adjust path as needed

        let keypair = KeyPair::generate(); // icn_identity::KeyPair
        let mana_ledger = Arc::new(InMemoryManaLedger::new());

        RuntimeContextBuilder::new()
            .with_identity(keypair)
            .with_mana_ledger(mana_ledger)
            // Add other minimal .with_xxx() calls as required by the builder
            .build()
            .expect("Failed to build minimal RuntimeContext for testing")
    }
}
*/

// Also, ensure JobExecutionContext::default() uses DIDs that can be created without panic,
// or provide a simpler way to create them if Did::from_str is problematic in test contexts.
// For example, if Did has a simpler test constructor.

// Ensure `icn_runtime::job_execution_context::JobExecutionContext` is public
// and `icn_runtime::wasm::register_host_functions` points to the correct linker registration.
// The module `icn_runtime::wasm::linker` might contain `register_host_functions` if `linker_legacy_impl`
// is brought in via `pub use legacy_linker_impl::*;` and `legacy_linker_impl` has a `pub mod full` structure.
// Or it might be directly `icn_runtime::wasm::linker_legacy_impl::register_host_functions`.
// The current setup is `pub use legacy_linker_impl::*;` so `crate::wasm::register_host_functions` should be correct
// if `legacy_linker_impl.rs` has `pub fn register_host_functions`.
// The `full_host_abi` feature needs to be enabled for these tests to pick up the correct linker.
// Add to Cargo.toml for icn-runtime:
// [features]
// full_host_abi = []
// tests = ["full_host_abi"] # Or ensure tests always build with it.
// Or, when running tests: `cargo test --features full_host_abi -p icn-runtime`

// Make sure `SectionContext` is also public if used directly in assertions from the test module.
// It is currently public in job_execution_context.rs.

"" 