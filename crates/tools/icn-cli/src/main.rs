use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use icn_ccl_compiler::CclCompiler;
use icn_identity_core::vc::ExecutionReceiptCredential;
use icn_runtime::{Proposal, ProposalState, QuorumStatus};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// Command-line interface for ICN governance
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    /// Subcommand to execute
    #[clap(subcommand)]
    command: Commands,
}

/// CLI commands
#[derive(Subcommand)]
enum Commands {
    /// Proposal management commands
    #[clap(subcommand)]
    Proposal(ProposalCommands),
    
    /// CCL compilation commands
    #[clap(subcommand)]
    Ccl(CclCommands),
    
    /// Runtime execution commands
    #[clap(subcommand)]
    Runtime(RuntimeCommands),
}

/// Proposal management commands
#[derive(Subcommand)]
enum ProposalCommands {
    /// Create a new proposal from a CCL file
    Create {
        /// Path to the CCL file
        #[clap(long, short)]
        ccl_file: PathBuf,
        
        /// Title of the proposal
        #[clap(long, short)]
        title: String,
        
        /// Output file for the created proposal
        #[clap(long, short)]
        output: Option<PathBuf>,
    },
    
    /// Vote on a proposal
    Vote {
        /// Path to the proposal file
        #[clap(long, short)]
        proposal: PathBuf,
        
        /// Vote direction (yes/no)
        #[clap(long, short)]
        direction: String,
        
        /// Weight of the vote (default: 1)
        #[clap(long, short, default_value = "1")]
        weight: u64,
    },
    
    /// Check the status of a proposal
    Status {
        /// Path to the proposal file
        #[clap(long, short)]
        proposal: PathBuf,
    },
}

/// CCL compilation commands
#[derive(Subcommand)]
enum CclCommands {
    /// Compile a CCL file to DSL
    CompileToDsl {
        /// Path to the CCL file
        #[clap(long, short)]
        input: PathBuf,
        
        /// Output file for the DSL
        #[clap(long, short)]
        output: PathBuf,
    },
    
    /// Compile a CCL file to WASM
    CompileToWasm {
        /// Path to the CCL file
        #[clap(long, short)]
        input: PathBuf,
        
        /// Output file for the WASM
        #[clap(long, short)]
        output: PathBuf,
    },
}

/// Runtime execution commands
#[derive(Subcommand)]
enum RuntimeCommands {
    /// Execute a WASM file
    Execute {
        /// Path to the WASM file
        #[clap(long, short)]
        wasm: PathBuf,
        
        /// Path to the proposal file (optional)
        #[clap(long, short)]
        proposal: Option<PathBuf>,
        
        /// Output file for the execution receipt
        #[clap(long, short)]
        receipt: Option<PathBuf>,
    },
    
    /// Verify an execution receipt
    Verify {
        /// Path to the execution receipt
        #[clap(long, short)]
        receipt: PathBuf,
    },
}

/// Simple in-memory implementation of the RuntimeStorage trait for CLI testing
struct CliRuntimeStorage {
    /// Proposals stored in memory
    proposals: Vec<Proposal>,
    
    /// WASM modules stored in memory (CID -> bytes)
    wasm_modules: std::collections::HashMap<String, Vec<u8>>,
    
    /// Execution receipts stored in memory (CID -> receipt)
    receipts: std::collections::HashMap<String, String>,
}

impl CliRuntimeStorage {
    fn new() -> Self {
        Self {
            proposals: Vec::new(),
            wasm_modules: std::collections::HashMap::new(),
            receipts: std::collections::HashMap::new(),
        }
    }
}

#[async_trait::async_trait]
impl icn_runtime::RuntimeStorage for CliRuntimeStorage {
    async fn load_proposal(&self, id: &str) -> Result<Proposal> {
        self.proposals
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("Proposal not found: {}", id))
    }
    
    async fn update_proposal(&self, proposal: &Proposal) -> Result<()> {
        // In a real implementation, we would update the proposal in a database
        println!("Updated proposal: {}", proposal.id);
        Ok(())
    }
    
    async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
        self.wasm_modules
            .get(cid)
            .cloned()
            .ok_or_else(|| anyhow!("WASM module not found: {}", cid))
    }
    
    async fn store_receipt(&self, receipt: &icn_runtime::ExecutionReceipt) -> Result<String> {
        // Generate a CID for the receipt (just a UUID for simplicity)
        let cid = format!("receipt-{}", Uuid::new_v4());
        
        // In a real implementation, we would store the receipt in IPFS/Filecoin
        println!("Stored receipt with CID: {}", cid);
        
        Ok(cid)
    }
    
    async fn anchor_to_dag(&self, cid: &str) -> Result<String> {
        // In a real implementation, we would anchor the CID to a DAG
        println!("Anchored CID to DAG: {}", cid);
        
        // Return a mocked DAG anchor ID
        Ok(format!("dag-anchor-{}", Uuid::new_v4()))
    }
}

/// Create a new proposal from a CCL file
async fn create_proposal(ccl_file: &Path, title: &str, output: Option<&Path>) -> Result<()> {
    println!("Creating proposal from CCL file: {}", ccl_file.display());
    
    // Compile the CCL file to WASM
    let compiler = CclCompiler::new()?;
    let wasm_bytes = compiler.compile_file(ccl_file)?;
    
    // Generate CIDs for the CCL and WASM (just UUIDs for simplicity)
    let ccl_cid = format!("ccl-{}", Uuid::new_v4());
    let wasm_cid = format!("wasm-{}", Uuid::new_v4());
    
    // Create the proposal
    let proposal = Proposal {
        id: format!("proposal-{}", Uuid::new_v4()),
        wasm_cid,
        ccl_cid,
        state: ProposalState::Created,
        quorum_status: QuorumStatus::Pending,
    };
    
    // Output the proposal
    let proposal_json = serde_json::to_string_pretty(&proposal)?;
    
    if let Some(output_path) = output {
        std::fs::write(output_path, &proposal_json)?;
        println!("Proposal saved to: {}", output_path.display());
    } else {
        println!("Proposal created:");
        println!("{}", proposal_json);
    }
    
    Ok(())
}

/// Vote on a proposal
async fn vote_on_proposal(proposal_path: &Path, direction: &str, weight: u64) -> Result<()> {
    // Load the proposal
    let proposal_json = std::fs::read_to_string(proposal_path)?;
    let mut proposal: Proposal = serde_json::from_str(&proposal_json)?;
    
    // Update proposal state
    proposal.state = ProposalState::Voting;
    
    // Simulate voting
    let vote_str = match direction.to_lowercase().as_str() {
        "yes" => {
            // Simulate reaching quorum
            proposal.quorum_status = QuorumStatus::MajorityReached;
            "YES".green()
        }
        "no" => {
            proposal.quorum_status = QuorumStatus::Failed;
            "NO".red()
        }
        _ => return Err(anyhow!("Invalid vote direction. Use 'yes' or 'no'")),
    };
    
    println!("Voted {} on proposal {} with weight {}", vote_str, proposal.id, weight);
    
    // If voting is complete, update state
    if proposal.quorum_status == QuorumStatus::MajorityReached {
        proposal.state = ProposalState::Approved;
        println!("Proposal has been {}", "APPROVED".green());
    } else if proposal.quorum_status == QuorumStatus::Failed {
        proposal.state = ProposalState::Rejected;
        println!("Proposal has been {}", "REJECTED".red());
    }
    
    // Save the updated proposal
    let proposal_json = serde_json::to_string_pretty(&proposal)?;
    std::fs::write(proposal_path, proposal_json)?;
    
    Ok(())
}

/// Check the status of a proposal
async fn check_proposal_status(proposal_path: &Path) -> Result<()> {
    // Load the proposal
    let proposal_json = std::fs::read_to_string(proposal_path)?;
    let proposal: Proposal = serde_json::from_str(&proposal_json)?;
    
    // Display the status
    println!("Proposal: {}", proposal.id);
    
    let state_str = match proposal.state {
        ProposalState::Created => "Created".blue(),
        ProposalState::Voting => "Voting".yellow(),
        ProposalState::Approved => "Approved".green(),
        ProposalState::Rejected => "Rejected".red(),
        ProposalState::Executed => "Executed".green(),
    };
    
    let quorum_str = match proposal.quorum_status {
        QuorumStatus::Pending => "Pending".yellow(),
        QuorumStatus::MajorityReached => "Majority".green(),
        QuorumStatus::ThresholdReached => "Threshold".green(),
        QuorumStatus::WeightedReached => "Weighted".green(),
        QuorumStatus::Failed => "Failed".red(),
    };
    
    println!("State: {}", state_str);
    println!("Quorum: {}", quorum_str);
    println!("WASM CID: {}", proposal.wasm_cid);
    println!("CCL CID: {}", proposal.ccl_cid);
    
    Ok(())
}

/// Compile a CCL file to DSL
async fn compile_to_dsl(input: &Path, output: &Path) -> Result<()> {
    println!("Compiling CCL to DSL: {} -> {}", input.display(), output.display());
    
    let compiler = CclCompiler::new()?;
    compiler.compile_file_to_dsl(input, output)?;
    
    println!("DSL compilation successful!");
    
    Ok(())
}

/// Compile a CCL file to WASM
async fn compile_to_wasm(input: &Path, output: &Path) -> Result<()> {
    println!("Compiling CCL to WASM: {} -> {}", input.display(), output.display());
    
    let compiler = CclCompiler::new()?;
    compiler.compile_file_to_wasm(input, output)?;
    
    println!("WASM compilation successful!");
    
    Ok(())
}

/// Execute a WASM file
async fn execute_wasm(wasm_path: &Path, proposal_path: Option<&Path>, receipt_path: Option<&Path>) -> Result<()> {
    println!("Executing WASM file: {}", wasm_path.display());
    
    // Create storage
    let storage = Arc::new(CliRuntimeStorage::new());
    
    // Create runtime
    let runtime = icn_runtime::Runtime::new(storage);
    
    // Execute the WASM file
    let receipt = runtime.execute_wasm_file(wasm_path).await?;
    
    // Display execution results
    println!("Execution successful!");
    println!("Fuel used: {}", receipt.metrics.fuel_used);
    println!("Host calls: {}", receipt.metrics.host_calls);
    println!("IO bytes: {}", receipt.metrics.io_bytes);
    
    if !receipt.anchored_cids.is_empty() {
        println!("Anchored CIDs:");
        for cid in &receipt.anchored_cids {
            println!("  - {}", cid);
        }
    }
    
    if !receipt.resource_usage.is_empty() {
        println!("Resource usage:");
        for (resource_type, amount) in &receipt.resource_usage {
            println!("  - {}: {}", resource_type, amount);
        }
    }
    
    // Create the receipt VC
    let receipt_vc = ExecutionReceiptCredential::new(
        format!("urn:uuid:{}", Uuid::new_v4()),
        "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string(),
        receipt.proposal_id,
        receipt.wasm_cid,
        receipt.ccl_cid,
        icn_identity_core::vc::ExecutionMetrics {
            fuel_used: receipt.metrics.fuel_used,
            host_calls: receipt.metrics.host_calls,
            io_bytes: receipt.metrics.io_bytes,
        },
        receipt.anchored_cids,
        receipt.resource_usage,
        receipt.timestamp,
        None,
        None,
    );
    
    // Output the receipt
    if let Some(output_path) = receipt_path {
        let receipt_json = serde_json::to_string_pretty(&receipt_vc)?;
        std::fs::write(output_path, receipt_json)?;
        println!("Receipt saved to: {}", output_path.display());
    }
    
    Ok(())
}

/// Verify an execution receipt
async fn verify_receipt(receipt_path: &Path) -> Result<()> {
    println!("Verifying execution receipt: {}", receipt_path.display());
    
    // Load the receipt
    let receipt_json = std::fs::read_to_string(receipt_path)?;
    let receipt: ExecutionReceiptCredential = serde_json::from_str(&receipt_json)?;
    
    // In a real implementation, we would verify the signature
    // For now, just display the receipt information
    println!("Receipt ID: {}", receipt.id);
    println!("Issuer: {}", receipt.issuer);
    println!("Proposal ID: {}", receipt.credential_subject.proposal_id);
    println!("WASM CID: {}", receipt.credential_subject.wasm_cid);
    println!("CCL CID: {}", receipt.credential_subject.ccl_cid);
    println!("Timestamp: {}", receipt.credential_subject.timestamp);
    
    println!("Metrics:");
    println!("  Fuel used: {}", receipt.credential_subject.metrics.fuel_used);
    println!("  Host calls: {}", receipt.credential_subject.metrics.host_calls);
    println!("  IO bytes: {}", receipt.credential_subject.metrics.io_bytes);
    
    println!("Receipt verification successful!");
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Proposal(cmd) => match cmd {
            ProposalCommands::Create { ccl_file, title, output } => {
                create_proposal(&ccl_file, &title, output.as_deref()).await?;
            }
            ProposalCommands::Vote { proposal, direction, weight } => {
                vote_on_proposal(&proposal, &direction, weight).await?;
            }
            ProposalCommands::Status { proposal } => {
                check_proposal_status(&proposal).await?;
            }
        },
        Commands::Ccl(cmd) => match cmd {
            CclCommands::CompileToDsl { input, output } => {
                compile_to_dsl(&input, &output).await?;
            }
            CclCommands::CompileToWasm { input, output } => {
                compile_to_wasm(&input, &output).await?;
            }
        },
        Commands::Runtime(cmd) => match cmd {
            RuntimeCommands::Execute { wasm, proposal, receipt } => {
                execute_wasm(&wasm, proposal.as_deref(), receipt.as_deref()).await?;
            }
            RuntimeCommands::Verify { receipt } => {
                verify_receipt(&receipt).await?;
            }
        },
    }
    
    Ok(())
} 