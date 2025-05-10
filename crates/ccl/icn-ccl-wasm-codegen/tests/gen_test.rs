use icn_ccl_dsl::DslModule;
use icn_ccl_wasm_codegen::WasmGenerator; // WasmGenerator is pub in the crate root
use insta::assert_json_snapshot;

// Helper function to convert CCL string to DSL AST
// This uses the public API of the icn-ccl-compiler crate
fn modules_from_ccl_string(ccl_string: &str) -> Vec<DslModule> {
    icn_ccl_compiler::lower::lower_str(ccl_string).unwrap_or_else(|e| {
        panic!("Failed to lower CCL string to DSL: {}\nCCL Source:\n{}", e, ccl_string);
    })
}

macro_rules! snapshot_file {
    ($name:expr, $path:expr) => {{
        let src = include_str!($path);
        let modules = modules_from_ccl_string(src);
        let prog = WasmGenerator::generate(&modules);
        assert_json_snapshot!($name, prog);
    }};
}

#[test]
fn election_template_ops() {
    snapshot_file!("election_ops", "../../icn-ccl-parser/templates/election.ccl");
}

#[test]
fn budget_template_ops() {
    snapshot_file!("budget_ops", "../../icn-ccl-parser/templates/budget.ccl");
}

#[test]
fn bylaws_template_ops() {
    snapshot_file!("bylaws_ops", "../../icn-ccl-parser/templates/bylaws.ccl");
} 