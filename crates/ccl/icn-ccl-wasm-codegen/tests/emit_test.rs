use icn_ccl_compiler::lower::lower_str;
use icn_ccl_wasm_codegen::compile_to_wasm;
use wasmparser::Validator;
use icn_ccl_wasm_codegen::{emit::program_to_wasm, WasmGenerator};
use wasmparser::{WasmFeatures, ImportSectionReader, ExternalKind, Parser, Payload, TypeRef};

#[test]
fn emit_budget_wasm_validates() {
    let src = include_str!("../../icn-ccl-parser/templates/budget.ccl");
    let modules = lower_str(src).expect("lower to DSL");
    let bytes = compile_to_wasm(modules);

    // quick sanity: wasmparser validates
    Validator::new()
        .validate_all(&bytes)
        .expect("output wasm must validate");

    // snapshot raw opcode list for reference
    let prog = icn_ccl_wasm_codegen::WasmGenerator::new()
        .generate(lower_str(src).unwrap());
    insta::assert_json_snapshot!("budget_wasm_opcodes", prog);
}

fn has_range_check_call(bytes: &[u8]) -> bool {
    for payload in Parser::new(0).parse_all(bytes) {
        match payload {
            Ok(Payload::ImportSection(reader)) => {
                for imp in reader {
                    // Assuming imp is a Result, let's unwrap or handle error
                    let imp_ok = imp.expect("Failed to read import entry");
                    if imp_ok.module == "icn" && imp_ok.name == "range_check" {
                        // Check if it's a function import
                        if let TypeRef::Func(_) = imp_ok.ty {
                            return true;
                        }
                    }
                }
            }
            Ok(_) => {} // Handle other payload types if necessary
            Err(e) => {
                eprintln!("Error parsing Wasm payload: {:?}", e);
                return false; // Or handle error appropriately
            }
        }
    }
    false
}

#[test]
fn wasm_contains_range_check() {
    // --- Minimal CCL with a named range ------------------------------
    let src = r#"
        proposal "demo" {
            thresholds {
                range 0 10 {
                    approvers 1
                }
            }
        }
    "#;

    let modules  = lower_str(src).expect("lowering failed");
    let program  = WasmGenerator::new().generate(modules);
    let wasm_bin = program_to_wasm(&program);

    // Validate module
    let mut validator = Validator::new_with_features(WasmFeatures::default());
    validator.validate_all(&wasm_bin).expect("wasm invalid");

    // Ensure our import is present
    assert!(
        has_range_check_call(&wasm_bin),
        "range_check import not found in module"
    );
} 