use anyhow::{Result};
// use once_cell::sync::Lazy; // Removed
use serde::{Deserialize, Serialize};
// use std::collections::HashMap; // Removed
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use thiserror::Error;
use wasm_encoder::{
    CodeSection, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction, MemorySection, MemoryType, Module, TypeSection, ValType, EntityType,
};

/// Error types specific to the CCL compiler
#[derive(Error, Debug)]
pub enum CompilerError {
    #[error("Failed to parse CCL: {0}")]
    ParseError(String),

    #[error("Failed to compile DSL to WASM: {0}")]
    WasmCompilationError(String),

    #[error("Failed to generate DSL code: {0}")]
    DslGenerationError(String),

    #[error("Missing required CCL section: {0}")]
    MissingSection(String),

    #[error("Invalid template: {0}")]
    TemplateError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Opcodes supported by the DSL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DslOpcode {
    /// Anchor data to the DAG
    AnchorData { cid: String },

    /// Perform a metered action
    PerformMeteredAction { action_type: String, amount: u64 },

    /// Mint a token
    MintToken {
        token_type: String,
        amount: u64,
        recipient: String,
    },

    /// Submit a job to the planetary mesh
    SubmitJob {
        wasm_cid: String,
        description: String,
        resource_type: String,
        resource_amount: u64,
        priority: String,
    },
}

/// DSL program structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DslProgram {
    /// Name of the program
    pub name: String,

    /// Description of the program
    pub description: String,

    /// Version of the DSL specification
    pub version: String,

    /// Opcodes in the program
    pub opcodes: Vec<DslOpcode>,
}

/// The CCL compiler for transforming CCL to DSL to WASM
pub struct CclCompiler {
    /// Storage for temporary files
    temp_dir: TempDir,
}

impl CclCompiler {
    /// Create a new CCL compiler
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;

        Ok(Self {
            temp_dir,
        })
    }

    /// Compile CCL source to an intermediate DSL representation (stub)
    pub fn compile_to_dsl(&self, _ccl_source: &str) -> Result<String> {
        // TODO: Implement actual DSL compilation from CCL document
        Ok("DSL representation (stub)".to_string())
    }

    /// Compile CCL source to DSL and then to WASM
    pub fn compile_to_wasm(&self, ccl_source: &str) -> Result<Vec<u8>> {
        // First compile to DSL
        let dsl_code = self.compile_to_dsl(ccl_source)?;

        // Now compile DSL to WASM
        self.compile_dsl_to_wasm(&dsl_code)
    }

    /// Compile DSL to WASM
    pub fn compile_dsl_to_wasm(&self, dsl_code: &str) -> Result<Vec<u8>> {
        // Create a temporary directory for the Rust project
        let project_dir = self.temp_dir.path().join("dsl_project");
        std::fs::create_dir_all(&project_dir)?;

        // Create a simple Rust project with the DSL code
        self.create_rust_project(&project_dir, dsl_code)?;

        // Compile the project to WASM
        self.build_wasm_module(&project_dir)?;

        // Read the resulting WASM file
        let wasm_path = project_dir.join("target/wasm32-unknown-unknown/release/dsl_program.wasm");
        let wasm_bytes = std::fs::read(wasm_path)?;

        Ok(wasm_bytes)
    }

    /// Create a Rust project structure for compiling the DSL
    fn create_rust_project(&self, project_dir: &Path, dsl_code: &str) -> Result<()> {
        // Create Cargo.toml
        let cargo_toml = r#"[package]
name = "dsl_program"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]

[profile.release]
opt-level = 3
lto = true
"#;

        std::fs::write(project_dir.join("Cargo.toml"), cargo_toml)?;

        // Create src directory
        let src_dir = project_dir.join("src");
        std::fs::create_dir_all(&src_dir)?;

        // Create lib.rs with the DSL code
        std::fs::write(src_dir.join("lib.rs"), dsl_code)?;

        Ok(())
    }

    /// Build the WASM module from the Rust project
    fn build_wasm_module(&self, project_dir: &Path) -> Result<()> {
        // Run cargo build to compile to WebAssembly
        let output = Command::new("cargo")
            .current_dir(project_dir)
            .args(["build", "--release", "--target=wasm32-unknown-unknown"])
            .output()?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(CompilerError::WasmCompilationError(error.to_string()).into());
        }

        Ok(())
    }

    /// Compile CCL directly from a file
    pub fn compile_file(&self, ccl_path: &Path) -> Result<Vec<u8>> {
        let ccl_source = std::fs::read_to_string(ccl_path)?;
        self.compile_to_wasm(&ccl_source)
    }

    /// Generate DSL for a file and save it
    pub fn compile_file_to_dsl(&self, ccl_path: &Path, dsl_path: &Path) -> Result<()> {
        let ccl_source = std::fs::read_to_string(ccl_path)?;
        let dsl_code = self.compile_to_dsl(&ccl_source)?;
        std::fs::write(dsl_path, dsl_code)?;
        Ok(())
    }

    /// Compile a file to WASM and save it
    pub fn compile_file_to_wasm(&self, ccl_path: &Path, wasm_path: &Path) -> Result<()> {
        let wasm_bytes = self.compile_file(ccl_path)?;
        std::fs::write(wasm_path, wasm_bytes)?;
        Ok(())
    }

    /// This function generates a WASM module from DSL opcodes.
    pub fn generate_wasm_from_dsl_opcodes(opcodes: &[DslOpcode]) -> Result<Vec<u8>, CompilerError> {
        let mut module = Module::new();

        // Type section: Define function signatures
        let mut types = TypeSection::new();
        // Type 0: (ptr, len) -> () for host_log_message and host_anchor_to_dag
        let host_fn_type_idx = Self::define_string_param_host_function_type_static(&mut types);
        // Type 1: () -> () for the main exported 'run' function
        types.function(vec![], vec![]);
        let run_fn_type_idx = types.len() - 1;
        module.section(&types);

        // Imports section for host functions
        let mut imports = ImportSection::new();
        // Imported functions get their own function indices starting from 0.
        // host_log_message will be func idx 0
        imports.import("host", "host_log_message", EntityType::Function(host_fn_type_idx));
        // host_anchor_to_dag will be func idx 1
        imports.import("host", "host_anchor_to_dag", EntityType::Function(host_fn_type_idx));
        module.section(&imports);

        // Function section: Declare 'run' function (locally defined)
        // This associates a function index in this module with a previously defined type index.
        let mut funcs = FunctionSection::new();
        funcs.function(run_fn_type_idx); // run_fn_type_idx is the *type index* for run()
        module.section(&funcs);

        // Memory section (minimum 1 page)
        let mut memory_section = MemorySection::new();
        memory_section.memory(MemoryType { minimum: 1, maximum: None, memory64: false, shared: false });
        module.section(&memory_section);

        // Code section: Define the body of the 'run' function
        let mut code = CodeSection::new();
        let locals = vec![]; 
        let mut f = Function::new(locals);

        for opcode in opcodes {
            match opcode {
                DslOpcode::AnchorData { cid: _ } => {
                    f.instruction(&Instruction::Call(1)); // Call host_anchor_to_dag (import func idx 1)
                }
                DslOpcode::PerformMeteredAction { action_type: _, amount: _ } => {
                    f.instruction(&Instruction::Call(0)); // Call host_log_message (import func idx 0)
                }
                DslOpcode::MintToken { token_type: _, amount: _, recipient: _ } => {
                    f.instruction(&Instruction::Call(0)); 
                }
                DslOpcode::SubmitJob { .. } => {
                    f.instruction(&Instruction::Call(0)); 
                }
            }
        }
        f.instruction(&Instruction::End);
        code.function(&f); 
        module.section(&code);

        // Exports section: Export 'run' function
        // The 'run' function is the first (and only) locally defined function.
        // Its index is the number of imported functions (currently 2).
        let mut exports = ExportSection::new();
        exports.export("run", ExportKind::Func, 2); // Export function at index 2 (0 & 1 are imports)
        module.section(&exports);

        Ok(module.finish())
    }

    fn define_string_param_host_function_type_static(types: &mut TypeSection) -> u32 {
        let param_types = vec![ValType::I32, ValType::I32];
        let result_types = vec![];
        types.function(param_types, result_types);
        types.len() - 1 
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dsl_generation() {
        let compiler = CclCompiler::new().expect("Failed to create compiler");
        let dsl_code = compiler
            .compile_to_dsl("dummy ccl code")
            .expect("Failed to compile to DSL");

        // Simple checks to ensure the template got rendered
        assert!(dsl_code.contains("Starting execution of Example Governance"));
        assert!(dsl_code.contains(
            "Anchoring data: bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
        ));
    }
}
