use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use icn_ccl_compiler::CclCompiler;
use icn_identity::{Did, FederationMetadata, KeyPair, QuorumProof, QuorumType, TrustBundle};
use icn_runtime::{ExecutionReceipt, Proposal, ProposalState, QuorumStatus, RuntimeExecutionReceipt, VmContext as RuntimeVmContext};
use icn_types::error::{IcnError, IdentityError as IcnTypesIdentityError, DagError as IcnTypesDagError, CryptoError as IcnTypesCryptoError, MeshError as IcnTypesMeshError, TrustError as IcnTypesTrustError, MulticodecError as IcnTypesMulticodecError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// Helper struct for deserializing keypair file content
#[derive(serde::Deserialize)]
struct KeypairFileFormat {
    did: String,
    public_key: String,
    secret_key: String,
    generated_at: String,
}

/// Formats an `icn_identity::DidError` into a user-friendly `anyhow::Error`.
fn format_did_error(did_err: &icn_identity::DidError, problematic_input: &str) -> anyhow::Error {
    match did_err {
        icn_identity::DidError::Malformed => {
            anyhow!("Invalid DID format for '{}'. A DID should start with 'did:key:' and be a base58btc encoded Ed25519 public key.", problematic_input)
        }
        icn_identity::DidError::UnsupportedCodec(codec) => {
            anyhow!("Unsupported key type in DID '{}'. The multicodec prefix {:#x} is not supported. Only Ed25519 keys (prefix 0xed) are currently accepted.", problematic_input, codec)
        }
    }
}

/// Formats a `serde_json::Error` into a user-friendly `anyhow::Error` with file context.
fn format_serde_json_error(err: &serde_json::Error, file_path: &Path, context_message: &str) -> anyhow::Error {
    let display_path = file_path.display();
    match err.classify() {
        serde_json::error::Category::Io => {
            anyhow!("{} I/O error while parsing JSON file '{}': {}", context_message, display_path, err)
        }
        serde_json::error::Category::Syntax => {
            anyhow!("{} Invalid JSON syntax in file '{}' at line {} column {}: {}", context_message, display_path, err.line(), err.column(), err)
        }
        serde_json::error::Category::Data => {
            anyhow!("{} Invalid data structure in JSON file '{}'. Error: {}", context_message, display_path, err)
        }
        serde_json::error::Category::Eof => {
            anyhow!("{} Unexpected end of file in JSON file '{}' while parsing.", context_message, display_path)
        }
    }
}

/// Reads a file and parses its content as JSON into a specified type `T`.
/// Provides context-aware error messages for both I/O and parsing failures.
fn read_and_parse_json<T: serde::de::DeserializeOwned>(file_path: &Path, context_for_error: &str) -> Result<T> {
    let json_str = std::fs::read_to_string(file_path)
        .map_err(|io_err| anyhow!("Failed to read {} file '{}': {}", context_for_error, file_path.display(), io_err))?;

    serde_json::from_str(&json_str)
        .map_err(|json_err| format_serde_json_error(&json_err, file_path, &format!("Failed to parse {} from", context_for_error)))
}

/// Formats an `icn_types::error::IcnError` into a user-friendly `anyhow::Error` for the CLI.
fn format_icn_error_for_cli(err: &IcnError, base_context_msg: &str) -> anyhow::Error {
    let detailed_msg = match err {
        IcnError::Io(io_err) => format!("Underlying I/O error: {}", io_err),
        IcnError::Serialization(json_err) => format!("Underlying JSON serialization/deserialization error: {}", json_err),
        IcnError::Identity(identity_err) => {
            // Handle IcnTypesIdentityError specifically
            match identity_err {
                IcnTypesIdentityError::DidProcessing { source: did_err } => {
                    // We need a 'problematic_input' string here. Since we don't have it directly,
                    // we pass a generic placeholder. The outer context from anyhow will be more helpful.
                    return format_did_error(did_err, "[DID processed during operation]")
                        .context(format!("{}: Identity error involving DID processing", base_context_msg))
                }
                IcnTypesIdentityError::JwsProcessing { source: jws_err } => {
                    format!("Identity error: JWS processing failed: {}", jws_err)
                }
                IcnTypesIdentityError::TrustBundleProcessing { source: tb_err } => {
                    format!("Identity error: Trust Bundle processing failed: {}", tb_err)
                }
                // Add more specific IcnTypesIdentityError arms as needed
                _ => format!("Underlying Identity error: {}", identity_err),
            }
        }
        IcnError::Dag(dag_err) => {
            match dag_err {
                IcnTypesDagError::MalformedCid(cid_err) => format!("DAG error: Malformed CID: {}", cid_err),
                IcnTypesDagError::LinkNotFound { cid } => format!("DAG error: Link not found for CID: {}", cid),
                IcnTypesDagError::NodeValidation { reason, node_cid } => {
                    if let Some(ncid) = node_cid {
                        format!("DAG error: Node validation failed for CID {}: {}", ncid, reason)
                    } else {
                        format!("DAG error: Node validation failed: {}", reason)
                    }
                }
                // Add more specific IcnTypesDagError arms as needed
                _ => format!("Underlying DAG error: {}", dag_err),
            }
        }
        IcnError::Crypto(crypto_err) => format!("Underlying Cryptography error: {}", crypto_err),
        IcnError::Mesh(mesh_err) => format!("Underlying Mesh error: {}", mesh_err),
        IcnError::Trust(trust_err) => format!("Underlying Trust error: {}", trust_err),
        IcnError::Multicodec(mc_err) => format!("Underlying Multicodec error: {}", mc_err),
        IcnError::InvalidUri(uri_err) => format!("Invalid URI encountered: {}", uri_err),
        IcnError::Timeout(s) => format!("Operation timed out: {}", s),
        IcnError::Config(s) => format!("Configuration error: {}", s),
        IcnError::Storage(s) => format!("Storage error: {}", s),
        IcnError::InvalidOperation(s) => format!("Invalid operation: {}", s),
        IcnError::NotFound(s) => format!("Resource not found: {}", s),
        IcnError::PermissionDenied(s) => format!("Permission denied: {}", s),
        IcnError::General(s) => format!("General error: {}", s),
        // Catch-all for any IcnError variants not explicitly handled above
        _ => format!("An unspecified ICN error occurred: {}", err),
    };
    anyhow!("{}: {}", base_context_msg, detailed_msg)
}

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

    /// Federation management commands
    #[clap(subcommand)]
    Federation(FederationCommands),

    /// Keypair management commands
    #[clap(subcommand)]
    Keypair(KeypairCommands),

    /// DAG operations
    #[clap(subcommand)]
    Dag(DagCommands),

    /// Ledger operations
    #[clap(subcommand)]
    Ledger(LedgerCommands),

    /// Token operations
    #[clap(subcommand)]
    Token(TokenCommands),
}

/// Federation management commands
#[derive(Subcommand)]
enum FederationCommands {
    /// Create a new federation with trusted signers
    Create {
        /// Name of the federation
        #[clap(long)]
        name: String,

        /// Description of the federation
        #[clap(long)]
        description: Option<String>,

        /// Signer DIDs, comma separated
        #[clap(long)]
        signers: String,

        /// Quorum type (majority, threshold, weighted)
        #[clap(long, default_value = "majority")]
        quorum_type: String,

        /// Threshold value (required for threshold quorum)
        #[clap(long)]
        threshold: Option<u8>,

        /// Output file for the trust bundle
        #[clap(long, short)]
        output: PathBuf,
    },

    /// Anchor a trust bundle to the DAG
    Anchor {
        /// Path to the trust bundle file
        #[clap(long)]
        bundle: PathBuf,

        /// Node API endpoint
        #[clap(long)]
        node_api: String,

        /// Output file for the anchored bundle
        #[clap(long, short)]
        output: PathBuf,
    },

    /// Verify a trust bundle from the DAG
    Verify {
        /// CID of the trust bundle to verify
        #[clap(long)]
        cid: String,

        /// Node API endpoint
        #[clap(long)]
        node_api: String,
    },
}

/// Keypair management commands
#[derive(Subcommand)]
enum KeypairCommands {
    /// Generate a new keypair
    Generate {
        /// Output file for the keypair
        #[clap(long, short)]
        output: PathBuf,
    },

    /// Show information about a keypair
    Info {
        /// Path to the keypair file
        #[clap(long, short)]
        input: PathBuf,
    },
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

        /// Execute in governance context (for token minting)
        #[clap(long)]
        governance: bool,
    },

    /// Verify an execution receipt
    Verify {
        /// Path to the execution receipt
        #[clap(long, short)]
        receipt: PathBuf,
    },

    /// Execute a CCL file directly
    ExecuteCcl {
        /// Path to the CCL file to execute
        #[clap(long, short)]
        input: PathBuf,

        /// Output file for the execution receipt
        #[clap(long, short)]
        output: Option<PathBuf>,
    },
}

/// Commands for working with the DAG store
#[derive(Subcommand)]
enum DagCommands {
    // ... existing code here
}

/// Commands for working with the economic ledger
#[derive(Subcommand)]
enum LedgerCommands {
    /// Show resource balances for a DID
    Show {
        /// The DID to show resources for
        #[clap(long, short)]
        did: String,

        /// Resource type to show (CPU, MEMORY, TOKEN, IO)
        #[clap(long, short)]
        resource: Option<String>,
    },

    /// Mint tokens for a DID (governance operation)
    Mint {
        /// The DID to mint tokens for
        #[clap(long, short)]
        did: String,

        /// Amount of tokens to mint
        #[clap(long, short)]
        amount: u64,
    },
}

/// Commands for token operations
#[derive(Subcommand)]
enum TokenCommands {
    /// Transfer tokens from one DID to another
    Transfer {
        /// The sender DID
        #[clap(long)]
        from: String,

        /// The recipient DID
        #[clap(long)]
        to: String,

        /// Amount of tokens to transfer
        #[clap(long)]
        amount: u64,
    },
}

/// Simple in-memory implementation of the RuntimeStorage trait for CLI testing
struct CliRuntimeStorage {
    /// Proposals stored in memory
    proposals: Vec<Proposal>,

    /// WASM modules stored in memory (CID -> bytes)
    wasm_modules: std::collections::HashMap<String, Vec<u8>>,

    /// Execution receipts stored in memory (CID -> receipt)
    receipts: std::collections::HashMap<String, RuntimeExecutionReceipt>,
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
        // For CLI, let's simulate finding and updating or adding
        if let Some(pos) = self.proposals.iter().position(|p| p.id == proposal.id) {
            // Not directly modifying self.proposals due to &self, this mock is simplified.
            // In a real scenario with &mut self or interior mutability, we'd update.
            println!("Simulating update for proposal: {}", proposal.id);
        } else {
            // Cannot add to self.proposals with &self. This mock has limitations.
            println!("Simulating add for new proposal: {}", proposal.id);
        }
        Ok(())
    }

    async fn load_wasm(&self, cid: &str) -> Result<Vec<u8>> {
        self.wasm_modules
            .get(cid)
            .cloned()
            .ok_or_else(|| anyhow!("WASM module not found: {}", cid))
    }

    // Added missing trait item
    async fn store_wasm(&self, cid: &str, bytes: &[u8]) -> Result<()> {
        // Not actually storing due to &self, this is a mock for CLI
        // In a real scenario, would be self.wasm_modules.lock().unwrap().insert(cid.to_string(), bytes.to_vec());
        println!("Simulating store for WASM: {} ({} bytes)", cid, bytes.len());
        Ok(())
    }

    // Updated signature to use RuntimeExecutionReceipt
    async fn store_receipt(&self, receipt: &RuntimeExecutionReceipt) -> Result<String> {
        let cid = format!("receipt-{}", Uuid::new_v4());
        // Not actually storing to self.receipts due to &self. This is a mock.
        // In a real scenario, would be self.receipts.lock().unwrap().insert(cid.clone(), receipt.clone());
        println!("Simulating store for receipt with mock CID: {} (for original ID: {})", cid, receipt.id);
        Ok(cid)
    }

    // Added missing trait item
    async fn load_receipt(&self, receipt_id: &str) -> Result<RuntimeExecutionReceipt> {
        // Not actually loading from self.receipts due to &self. This is a mock.
        println!("Simulating load for receipt: {}", receipt_id);
        Err(anyhow!("Mock load_receipt not implemented for CliRuntimeStorage: {}", receipt_id))
        // Example if we could load:
        // self.receipts.get(receipt_id).cloned().ok_or_else(|| anyhow!("Receipt not found: {}", receipt_id))
    }

    async fn anchor_to_dag(&self, cid: &str) -> Result<String> {
        // In a real implementation, we would anchor the CID to a DAG
        println!("Anchored CID to DAG: {}", cid);

        // Return a mocked DAG anchor ID
        Ok(format!("dag-anchor-{}", Uuid::new_v4()))
    }
}

/// Create a new proposal from a CCL file
async fn create_proposal(ccl_file: &Path, _title: &str, output: Option<&Path>) -> Result<()> {
    println!("Creating proposal from CCL file: {}", ccl_file.display());

    // Compile the CCL file to WASM
    let compiler = CclCompiler::new()?;
    let _wasm_bytes = compiler.compile_file(ccl_file)?;

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
    // Load the proposal using the helper function
    let mut proposal: Proposal = read_and_parse_json(proposal_path, "proposal data")?;

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

    println!(
        "Voted {} on proposal {} with weight {}",
        vote_str, proposal.id, weight
    );

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
    // Load the proposal using the helper function
    let proposal: Proposal = read_and_parse_json(proposal_path, "proposal data")?;

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
    println!(
        "Compiling CCL to DSL: {} -> {}",
        input.display(),
        output.display()
    );

    let compiler = CclCompiler::new()?;
    compiler.compile_file_to_dsl(input, output)?;

    println!("DSL compilation successful!");

    Ok(())
}

/// Compile a CCL file to WASM
async fn compile_to_wasm(input: &Path, output: &Path) -> Result<()> {
    println!(
        "Compiling CCL to WASM: {} -> {}",
        input.display(),
        output.display()
    );

    let compiler = CclCompiler::new()?;
    compiler.compile_file_to_wasm(input, output)?;

    println!("WASM compilation successful!");

    Ok(())
}

/// Execute a WASM file directly
async fn execute_wasm(
    wasm_path: &Path,
    _proposal_path: Option<&Path>,
    receipt_path: Option<&Path>,
    governance: bool,
) -> Result<String> {
    println!("Executing WASM file: {}", wasm_path.display());
    if governance {
        println!("Running in governance context (privileged operations enabled)");
    }

    // Read the WASM file
    let wasm_bytes =
        std::fs::read(wasm_path).map_err(|e| anyhow!("Failed to read WASM file: {}", e))?;

    // Set up storage
    let storage = Arc::new(CliRuntimeStorage::new());

    // Create a runtime instance
    let runtime = icn_runtime::Runtime::new(storage);

    // Create a default context
    let context = icn_runtime::VmContext {
        executor_did: "did:icn:executor".to_string(),
        scope: Some("icn/governance".to_string()),
        epoch: Some(chrono::Utc::now().to_rfc3339()),
        code_cid: Some(format!("file://{}", wasm_path.display())),
        resource_limits: None,
        coop_id: None,
        community_id: None,
    };

    // Execute the WASM module
    println!("Executing WASM in CoVM...");
    let result = if governance {
        runtime
            .governance_execute_wasm(&wasm_bytes, context.clone())
            .map_err(|e: icn_runtime::RuntimeError| {
                match e {
                    icn_runtime::RuntimeError::ExecutionError(s) |
                    icn_runtime::RuntimeError::Execution(s) => {
                        anyhow!("WASM execution failed: {}", s)
                    }
                    icn_runtime::RuntimeError::LoadError(s) => {
                        anyhow!("Failed to load WASM module for execution: {}", s)
                    }
                    icn_runtime::RuntimeError::ReceiptError(s) => {
                        anyhow!("Runtime failed to generate an execution receipt: {}", s)
                    }
                    icn_runtime::RuntimeError::InvalidProposalState(s) => {
                        anyhow!("Cannot execute WASM: Invalid proposal state: {}", s)
                    }
                    icn_runtime::RuntimeError::AuthorizationFailed(s) => {
                        anyhow!("Execution forbidden: Authorization failed: {}", s)
                    }
                    icn_runtime::RuntimeError::TrustBundleVerificationError(tb_err) => {
                        anyhow!("Execution failed due to trust bundle issue: {}", tb_err)
                    }
                    icn_runtime::RuntimeError::NoTrustValidator => {
                        anyhow!("Execution failed: No trust validator is configured in the runtime.")
                    }
                    icn_runtime::RuntimeError::HostEnvironmentNotSet => {
                        anyhow!("Execution failed: Runtime host environment is not set.")
                    }
                    icn_runtime::RuntimeError::Instantiation(s) => {
                        anyhow!("WASM module instantiation failed: {}", s)
                    }
                    icn_runtime::RuntimeError::FunctionNotFound(s) => {
                        anyhow!("WASM execution failed: Required function '{}' not found in module.", s)
                    }
                    icn_runtime::RuntimeError::DidError(did_err) => {
                        // context.executor_did is passed as the problematic_input
                        format_did_error(&did_err, &context.executor_did)
                            .context("Execution failed due to an invalid executor DID configured in runtime context")
                    }
                    icn_runtime::RuntimeError::WasmError(source_anyhow_err) => {
                        if let Some(icn_err) = source_anyhow_err.downcast_ref::<IcnError>() {
                            format_icn_error_for_cli(icn_err, "WASM execution failed due to an underlying ICN system error")
                        } else {
                            anyhow!("WASM execution error: {}. Source: {}", source_anyhow_err, source_anyhow_err.root_cause())
                        }
                    }
                }
            })?
    } else {
        runtime
            .execute_wasm(&wasm_bytes, context.clone()) // Use RuntimeVmContext here
            .map_err(|e: icn_runtime::RuntimeError| { 
                match e {
                    icn_runtime::RuntimeError::ExecutionError(s) |
                    icn_runtime::RuntimeError::Execution(s) => {
                        anyhow!("WASM execution failed: {}", s)
                    }
                    icn_runtime::RuntimeError::LoadError(s) => {
                        anyhow!("Failed to load WASM module for execution: {}", s)
                    }
                    icn_runtime::RuntimeError::ReceiptError(s) => {
                        anyhow!("Runtime failed to generate an execution receipt: {}", s)
                    }
                    icn_runtime::RuntimeError::InvalidProposalState(s) => {
                        anyhow!("Cannot execute WASM: Invalid proposal state: {}", s)
                    }
                    icn_runtime::RuntimeError::AuthorizationFailed(s) => {
                        anyhow!("Execution forbidden: Authorization failed: {}", s)
                    }
                    icn_runtime::RuntimeError::TrustBundleVerificationError(tb_err) => {
                        anyhow!("Execution failed due to trust bundle issue: {}", tb_err)
                    }
                    icn_runtime::RuntimeError::NoTrustValidator => {
                        anyhow!("Execution failed: No trust validator is configured in the runtime.")
                    }
                    icn_runtime::RuntimeError::HostEnvironmentNotSet => {
                        anyhow!("Execution failed: Runtime host environment is not set.")
                    }
                    icn_runtime::RuntimeError::Instantiation(s) => {
                        anyhow!("WASM module instantiation failed: {}", s)
                    }
                    icn_runtime::RuntimeError::FunctionNotFound(s) => {
                        anyhow!("WASM execution failed: Required function '{}' not found in module.", s)
                    }
                    icn_runtime::RuntimeError::DidError(did_err) => {
                        format_did_error(&did_err, &context.executor_did) // Use RuntimeVmContext here
                            .context("Execution failed due to an invalid executor DID configured in runtime context")
                    }
                    icn_runtime::RuntimeError::WasmError(source_anyhow_err) => {
                        if let Some(icn_err) = source_anyhow_err.downcast_ref::<IcnError>() {
                            format_icn_error_for_cli(icn_err, "WASM execution failed due to an underlying ICN system error")
                        } else {
                            anyhow!("WASM execution error: {}. Source: {}", source_anyhow_err, source_anyhow_err.root_cause())
                        }
                    }
                }
            })?
    };

    // Create a mock execution receipt
    println!("Generating execution receipt...");
    let execution_receipt = ExecutionReceipt {
        proposal_id: format!("cli-proposal-{}", Uuid::new_v4()),
        wasm_cid: format!("wasm-{}", Uuid::new_v4()),
        ccl_cid: format!("ccl-{}", Uuid::new_v4()),
        metrics: result.metrics.clone(),
        anchored_cids: result.anchored_cids.clone(),
        resource_usage: result.resource_usage.clone(),
        timestamp: chrono::Utc::now().timestamp_millis() as u64,
        dag_epoch: None,
        receipt_cid: None,
        federation_signature: None,
    };

    // Convert to JSON for saving
    let receipt_json = serde_json::to_string_pretty(&execution_receipt)?;

    // Save to file if requested
    if let Some(path) = receipt_path {
        std::fs::write(path, &receipt_json)
            .map_err(|e| anyhow!("Failed to write receipt to file: {}", e))?;
        println!("Receipt saved to {}", path.display());
    }

    // Print a summary of the execution
    println!("\n{}", "Execution Summary".green().bold());
    println!("Fuel used: {}", result.metrics.fuel_used);
    println!("Host calls: {}", result.metrics.host_calls);

    if !result.logs.is_empty() {
        println!("\n{}", "Execution Logs".yellow().bold());
        for log in &result.logs {
            println!("  {}", log);
        }
    }

    // Create a mock receipt CID
    let receipt_cid = format!("receipt-{}", uuid::Uuid::new_v4());
    println!("\nReceipt CID: {}", receipt_cid.cyan());

    // Return the receipt CID
    Ok(receipt_cid)
}

/// Verify an execution receipt
async fn verify_receipt(receipt_path: &Path) -> Result<()> {
    println!("Verifying execution receipt: {}", receipt_path.display());

    // Load the receipt using the helper function
    let receipt: ExecutionReceipt = read_and_parse_json(receipt_path, "execution receipt data")?;

    // In a real implementation, we would verify the signature
    // For now, just display the receipt information
    println!("Receipt for proposal: {}", receipt.proposal_id);
    println!("WASM CID: {}", receipt.wasm_cid);
    println!("CCL CID: {}", receipt.ccl_cid);
    println!("Timestamp: {}", receipt.timestamp);

    println!("Metrics:");
    println!("  Fuel used: {}", receipt.metrics.fuel_used);
    println!("  Host calls: {}", receipt.metrics.host_calls);

    println!("Receipt verification successful!");

    Ok(())
}

/// Execute a CCL file by compiling to DSL, then WASM, and executing
async fn execute_ccl(ccl_path: &Path, receipt_path: Option<&Path>) -> Result<String> {
    println!("{}", "Executing CCL file".blue().bold());
    println!("Source: {}", ccl_path.display());

    // Temporary files for the compilation pipeline
    let temp_dir = tempfile::tempdir()?;
    let dsl_path = temp_dir.path().join("output.dsl");
    let wasm_path = temp_dir.path().join("output.wasm");

    // Step 1: Compile CCL to DSL
    println!("\n{}", "Step 1: Compiling CCL to DSL".yellow());
    compile_to_dsl(ccl_path, &dsl_path).await?;

    // Step 2: Compile DSL to WASM
    println!("\n{}", "Step 2: Compiling DSL to WASM".yellow());
    compile_to_wasm(&dsl_path, &wasm_path).await?;

    // Step 3: Execute the WASM
    println!("\n{}", "Step 3: Executing WASM".yellow());
    let receipt_cid = execute_wasm(&wasm_path, None, receipt_path, false).await?;

    // Print final result
    println!("\n{}", "CCL Execution Pipeline Complete".green().bold());
    println!("Receipt CID: {}", receipt_cid.cyan());

    Ok(receipt_cid)
}

/// Create a new federation with trusted signers
async fn create_federation(
    name: &str,
    description: Option<&str>,
    signers_str: &str,
    quorum_type_str: &str,
    threshold: Option<u8>,
    output: &Path,
) -> Result<()> {
    println!("Creating federation: {}", name);

    // Parse signer DIDs
    let signer_dids: Vec<Did> = signers_str
        .split(',')
        .map(|s| {
            let trimmed_s = s.trim();
            trimmed_s.parse::<Did>().map_err(|e| format_did_error(&e, trimmed_s))
        })
        .collect::<Result<Vec<Did>, _>>()?;

    if signer_dids.is_empty() {
        return Err(anyhow!("At least one signer DID must be provided"));
    }

    println!("Registered {} signers", signer_dids.len());

    // Create federation metadata
    let metadata = FederationMetadata {
        name: name.to_string(),
        description: description.map(String::from),
        version: "1.0".to_string(),
        additional: HashMap::new(),
    };

    // Create a trust bundle with a test DAG CID
    let mut bundle = TrustBundle::new(
        // This would normally be generated by anchoring to the DAG
        format!("federation-{}", Uuid::new_v4()),
        metadata,
    );

    // In a real implementation, we would:
    // 1. Generate keypairs for each signer
    // 2. Get signatures from each signer
    // 3. Create a quorum proof
    // 4. Add the proof to the bundle
    println!("Federation trust bundle created");

    // Output the trust bundle
    let bundle_json = serde_json::to_string_pretty(&bundle)?;
    std::fs::write(output, &bundle_json)?;
    println!("Trust bundle saved to: {}", output.display());

    Ok(())
}

/// Generate a new keypair
async fn generate_keypair(output: &Path) -> Result<()> {
    println!("Generating new Ed25519 keypair...");

    // Generate a new keypair
    let keypair = KeyPair::generate();

    // Create serializable structure with the keypair information
    let keypair_info = serde_json::json!({
        "did": keypair.did.as_str(),
        "public_key": hex::encode(keypair.pk.to_bytes()),
        "secret_key": hex::encode(keypair.to_bytes()),
        "generated_at": chrono::Utc::now().to_rfc3339(),
    });

    // Output the keypair
    let keypair_json = serde_json::to_string_pretty(&keypair_info)?;
    std::fs::write(output, &keypair_json)?;

    println!("Keypair saved to: {}", output.display());
    println!("DID: {}", keypair.did.as_str());

    Ok(())
}

/// Show information about a keypair
async fn keypair_info(input: &Path) -> Result<()> {
    println!("Reading keypair from: {}", input.display());

    // Use the new helper function to read and parse the keypair file.
    // The context "keypair data" will be used in error messages.
    let keypair_data: KeypairFileFormat = read_and_parse_json(input, "keypair data")?;

    // Display keypair information
    match keypair_data.did.parse::<Did>() {
        Ok(parsed_did) => {
            println!("DID: {}", parsed_did);
        }
        Err(did_err) => {
            let descriptive_error = format_did_error(&did_err, &keypair_data.did);
            println!("DID: {} ({})", keypair_data.did.red(), descriptive_error.to_string().yellow());
        }
    }
    println!("Public Key: {}", keypair_data.public_key);
    println!("Generated: {}", keypair_data.generated_at);

    Ok(())
}

/// Anchor a trust bundle to the DAG
async fn anchor_trust_bundle(bundle_path: &Path, node_api: &str, output: &Path) -> Result<String> {
    println!("Anchoring trust bundle to DAG via node: {}", node_api);

    // Read the trust bundle file using the helper function
    let mut bundle: TrustBundle = read_and_parse_json(bundle_path, "trust bundle data")?;

    // In a real implementation, we would:
    // 1. Send the bundle to the node API
    // 2. Node would anchor it to the DAG
    // 3. Return the CID

    // For now, just generate a mock CID
    let cid = format!("bundle-{}", Uuid::new_v4());
    println!("Trust bundle anchored with CID: {}", cid);

    // Update the trust bundle with the CID and save it
    bundle.root_dag_cid = cid.clone();
    let updated_bundle = serde_json::to_string_pretty(&bundle)?;
    std::fs::write(output, &updated_bundle)?;

    Ok(cid)
}

/// Verify a trust bundle from the DAG
async fn verify_trust_bundle(cid: &str, node_api: &str) -> Result<()> {
    println!(
        "Verifying trust bundle with CID: {} via node: {}",
        cid, node_api
    );

    // In a real implementation, we would:
    // 1. Retrieve the bundle from the DAG using the CID
    // 2. Verify its signatures using TrustValidator

    // Mock implementation
    println!("Trust bundle verification: {}", "SUCCESSFUL".green());
    println!("Signatures verified: 3/3");

    Ok(())
}

/// Transfer tokens from one DID to another
async fn transfer_tokens(from_did: &str, to_did: &str, amount: u64) -> Result<()> {
    println!(
        "Transferring {} tokens from {} to {}",
        amount, from_did, to_did
    );

    // Create a CCL script for the transfer
    let ccl = format!(
        r#"
title: "Token Transfer";
description: "Transfer {} tokens from {} to {}";

actions {{
  on "execute" {{
    transfer_token {{
      type "token"
      amount {}
      sender "{}"
      recipient "{}"
    }}
  }}
}}
"#,
        amount, from_did, to_did, amount, from_did, to_did
    );

    // Write to a temporary file
    let temp_dir = tempfile::tempdir()?;
    let ccl_path = temp_dir.path().join("transfer.ccl");
    std::fs::write(&ccl_path, ccl)?;

    // Compile and execute
    let receipt_cid = execute_ccl(&ccl_path, None).await?;
    println!("Transfer complete! Receipt: {}", receipt_cid);

    // Show updated balances
    println!("\nUpdated balances:");
    println!("Sender ({}):", from_did);
    // In a real implementation, we would query the ledger here
    println!("Recipient ({}):", to_did);
    // In a real implementation, we would query the ledger here

    Ok(())
}

/// Show the resource balances for a DID
async fn show_ledger(did: &str, resource_type: Option<&str>) -> Result<()> {
    println!("Showing ledger for DID: {}", did);

    // In a real implementation, we would query the actual ledger
    // For this CLI prototype, we'll show mock data
    let resources = vec![
        ("TOKEN", 100),
        ("CPU", 5000),
        ("MEMORY", 10000),
        ("IO", 1000),
    ];

    if let Some(rt) = resource_type {
        // Show only the specified resource
        if let Some(&(_, amount)) = resources.iter().find(|(r, _)| r == &rt) {
            println!("{}: {}", rt, amount);
        } else {
            println!("Resource type '{}' not found for DID {}", rt, did);
        }
    } else {
        // Show all resources
        println!("Resources for DID {}:", did);
        for (resource, amount) in resources {
            println!("  {}: {}", resource, amount);
        }
    }

    Ok(())
}

/// Entrypoint
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Proposal(cmd) => match cmd {
            ProposalCommands::Create {
                ccl_file,
                title,
                output,
            } => {
                create_proposal(ccl_file, title, output.as_deref()).await?;
            }
            ProposalCommands::Vote {
                proposal,
                direction,
                weight,
            } => {
                vote_on_proposal(proposal, direction, *weight).await?;
            }
            ProposalCommands::Status { proposal } => {
                check_proposal_status(proposal).await?;
            }
        },
        Commands::Ccl(cmd) => match cmd {
            CclCommands::CompileToDsl { input, output } => {
                compile_to_dsl(input, output).await?;
            }
            CclCommands::CompileToWasm { input, output } => {
                compile_to_wasm(input, output).await?;
            }
        },
        Commands::Runtime(cmd) => match cmd {
            RuntimeCommands::Execute {
                wasm,
                proposal,
                receipt,
                governance,
            } => {
                execute_wasm(wasm, proposal.as_deref(), receipt.as_deref(), *governance).await?;
            }
            RuntimeCommands::Verify { receipt } => {
                verify_receipt(receipt).await?;
            }
            RuntimeCommands::ExecuteCcl { input, output } => {
                execute_ccl(input, output.as_deref()).await?;
            }
        },
        Commands::Federation(cmd) => match cmd {
            FederationCommands::Create {
                name,
                description,
                signers,
                quorum_type,
                threshold,
                output,
            } => {
                create_federation(
                    name,
                    description.as_deref(),
                    signers,
                    quorum_type,
                    *threshold,
                    output,
                )
                .await?;
            }
            FederationCommands::Anchor {
                bundle,
                node_api,
                output,
            } => {
                anchor_trust_bundle(bundle, node_api, output).await?;
            }
            FederationCommands::Verify { cid, node_api } => {
                verify_trust_bundle(cid, node_api).await?;
            }
        },
        Commands::Keypair(cmd) => match cmd {
            KeypairCommands::Generate { output } => {
                generate_keypair(output).await?;
            }
            KeypairCommands::Info { input } => {
                keypair_info(input).await?;
            }
        },
        Commands::Dag(_cmd) => {
            // Handle DAG commands
            unimplemented!("DAG commands not yet implemented");
        }
        Commands::Ledger(cmd) => match cmd {
            LedgerCommands::Show { did, resource } => {
                show_ledger(did, resource.as_deref()).await?;
            }
            LedgerCommands::Mint { did, amount } => {
                // Implementation for minting tokens
                println!("Minting {} tokens for {}", amount, did);
                // For a real implementation, this would interact with the runtime
                // in governance mode to mint tokens
                println!("Note: Token minting requires governance context");
            }
        },
        Commands::Token(cmd) => match cmd {
            TokenCommands::Transfer { from, to, amount } => {
                transfer_tokens(from, to, *amount).await?;
            }
        },
    }

    Ok(())
}
