use crate::opcodes::{Opcode, Program};
use icn_economics::ResourceType;
use icn_types::mesh::{MeshJobParams, QoSProfile, WorkflowType};
use serde_cbor;
use serde_json;
use std::collections::HashMap;

use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, EntityType, ExportKind,
    ExportSection, Function, FunctionSection, ImportSection, Instruction, MemorySection,
    MemoryType, Module, TypeSection, ValType,
};

pub const JOB_ID_BUFFER_SIZE: u32 = 128;
pub const JOB_ID_BUFFER_OFFSET: u32 = 0; // Start data segments at offset 0

// Helper to emit data segments correctly using ConstExpr
fn emit_data_segment(data_section: &mut DataSection, offset: u32, data: &[u8]) {
    data_section.active(
        0,                                    // Memory index 0
        &ConstExpr::i32_const(offset as i32), // CORRECTED: Use ConstExpr::i32_const
        data.iter().copied(),
    );
}

// New encode_push_string
fn encode_push_string(
    f: &mut Function,
    s: &str,
    data_section: &mut DataSection,
    current_data_offset: &mut u32,
) {
    let string_bytes = s.as_bytes();
    let string_len = string_bytes.len() as i32;
    // Use the current_data_offset as the pointer for this string
    let string_ptr = *current_data_offset;

    if string_len > 0 {
        // Place the string bytes into linear memory at the current offset
        emit_data_segment(data_section, string_ptr, string_bytes);
        // Advance the offset for the next piece of data
        *current_data_offset += string_len as u32;
        // TODO: Consider alignment for current_data_offset if mixing with non-byte data types
    }

    f.instruction(&Instruction::I32Const(string_ptr as i32));
    f.instruction(&Instruction::I32Const(string_len));
}

pub fn program_to_wasm(prog: &Program) -> Vec<u8> {
    let mut module = Module::new();
    let mut code = CodeSection::new();
    let mut functions_section = FunctionSection::new();
    let mut type_section = TypeSection::new();
    let mut import_section = ImportSection::new();
    let mut memory_section = MemorySection::new();
    let mut data_section = DataSection::new();

    // Initialize next_data_offset. It will be updated for CBOR and strings.
    // Starts after the JOB_ID_BUFFER.
    let mut next_data_offset = JOB_ID_BUFFER_OFFSET + JOB_ID_BUFFER_SIZE;

    // Define main function type: () -> i32
    // This type will be used for the "_start" function.
    // Find an existing ()->i32 type if host_submit_mesh_job (type 16) matches, or add new.
    // Type 16 is (i32, i32, i32, i32) -> i32. So, we need a new type for () -> i32.
    // Let's assume existing types are 0-16. New type for _start will be 17.
    let main_func_signature_type_idx = 17;
    type_section.function(vec![], vec![ValType::I32]);

    // Define memory (memory 0)
    // Initial size of 1 page (64KiB) should be enough for now.
    memory_section.memory(MemoryType {
        minimum: 1, // 1 page = 64KiB
        maximum: None,
        memory64: false,
        shared: false,
    });

    // Define Data Segment for JobId output buffer
    // This buffer will be written to by the host.
    let job_id_buffer_data = vec![0u8; JOB_ID_BUFFER_SIZE as usize];
    data_section.active(
        0, // Memory index 0
        &ConstExpr::i32_const(JOB_ID_BUFFER_OFFSET as i32),
        job_id_buffer_data,
    );

    // Determine the index for the main function we are about to define.
    // It will be after all imported functions.
    // Assuming host_fns has N items, imported functions are 0..N-1.
    // The first *defined* function in this module will be index N.
    let host_fns_count = 17; // As per current host_fns array (indices 0-16)
    let main_function_idx = host_fns_count as u32;

    // Declare the main function in the FunctionSection
    functions_section.function(main_func_signature_type_idx);

    // Create the body for the main "_start" function
    // Locals:
    // 0: last_submit_job_result (i32)
    let locals = vec![(1, ValType::I32)]; // CORRECTED: Declare locals if needed, pass to Function::new
    let mut main_f = Function::new(locals); // CORRECTED: Use Function::new
                                            // Initialize local(0) to 0 (default successful/neutral return if no SubmitJob happens or if it's not last)
    main_f.instruction(&Instruction::I32Const(0));
    main_f.instruction(&Instruction::LocalSet(0));

    // Process all opcodes, emitting them into the single main_f function body
    for op in prog.ops.iter() {
        // Note: The original type_index mapping per opcode is no longer used to declare separate functions.
        // All instructions are now part of main_f.

        match op {
            Opcode::BeginSection { kind, title } => {
                encode_push_string(&mut main_f, kind, &mut data_section, &mut next_data_offset);
                if let Some(t) = title {
                    encode_push_string(&mut main_f, t, &mut data_section, &mut next_data_offset);
                } else {
                    // Push pointer and length for an empty string if title is None
                    encode_push_string(&mut main_f, "", &mut data_section, &mut next_data_offset);
                }
                main_f.instruction(&Instruction::Call(0)); // host fn 0: begin_section
            }
            Opcode::EndSection => {
                // end_section now takes one i32 argument (a dummy one for now, consistent with type def)
                // If it truly takes no args, type def vec![] was right, but then call should also have no args.
                // User's type def was vec![ValType::I32]. Assuming it needs a dummy value.
                // f.instruction(&Instruction::I32Const(0)); // Dummy argument if needed by host
                main_f.instruction(&Instruction::Call(1)); // host fn 1: end_section
            }
            Opcode::CreateProposal { title, version } => {
                encode_push_string(&mut main_f, title, &mut data_section, &mut next_data_offset);
                encode_push_string(&mut main_f, version.as_deref().unwrap_or("0.0.0"), &mut data_section, &mut next_data_offset);
                main_f.instruction(&Instruction::Call(2)); // host fn 2: create_proposal
            }
            Opcode::MintToken {
                res_type,
                amount,
                recipient,
                data,
            } => {
                encode_push_string(&mut main_f, res_type, &mut data_section, &mut next_data_offset);
                main_f.instruction(&Instruction::I64Const(*amount as i64));
                encode_push_string(&mut main_f, recipient.as_deref().unwrap_or_default(), &mut data_section, &mut next_data_offset);
                encode_push_string(&mut main_f, data.as_deref().unwrap_or_default(), &mut data_section, &mut next_data_offset); // Added handling for data
                main_f.instruction(&Instruction::Call(3)); // host fn 3: mint_token
            }
            Opcode::AnchorData { path, data_ref } => {
                encode_push_string(&mut main_f, path.as_deref().unwrap_or_default(), &mut data_section, &mut next_data_offset);
                encode_push_string(&mut main_f, data_ref, &mut data_section, &mut next_data_offset);
                main_f.instruction(&Instruction::Call(4)); // host fn 4: anchor_data
            }
            Opcode::CallHost { fn_name, args_payload } => {
                encode_push_string(&mut main_f, fn_name, &mut data_section, &mut next_data_offset);
                encode_push_string(&mut main_f, args_payload, &mut data_section, &mut next_data_offset);
                main_f.instruction(&Instruction::Call(5)); // host fn 5: generic_call
            }
            Opcode::If { condition, .. } => {
                #[allow(clippy::needless_borrow)]
                encode_push_string(&mut main_f, &condition, &mut data_section, &mut next_data_offset);
                // encode_push_string(&mut f, &format!("{:?}", op)); // old log behavior
                main_f.instruction(&Instruction::Call(6)); // host fn 6: log (or a dedicated if_cond_eval)
            }
            Opcode::Else => {
                // encode_push_string(&mut f, &format!("{:?}", op)); // old log behavior
                main_f.instruction(&Instruction::Call(7)); // host fn 7: log_else (or just log)
            }
            Opcode::EndIf => {
                // encode_push_string(&mut f, &format!("{:?}", op)); // old log behavior
                main_f.instruction(&Instruction::Call(8)); // host fn 8: log_endif (or just log)
            }
            Opcode::SetProperty {
                key, value_json, ..
            } => {
                encode_push_string(&mut main_f, key, &mut data_section, &mut next_data_offset);
                encode_push_string(&mut main_f, value_json, &mut data_section, &mut next_data_offset);
                // encode_push_string(&mut f, &format!("{:?}", op)); // old log behavior
                main_f.instruction(&Instruction::Call(9)); // host fn 9: set_property (or just log)
            }
            Opcode::Todo(msg) => {
                encode_push_string(&mut main_f, msg, &mut data_section, &mut next_data_offset);
                // encode_push_string(&mut f, &format!("{:?}", op)); // old log behavior
                main_f.instruction(&Instruction::Call(10)); // host fn 10: log_todo (or just log)
            }
            Opcode::OnEvent { event } => {
                encode_push_string(&mut main_f, event, &mut data_section, &mut next_data_offset);
                main_f.instruction(&Instruction::Call(11)); // host fn 11: on_event
            }
            Opcode::RangeCheck { start, end } => {
                main_f.instruction(&Instruction::F64Const(*start));
                main_f.instruction(&Instruction::F64Const(*end));
                // TODO: Define and use the correct range_check_func_idx, assuming 13 for now as it's next.
                main_f.instruction(&Instruction::Call(13));
            }
            Opcode::UseResource {
                resource_type,
                amount,
            } => {
                encode_push_string(&mut main_f, resource_type, &mut data_section, &mut next_data_offset);
                main_f.instruction(&Instruction::I64Const(*amount as i64));
                main_f.instruction(&Instruction::Call(14)); // host fn 14: use_resource
            }
            Opcode::TransferToken {
                token_type,
                amount,
                sender,
                recipient,
            } => {
                encode_push_string(&mut main_f, token_type, &mut data_section, &mut next_data_offset);
                main_f.instruction(&Instruction::I64Const(*amount as i64));
                encode_push_string(&mut main_f, sender.as_deref().unwrap_or_default(), &mut data_section, &mut next_data_offset);
                encode_push_string(&mut main_f, recipient, &mut data_section, &mut next_data_offset);
                main_f.instruction(&Instruction::Call(15)); // host fn 15: transfer_token
            }
            Opcode::SubmitJob {
                wasm_cid,
                description,
                input_data_cid,
                entry_function: _, // Not directly used in MeshJobParams, info for executor
                required_resources_json,
                qos_profile_json,
                max_acceptable_bid_tokens, // Destructure this field
                deadline_utc_ms,
                metadata_json: _, // Not directly used in MeshJobParams currently
            } => {
                // 1. Construct MeshJobParams
                let mut resources_required_vec: Vec<(ResourceType, u64)> = Vec::new();
                if let Some(json_str) = required_resources_json {
                    match serde_json::from_str::<HashMap<String, u64>>(json_str) {
                        Ok(parsed_resources) => {
                            for (key, value) in parsed_resources {
                                let res_type = match key.to_lowercase().as_str() {
                                    "cpu" | "compute" => ResourceType::Cpu,
                                    "memory" => ResourceType::Memory,
                                    "io" => ResourceType::Io,
                                    "token" => ResourceType::Token,
                                    _ => {
                                        // TODO: Consider emitting a trap or error log
                                        continue;
                                    }
                                };
                                resources_required_vec.push((res_type, value));
                            }
                        }
                        Err(_e) => {
                            // TODO: Emit trap or error handling WASM for JSON parsing errors
                        }
                    }
                }

                let qos_profile_val = match qos_profile_json.as_ref() {
                    Some(json_str) => match serde_json::from_str::<QoSProfile>(json_str) {
                        Ok(profile) => profile,
                        Err(_e) => {
                            // TODO: Emit trap or error handling WASM for JSON parsing errors
                            QoSProfile::BestEffort // Default on parsing error
                        }
                    },
                    None => QoSProfile::BestEffort, // Default if not provided
                };

                let params = MeshJobParams {
                    wasm_cid: wasm_cid.clone(),
                    description: description.clone().unwrap_or_default(),
                    resources_required: resources_required_vec, // Use parsed vec
                    qos_profile: qos_profile_val,               // Use parsed value
                    deadline: *deadline_utc_ms,
                    input_data_cid: input_data_cid.clone(),
                    max_acceptable_bid_tokens: *max_acceptable_bid_tokens,
                    explicit_mana_cost: None,
                    workflow_type: WorkflowType::SingleWasmModule,
                    stages: None,
                    is_interactive: false,
                    expected_output_schema_cid: None,
                    execution_policy: None,
                };

                // 2. Serialize MeshJobParams to CBOR
                let params_cbor = serde_cbor::to_vec(&params).unwrap_or_default();

                // 3. Add CBOR Payload as a Data Segment & Update next_data_offset
                let params_cbor_ptr_val = next_data_offset;
                let params_cbor_len_val = params_cbor.len() as i32;

                if params_cbor_len_val > 0 {
                    // CORRECTED: Use emit_data_segment helper
                    emit_data_segment(&mut data_section, params_cbor_ptr_val, &params_cbor);
                    next_data_offset += params_cbor_len_val as u32;
                }

                // 4. Prepare JobId Output Buffer Parameters
                let job_id_buffer_ptr_val = JOB_ID_BUFFER_OFFSET as i32;
                let job_id_buffer_len_val = JOB_ID_BUFFER_SIZE as i32;

                // 5. Emit WASM instructions to call host_submit_mesh_job
                main_f.instruction(&Instruction::I32Const(params_cbor_ptr_val as i32));
                main_f.instruction(&Instruction::I32Const(params_cbor_len_val));
                main_f.instruction(&Instruction::I32Const(job_id_buffer_ptr_val));
                main_f.instruction(&Instruction::I32Const(job_id_buffer_len_val));
                main_f.instruction(&Instruction::Call(16)); // host_submit_mesh_job

                // Store result in local(0)
                main_f.instruction(&Instruction::LocalSet(0));
            }
        }
    }

    // Finalize main function body
    main_f.instruction(&Instruction::LocalGet(0));
    main_f.instruction(&Instruction::End);
    code.function(&main_f);

    // Types: Define types for all imported host functions
    type_section.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![]); // 0: begin_section
    type_section.function(vec![], vec![]); // 1: end_section()
    type_section.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![]); // 2: create_proposal
    type_section.function(
        vec![ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        vec![],
    ); // 3: mint_token
    type_section.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![]); // 4: anchor_data
    type_section.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![]); // 5: call_host
    type_section.function(vec![ValType::I32, ValType::I32], vec![]); // 6: log_if_condition
    type_section.function(vec![], vec![]); // 7: log_else()
    type_section.function(vec![], vec![]); // 8: log_endif()
    type_section.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![]); // 9: set_property
    type_section.function(vec![ValType::I32, ValType::I32], vec![]); // 10: log_todo
    type_section.function(vec![ValType::I32, ValType::I32], vec![]); // 11: on_event
    type_section.function(vec![ValType::I32, ValType::I32], vec![]); // 12: log_range_check
    type_section.function(vec![ValType::F64, ValType::F64], vec![]); // 13: range_check
    type_section.function(vec![ValType::I32, ValType::I64], vec![]); // 14: use_resource
    type_section.function(
        vec![ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        vec![],
    ); // 15: transfer_token
       // Type 16: host_submit_mesh_job(params_ptr: i32, params_len: i32, job_id_buf_ptr: i32, job_id_buf_len: i32) -> written_len: i32
    type_section.function(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );

    // Imports: Define all imported host functions
    let host_fns = [
        ("begin_section", 0u32),
        ("end_section", 1u32),
        ("create_proposal", 2u32),
        ("mint_token", 3u32),
        ("anchor_data", 4u32),
        ("call_host", 5u32),
        ("log_if_condition", 6u32),
        ("log_else", 7u32),
        ("log_endif", 8u32),
        ("set_property", 9u32),
        ("log_todo", 10u32),
        ("on_event", 11u32),
        ("log_range_check", 12u32),
        ("range_check", 13u32),
        ("use_resource", 14u32),
        ("transfer_token", 15u32),
        ("host_submit_mesh_job", 16u32),
    ];
    for (name, type_idx) in host_fns.iter() {
        import_section.import("icn_host", name, EntityType::Function(*type_idx));
        // Changed module to "icn_host"
    }

    // Exports: Export the main function as "_start"
    let mut export_section = ExportSection::new();
    export_section.export("_start", ExportKind::Func, main_function_idx);

    module.section(&type_section);
    module.section(&import_section);
    module.section(&functions_section); // Declares type signatures for functions in the code section
    module.section(&memory_section); // Add memory section
    module.section(&data_section); // Add data section
    module.section(&code); // Actual function bodies
    module.section(&export_section); // Add export section

    module.finish()
}
