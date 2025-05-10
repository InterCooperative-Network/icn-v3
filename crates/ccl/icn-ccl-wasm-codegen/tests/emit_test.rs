use icn_ccl_compiler::lower::lower_str;
use icn_ccl_wasm_codegen::compile_to_wasm;
use wasmparser::Validator;

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