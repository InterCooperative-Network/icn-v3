use icn_ccl_compiler::lower::lower_str;
use icn_ccl_wasm_codegen::WasmGenerator;
use icn_ccl_wasm_codegen::opcodes::Program;
use insta::assert_json_snapshot;

// Helper to load CCL, parse, lower, and generate opcodes
fn modules_from_ccl_string(ccl_string: &str) -> Program {
    let modules = lower_str(ccl_string).unwrap_or_else(|e| {
        panic!("Failed to lower CCL string to DSL: {:#?}", e);
    });
    let generator = WasmGenerator::new();
    generator.generate(modules) // Call generate on the instance
}

macro_rules! snapshot_file {
    ($name:ident, $path:expr) => {
        #[test]
        fn $name() {
            let src = include_str!($path);
            let program_ops = modules_from_ccl_string(src);
            assert_json_snapshot!(stringify!($name), program_ops);
        }
    };
}

// Test cases
snapshot_file!(election_ops, "../../icn-ccl-parser/templates/election.ccl");
snapshot_file!(budget_ops, "../../icn-ccl-parser/templates/budget.ccl");
snapshot_file!(bylaws_ops, "../../icn-ccl-parser/templates/bylaws.ccl"); 