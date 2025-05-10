use crate::opcodes::{Opcode, Program};
use icn_types::mesh::{MeshJobParams, QoSProfile};
use icn_economics::ResourceType;
use serde_json;
use serde_cbor;
use std::collections::HashMap;

use wasm_encoder::{
    CodeSection, EntityType, Function, FunctionSection, ImportSection, Instruction, Module,
    TypeSection, ValType,
};

pub fn program_to_wasm(prog: &Program) -> Vec<u8> {
    let mut module = Module::new();
    let mut code = CodeSection::new();
    let mut functions_section = FunctionSection::new();

    // one wasm function per opcode (MVP-style demo)
    for op in prog.ops.iter() {
        let mut f = Function::new(vec![]); // All functions () -> ()
        let type_index = match op {
            Opcode::BeginSection { .. } => 0,
            Opcode::EndSection => 1,
            Opcode::CreateProposal { .. } => 2,
            Opcode::MintToken { .. } => 3,
            Opcode::AnchorData { .. } => 4,
            Opcode::CallHost { .. } => 5,
            Opcode::If { .. } => 6,          // Was previously part of '_'
            Opcode::Else => 7,               // Was previously part of '_'
            Opcode::EndIf => 8,              // Was previously part of '_'
            Opcode::SetProperty { .. } => 9, // Was previously part of '_'
            Opcode::Todo(_) => 10,           // Was previously part of '_'
            Opcode::OnEvent { .. } => 11,    // New
            Opcode::RangeCheck { .. } => 13, // New type index for (F64, F64) -> ()
            Opcode::UseResource { .. } => 14, // New for resource usage tracking
            Opcode::TransferToken { .. } => 15, // New for token transfers
            Opcode::SubmitJob { .. } => 16,  // New for job submission
        };
        functions_section.function(type_index); // Map this function body to its type index

        match op {
            Opcode::BeginSection { kind, title } => {
                encode_push_string(&mut f, kind);
                if let Some(t) = title {
                    encode_push_string(&mut f, t);
                } else {
                    // Push a placeholder for missing optional title to keep call signature consistent
                    encode_push_string(&mut f, "");
                }
                f.instruction(&Instruction::Call(0)); // host fn 0: begin_section
            }
            Opcode::EndSection => {
                // end_section now takes one i32 argument (a dummy one for now, consistent with type def)
                // If it truly takes no args, type def vec![] was right, but then call should also have no args.
                // User's type def was vec![ValType::I32]. Assuming it needs a dummy value.
                // f.instruction(&Instruction::I32Const(0)); // Dummy argument if needed by host
                f.instruction(&Instruction::Call(1)); // host fn 1: end_section
            }
            Opcode::CreateProposal { title, version } => {
                encode_push_string(&mut f, title);
                encode_push_string(&mut f, version.as_deref().unwrap_or("0.0.0"));
                f.instruction(&Instruction::Call(2)); // host fn 2: create_proposal
            }
            Opcode::MintToken {
                res_type,
                amount,
                recipient,
                data,
            } => {
                encode_push_string(&mut f, res_type);
                f.instruction(&Instruction::I64Const(*amount as i64));
                encode_push_string(&mut f, recipient.as_deref().unwrap_or_default());
                encode_push_string(&mut f, data.as_deref().unwrap_or_default()); // Added handling for data
                f.instruction(&Instruction::Call(3)); // host fn 3: mint_token
            }
            Opcode::AnchorData { path, data_ref } => {
                encode_push_string(&mut f, path.as_deref().unwrap_or_default());
                encode_push_string(&mut f, data_ref);
                f.instruction(&Instruction::Call(4)); // host fn 4: anchor_data
            }
            Opcode::CallHost { fn_name, args } => {
                encode_push_string(&mut f, fn_name);
                let joined = serde_json::to_string(args).unwrap_or_else(|_| "[]".to_string()); // Default to empty JSON array
                encode_push_string(&mut f, &joined);
                f.instruction(&Instruction::Call(5)); // host fn 5: generic_call
            }
            Opcode::If { condition, .. } => {
                #[allow(clippy::needless_borrow)]
                encode_push_string(&mut f, &condition);
                // encode_push_string(&mut f, &format!("{:?}", op)); // old log behavior
                f.instruction(&Instruction::Call(6)); // host fn 6: log (or a dedicated if_cond_eval)
            }
            Opcode::Else => {
                // encode_push_string(&mut f, &format!("{:?}", op)); // old log behavior
                f.instruction(&Instruction::Call(7)); // host fn 7: log_else (or just log)
            }
            Opcode::EndIf => {
                // encode_push_string(&mut f, &format!("{:?}", op)); // old log behavior
                f.instruction(&Instruction::Call(8)); // host fn 8: log_endif (or just log)
            }
            Opcode::SetProperty {
                key, value_json, ..
            } => {
                encode_push_string(&mut f, key);
                encode_push_string(&mut f, value_json);
                // encode_push_string(&mut f, &format!("{:?}", op)); // old log behavior
                f.instruction(&Instruction::Call(9)); // host fn 9: set_property (or just log)
            }
            Opcode::Todo(msg) => {
                encode_push_string(&mut f, msg);
                // encode_push_string(&mut f, &format!("{:?}", op)); // old log behavior
                f.instruction(&Instruction::Call(10)); // host fn 10: log_todo (or just log)
            }
            Opcode::OnEvent { event } => {
                encode_push_string(&mut f, event);
                f.instruction(&Instruction::Call(11)); // host fn 11: on_event
            }
            Opcode::RangeCheck { start, end } => {
                f.instruction(&Instruction::F64Const(*start));
                f.instruction(&Instruction::F64Const(*end));
                // TODO: Define and use the correct range_check_func_idx, assuming 13 for now as it's next.
                f.instruction(&Instruction::Call(13));
            }
            Opcode::UseResource { resource_type, amount } => {
                encode_push_string(&mut f, resource_type);
                f.instruction(&Instruction::I64Const(*amount as i64));
                f.instruction(&Instruction::Call(14)); // host fn 14: use_resource
            }
            Opcode::TransferToken { token_type, amount, sender, recipient } => {
                encode_push_string(&mut f, token_type);
                f.instruction(&Instruction::I64Const(*amount as i64));
                encode_push_string(&mut f, sender.as_deref().unwrap_or_default());
                encode_push_string(&mut f, recipient);
                f.instruction(&Instruction::Call(15)); // host fn 15: transfer_token
            }
            Opcode::SubmitJob { 
                wasm_cid, 
                description, 
                input_data_cid, 
                entry_function: _, // Not directly used in MeshJobParams, info for executor
                required_resources_json, 
                qos_profile_json, 
                max_acceptable_bid_tokens: _, // Not directly used in MeshJobParams, for bidding phase
                deadline_utc_ms, 
                metadata_json: _, // Not directly used in MeshJobParams currently
            } => {
                // 1. Construct MeshJobParams
                let mut resources_required_vec: Vec<(ResourceType, u64)> = Vec::new();
                if let Some(json_str) = required_resources_json {
                    if let Ok(parsed_resources) = serde_json::from_str::<HashMap<String, u64>>(json_str) {
                        for (key, value) in parsed_resources {
                            let res_type = match key.to_lowercase().as_str() {
                                "cpu" => ResourceType::Cpu,
                                "memory" => ResourceType::Memory,
                                "io" => ResourceType::Io,
                                "token" => ResourceType::Token,
                                _ => continue, // Or handle as error
                            };
                            resources_required_vec.push((res_type, value));
                        }
                    }
                    // TODO: Handle parsing error robustly (e.g., emit trap or error code)
                }

                let qos_profile_val = qos_profile_json.as_ref()
                    .and_then(|json_str| serde_json::from_str::<QoSProfile>(json_str).ok())
                    .unwrap_or(QoSProfile::BestEffort); // Default QoS

                let params = MeshJobParams {
                    wasm_cid: wasm_cid.clone(),
                    description: description.clone().unwrap_or_default(),
                    resources_required: resources_required_vec,
                    qos_profile: qos_profile_val,
                    deadline: deadline_utc_ms.clone(),
                    input_data_cid: input_data_cid.clone(),
                };

                // 2. Serialize MeshJobParams to CBOR
                // This CBOR data needs to be placed in WASM memory.
                // For now, this step is conceptual. Actual WASM instructions depend on memory strategy.
                let params_cbor = serde_cbor::to_vec(&params).unwrap_or_default();
                // TODO: Error handling for serialization

                // 3. Emit WASM instructions (Conceptual - assumes pointers are available)
                //    Actual implementation requires defining data segments or using globals for pointers.
                
                // TODO: Get pointer and length for params_cbor (e.g., from a data segment)
                let params_cbor_ptr_val = 0; // Placeholder
                let params_cbor_len_val = params_cbor.len() as i32; // Placeholder

                // TODO: Get pointer and length for JobId output buffer (e.g., from a data segment or global)
                let job_id_buffer_ptr_val = 0; // Placeholder, e.g., address of a global buffer
                const JOB_ID_BUFFER_LEN: i32 = 128; // Placeholder
                
                f.instruction(&Instruction::I32Const(params_cbor_ptr_val));
                f.instruction(&Instruction::I32Const(params_cbor_len_val));
                f.instruction(&Instruction::I32Const(job_id_buffer_ptr_val));
                f.instruction(&Instruction::I32Const(JOB_ID_BUFFER_LEN));
                f.instruction(&Instruction::Call(16)); // host_submit_mesh_job
                
                // TODO: Handle the i32 result from host_submit_mesh_job (bytes written or error)
                // For example, store it in a local, check if negative (error), etc.
                f.instruction(&Instruction::Drop); // Drop the result for now
            }
        }
        f.instruction(&Instruction::End);
        code.function(&f);
    }

    // Types: Define types for all imported host functions
    let mut type_section = TypeSection::new();
    type_section.function(vec![ValType::I32, ValType::I32], vec![]); // 0: begin_section(kind: ptr, title: ptr)
    type_section.function(vec![], vec![]); // 1: end_section()
    type_section.function(vec![ValType::I32, ValType::I32], vec![]); // 2: create_proposal(title: ptr, version: ptr)
    type_section.function(
        vec![ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        vec![],
    ); // 3: mint_token(res: ptr, amt: i64, recip: ptr, data: ptr)
    type_section.function(vec![ValType::I32, ValType::I32], vec![]); // 4: anchor_data(path: ptr, ref: ptr)
    type_section.function(vec![ValType::I32, ValType::I32], vec![]); // 5: call_host(name: ptr, args_json: ptr)
    type_section.function(vec![ValType::I32], vec![]); // 6: log_if_condition(condition_str: ptr)
    type_section.function(vec![], vec![]); // 7: log_else()
    type_section.function(vec![], vec![]); // 8: log_endif()
    type_section.function(vec![ValType::I32, ValType::I32], vec![]); // 9: set_property(key: ptr, value_json: ptr)
    type_section.function(vec![ValType::I32], vec![]); // 10: log_todo(msg: ptr)
    type_section.function(vec![ValType::I32], vec![]); // 11: on_event(event_str: ptr) - New
    type_section.function(vec![ValType::I32], vec![]); // 12: log_range_check(debug_str: ptr) - New
    type_section.function(vec![ValType::F64, ValType::F64], vec![]); // 13: range_check(start: f64, end: f64)
    type_section.function(vec![ValType::I32, ValType::I64], vec![]); // 14: use_resource(resource_type: ptr, amount: i64)
    type_section.function(vec![ValType::I32, ValType::I64, ValType::I32, ValType::I32], vec![]); // 15: transfer_token(token_type: ptr, amount: i64, sender: ptr, recipient: ptr)
    // Type 16: host_submit_mesh_job(params_ptr: i32, params_len: i32, job_id_buf_ptr: i32, job_id_buf_len: i32) -> written_len: i32
    type_section.function(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]); 

    // Imports: Define all imported host functions
    let mut import_section = ImportSection::new();
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
        ("on_event", 11u32),        // New
        ("log_range_check", 12u32), // New
        ("range_check", 13u32),     // New for actual range check
        ("use_resource", 14u32),    // New for resource usage tracking
        ("transfer_token", 15u32),  // New for token transfers
        ("host_submit_mesh_job", 16u32), // Added for mesh job submission
    ];
    for (name, type_idx) in host_fns.iter() {
        import_section.import("icn_host", name, EntityType::Function(*type_idx)); // Changed module to "icn_host"
    }

    module.section(&type_section);
    module.section(&import_section);
    module.section(&functions_section); // Declares type signatures for functions in the code section
    module.section(&code); // Actual function bodies

    module.finish()
}

fn encode_push_string(f: &mut Function, s: &str) {
    let addr = crate::hash32(s) as i32; // Use the hash from lib.rs
    f.instruction(&Instruction::I32Const(addr));
}
