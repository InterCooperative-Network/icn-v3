use anyhow::{anyhow, Result};
use handlebars::Handlebars;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use thiserror::Error;
use wasm_encoder::{
    CodeSection, ExportSection, Function, FunctionSection, ImportSection, Module, TypeSection,
    ValType,
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
    MintToken { token_type: String, amount: u64, recipient: String },
    
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
    /// Handlebars instance for template rendering
    handlebars: Handlebars<'static>,
    
    /// Storage for temporary files
    temp_dir: TempDir,
}

/// Handlebars templates for DSL generation
static DSL_TEMPLATES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut templates = HashMap::new();
    
    // Main program template
    templates.insert("main", r#"
// Generated DSL program from CCL
// Name: {{name}}
// Description: {{description}}
// Version: {{version}}

// Host imports
extern "C" {
    // Log a message to the host
    fn host_log_message(ptr: *const u8, len: usize);
    
    // Anchor a CID to the DAG
    fn host_anchor_to_dag(ptr: *const u8, len: usize) -> i32;
    
    // Check resource authorization
    fn host_check_resource_authorization(type_ptr: *const u8, type_len: usize, amount: i64) -> i32;
    
    // Record resource usage
    fn host_record_resource_usage(type_ptr: *const u8, type_len: usize, amount: i64);
    
    // Submit a job to the mesh network
    fn host_submit_job(
        wasm_cid_ptr: *const u8, 
        wasm_cid_len: usize,
        description_ptr: *const u8,
        description_len: usize,
        resource_type_ptr: *const u8,
        resource_type_len: usize,
        resource_amount: u64,
        priority_ptr: *const u8,
        priority_len: usize
    ) -> i32;
}

// Helper function to log a message
fn log(message: &str) {
    unsafe {
        host_log_message(message.as_ptr(), message.len());
    }
}

// Helper function to anchor a CID
fn anchor_data(cid: &str) -> bool {
    let result = unsafe {
        host_anchor_to_dag(cid.as_ptr(), cid.len())
    };
    result != 0
}

// Helper function to check authorization
fn check_authorization(resource_type: &str, amount: u64) -> bool {
    let result = unsafe {
        host_check_resource_authorization(
            resource_type.as_ptr(),
            resource_type.len(),
            amount as i64
        )
    };
    result != 0
}

// Helper function to record resource usage
fn record_usage(resource_type: &str, amount: u64) {
    unsafe {
        host_record_resource_usage(
            resource_type.as_ptr(),
            resource_type.len(),
            amount as i64
        );
    }
}

// Helper function to submit a job to the mesh network
fn submit_job(wasm_cid: &str, description: &str, resource_type: &str, amount: u64, priority: &str) -> bool {
    let result = unsafe {
        host_submit_job(
            wasm_cid.as_ptr(),
            wasm_cid.len(),
            description.as_ptr(),
            description.len(),
            resource_type.as_ptr(),
            resource_type.len(),
            amount,
            priority.as_ptr(),
            priority.len()
        )
    };
    result != 0
}

// Program entrypoint
#[no_mangle]
pub extern "C" fn run() {
    log("Starting execution of {{name}}");
    
    {{#each opcodes}}
    {{#if (eq this.type "AnchorData")}}
    // Anchor data to DAG
    log("Anchoring data: {{this.cid}}");
    let anchored = anchor_data("{{this.cid}}");
    if !anchored {
        log("Failed to anchor data");
    }
    {{/if}}
    
    {{#if (eq this.type "PerformMeteredAction")}}
    // Perform a metered action
    log("Performing action: {{this.action_type}} with amount {{this.amount}}");
    if check_authorization("{{this.action_type}}", {{this.amount}}) {
        record_usage("{{this.action_type}}", {{this.amount}});
        log("Action authorized and recorded");
    } else {
        log("Action not authorized");
    }
    {{/if}}
    
    {{#if (eq this.type "MintToken")}}
    // Mint tokens
    log("Minting {{this.amount}} of {{this.token_type}} to {{this.recipient}}");
    if check_authorization("token_mint", {{this.amount}}) {
        record_usage("token_mint", {{this.amount}});
        log("Token minting authorized and recorded");
    } else {
        log("Token minting not authorized");
    }
    {{/if}}
    
    {{#if (eq this.type "SubmitJob")}}
    // Submit job to mesh network
    log("Submitting job: {{this.description}} (WASM: {{this.wasm_cid}})");
    if check_authorization("{{this.resource_type}}", {{this.resource_amount}}) {
        let job_submitted = submit_job(
            "{{this.wasm_cid}}",
            "{{this.description}}",
            "{{this.resource_type}}",
            {{this.resource_amount}},
            "{{this.priority}}"
        );
        
        if job_submitted {
            record_usage("{{this.resource_type}}", {{this.resource_amount}});
            log("Job submitted successfully");
        } else {
            log("Failed to submit job");
        }
    } else {
        log("Job submission not authorized - insufficient resource quota");
    }
    {{/if}}
    {{/each}}
    
    log("Execution completed successfully");
}
"#);
    
    templates
});

impl CclCompiler {
    /// Create a new CCL compiler
    pub fn new() -> Result<Self> {
        let mut handlebars = Handlebars::new();
        
        // Register handlebars templates
        for (name, template) in DSL_TEMPLATES.iter() {
            handlebars.register_template_string(name, template)?;
        }
        
        // Register handlebars helpers
        handlebars.register_helper(
            "eq", 
            Box::new(|params| {
                if params.len() != 2 {
                    return Ok(false.into());
                }
                
                let left = params[0].value();
                let right = params[1].value();
                
                Ok((left == right).into())
            })
        );
        
        let temp_dir = TempDir::new()?;
        
        Ok(Self { handlebars, temp_dir })
    }
    
    /// Compile CCL source to DSL
    pub fn compile_to_dsl(&self, ccl_source: &str) -> Result<String> {
        // This is a stub for now, in a real implementation we would:
        // 1. Parse the CCL using icn-ccl-parser
        // 2. Extract governance rules and actions
        // 3. Map them to DSL opcodes
        
        // For this prototype, we'll create a simple example DSL program
        let program = DslProgram {
            name: "Example Governance".to_string(),
            description: "Example governance program from CCL".to_string(),
            version: "0.1.0".to_string(),
            opcodes: vec![
                DslOpcode::AnchorData { 
                    cid: "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi".to_string()
                },
                DslOpcode::PerformMeteredAction {
                    action_type: "budget_allocation".to_string(),
                    amount: 1000,
                },
                DslOpcode::MintToken {
                    token_type: "governance_token".to_string(),
                    amount: 100,
                    recipient: "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
                },
                DslOpcode::SubmitJob {
                    wasm_cid: "bafybeih7q27itb576mtmy5yzggkfzqnfj5dis4h2og6epvyvjyvcedwmze".to_string(),
                    description: "Data processing task".to_string(),
                    resource_type: "compute".to_string(),
                    resource_amount: 500,
                    priority: "medium".to_string(),
                },
            ],
        };
        
        // Render the DSL using Handlebars
        let dsl_code = self.handlebars.render("main", &program)
            .map_err(|e| CompilerError::DslGenerationError(e.to_string()))?;
        
        Ok(dsl_code)
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
            .args(&[
                "build",
                "--release",
                "--target=wasm32-unknown-unknown",
            ])
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
}

// Direct WASM generation without Rust compilation (simpler alternative)
pub fn generate_simple_wasm() -> Result<Vec<u8>> {
    let mut module = Module::new();
    
    // Add types
    let mut types = TypeSection::new();
    // void -> void for the 'run' function
    types.function(Vec::new(), Vec::new());
    // (i32, i32) -> void for host_log_message
    types.function(vec![ValType::I32, ValType::I32], Vec::new());
    // (i32, i32) -> i32 for host_anchor_to_dag
    types.function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    module.section(&types);
    
    // Add imports
    let mut imports = ImportSection::new();
    imports.import("host", "host_log_message", Function { ty: 1 });
    imports.import("host", "host_anchor_to_dag", Function { ty: 2 });
    module.section(&imports);
    
    // Add functions
    let mut functions = FunctionSection::new();
    // The 'run' function has type 0 (void -> void)
    functions.function(0);
    module.section(&functions);
    
    // Add exports
    let mut exports = ExportSection::new();
    exports.export("run", wasm_encoder::ExportKind::Func, 0);
    module.section(&exports);
    
    // Add code
    let mut code = CodeSection::new();
    // Empty function body for 'run'
    code.function(wasm_encoder::Function::new(Vec::new()));
    module.section(&code);
    
    Ok(module.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dsl_generation() {
        let compiler = CclCompiler::new().expect("Failed to create compiler");
        let dsl_code = compiler.compile_to_dsl("dummy ccl code").expect("Failed to compile to DSL");
        
        // Simple checks to ensure the template got rendered
        assert!(dsl_code.contains("Starting execution of Example Governance"));
        assert!(dsl_code.contains("Anchoring data: bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"));
    }
} 