use anyhow::{anyhow, Result};
// use once_cell::sync::Lazy; // Removed
use serde::{Deserialize, Serialize};
// use std::collections::HashMap; // Removed
use std::path::{Path, PathBuf};
// use std::process::Command; // To be removed if compile_dsl_to_wasm is removed
use tempfile::TempDir; // Keeping TempDir for now
use thiserror::Error;
// use icn_ccl_dsl::{ActionStep, ResourceType}; // This line removed
// wasm_encoder imports might be removable if generate_wasm_from_dsl_opcodes is removed
/* // Commenting out wasm_encoder imports for now, will be removed if generate_wasm_from_dsl_opcodes is removed
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection,
    Instruction, MemorySection, MemoryType, Module, TypeSection, ValType, BlockType,
};
*/

pub mod lower;

// Import for the new compilation path
use icn_ccl_wasm_codegen;

/// Error types specific to the CCL compiler
#[derive(Error, Debug)]
pub enum CompilerError {
    #[error("Failed to parse CCL: {0}")]
    ParseError(String),

    #[error("Failed to compile DSL to WASM: {0}")]
    WasmCompilationError(String),

    #[error("Failed to generate DSL code: {0}")]
    DslGenerationError(String),

    #[error("Lowering CCL to DSL AST failed: {0}")] // New error variant
    LoweringError(String),

    #[error("Missing required CCL section: {0}")]
    MissingSection(String),

    #[error("Invalid template: {0}")]
    TemplateError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// The CCL compiler for transforming CCL to DSL to WASM
pub struct CclCompiler {
    /// Storage for temporary files
    _temp_dir: TempDir, // Renamed to indicate it might become unused by CclCompiler itself
}

impl CclCompiler {
    /// Create a new CCL compiler
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        Ok(Self { _temp_dir: temp_dir })
    }

    /// Lowers CCL source to an intermediate DSL AST representation.
    fn lower_ccl_to_dsl_ast(&self, ccl_source: &str) -> Result<Vec<icn_ccl_dsl::DslModule>> {
        lower::lower_str(ccl_source).map_err(|e| {
            anyhow!(CompilerError::LoweringError(format!("Lowering failed: {}", e)))
        })
    }

    /// Compile CCL source to an intermediate DSL representation (JSON string).
    pub fn compile_to_dsl_string(&self, ccl_source: &str) -> Result<String> {
        let dsl_modules = self.lower_ccl_to_dsl_ast(ccl_source)?;
        serde_json::to_string_pretty(&dsl_modules).map_err(|e| {
            anyhow!(CompilerError::DslGenerationError(format!("Failed to serialize DSL modules: {}", e)))
        })
    }

    /// Compile CCL source to WASM bytecode.
    pub fn compile_to_wasm(&self, ccl_source: &str) -> Result<Vec<u8>> {
        let dsl_modules = self.lower_ccl_to_dsl_ast(ccl_source)?;
        // Use the wasm-codegen crate for DSL AST to WASM compilation
        Ok(icn_ccl_wasm_codegen::compile_to_wasm(dsl_modules))
    }

    /// Compile CCL directly from a file to WASM bytecode.
    pub fn compile_file(&self, ccl_path: &Path) -> Result<Vec<u8>> {
        let ccl_source = std::fs::read_to_string(ccl_path)?;
        self.compile_to_wasm(&ccl_source)
    }

    /// Generate DSL (JSON string) for a file and save it.
    pub fn compile_file_to_dsl_string(&self, ccl_path: &Path, dsl_path: &Path) -> Result<()> {
        let ccl_source = std::fs::read_to_string(ccl_path)?;
        let dsl_string = self.compile_to_dsl_string(&ccl_source)?;
        std::fs::write(dsl_path, dsl_string)?;
        Ok(())
    }

    /// Compile a file to WASM and save it.
    pub fn compile_file_to_wasm(&self, ccl_path: &Path, wasm_path: &Path) -> Result<()> {
        let wasm_bytes = self.compile_file(ccl_path)?;
        std::fs::write(wasm_path, wasm_bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    
    use crate::lower::lower_str;
    use insta::assert_json_snapshot;

    const ELECTION_CCL_STR: &str = include_str!("../../icn-ccl-parser/templates/election.ccl");
    const BUDGET_CCL_STR: &str = include_str!("../../icn-ccl-parser/templates/budget.ccl");
    const BYLAWS_CCL_STR: &str = r#"
// ICN Contract Chain Language – Bylaws Template
bylaws_def "Cooperative Bylaws from CONST" version "1.0.0-const" {

  // ─────────────  High-level parameters  ─────────────
  description "Core operational rules and governance structure for the cooperative."
  min_members_for_quorum 10
  max_voting_period_days 14
  default_proposal_duration "7d"

  // ─────────────  Conditional rules  ─────────────
  if proposal.type == "bylaw_change" {
    description "Special rules for bylaw changes."
    quorum 0.60
    voting_period "14d"
  }

  if proposal.category == "emergency" {
    fast_track true
    notification_period "1d"
  } else {
    standard_review_period "7d"
  }

  // ─────────────  Range-based rules  ─────────────
  member_age_requirement range 18 120 {
    status "eligible"
    requires_guardian_approval false
  }

  // ─────────────  Nested config blocks  ─────────────
  proposal_processing {
    min_duration "7d"
    max_duration "21d"
    default_duration "14d"
    pass_threshold_percentage 0.66
    quorum_percentage 0.10
    can_be_emergency true
    emergency_pass_threshold_percentage 0.75
    emergency_quorum_percentage 0.20
  }

  // ─────────────  Lifecycle actions  ─────────────
  actions {
    on "bylaw.amendment.proposed" {
      mint_token {
        type "bylaw_amendment_proposal_receipt"
        recipients proposal.submitter_id
        data {
          proposal_id proposal.id
          submitted_at timestamp()
        }
      }

      anchor_data {
        path "governance/bylaws"
        data proposal.content
      }
    }
  }

  // ─────────────  Logging example  ─────────────
  log_event(name: "bylaws_loaded", detail: "Cooperative Bylaws CONST v1.0.0-const processed");
}
"#;

    #[test]
    fn election_template_lowers() {
        let dsl_modules = lower_str(ELECTION_CCL_STR).unwrap();
        assert_json_snapshot!(dsl_modules);
    }

    #[test]
    fn budget_template_lowers() {
        let dsl_modules = lower_str(BUDGET_CCL_STR).unwrap();
        assert_json_snapshot!(dsl_modules);
    }

    #[test]
    fn bylaws_template_lowers() {
        let dsl_modules = lower_str(BYLAWS_CCL_STR).unwrap();
        assert_json_snapshot!(dsl_modules);
    }
}
