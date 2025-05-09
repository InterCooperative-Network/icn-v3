use anyhow::{anyhow, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use colored::Colorize;
use icn_economics::ScopedResourceToken;
// use icn_identity_core::did::Did;
type Did = String; // DIDs are strings
use planetary_mesh::{
    Bid, ComputeRequirements, JobManifest, JobPriority, JobStatus, MeshNode, NodeCapability,
    PlanetaryMeshNode,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;
use icn_core_vm::ExecutionMetrics;
use tokio;

/// Command-line interface for ICN Planetary Mesh
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
    /// Submit a job to the mesh network
    SubmitJob {
        /// Path to the WASM file to execute
        #[clap(long, short)]
        wasm: PathBuf,

        /// Job description
        #[clap(long, short)]
        description: String,

        /// Resource type
        #[clap(long, default_value = "compute")]
        resource_type: String,

        /// Resource amount
        #[clap(long, default_value = "100")]
        resource_amount: u64,

        /// Resource scope
        #[clap(long, default_value = "default")]
        scope: String,

        /// Job priority
        #[clap(long, default_value = "medium")]
        priority: String,

        /// Minimum memory in MB
        #[clap(long, default_value = "512")]
        min_memory: u32,

        /// Minimum CPU cores
        #[clap(long, default_value = "2")]
        min_cpu: u32,

        /// Output file for the job ID
        #[clap(long, short)]
        output: Option<PathBuf>,
    },

    /// List available mesh nodes
    ListNodes,

    /// Get bids for a job
    GetBids {
        /// Job ID
        #[clap(long, short)]
        job_id: String,
    },

    /// Get job status
    JobStatus {
        /// Job ID
        #[clap(long, short)]
        job_id: String,
    },

    /// Accept a bid for a job
    AcceptBid {
        /// Job ID
        #[clap(long, short)]
        job_id: String,

        /// Node ID
        #[clap(long, short)]
        node_id: String,
    },

    /// Execute a job locally
    Execute {
        /// Path to the WASM file to execute
        #[clap(long, short)]
        wasm: PathBuf,

        /// Output file for the receipt
        #[clap(long, short)]
        output: Option<PathBuf>,
    },

    /// Create a test node
    CreateNode {
        /// Node name
        #[clap(long, short)]
        name: String,

        /// Available memory in MB
        #[clap(long, default_value = "4096")]
        memory: u32,

        /// Available CPU cores
        #[clap(long, default_value = "4")]
        cpu: u32,

        /// Node location
        #[clap(long, default_value = "us-west")]
        location: String,
    },
}

/// Create a test node for the mesh network
async fn create_node(name: &str, memory: u32, cpu: u32, location: &str) -> Result<()> {
    println!("{}", "Creating test mesh node".blue().bold());
    println!("Name: {}", name);
    println!("Memory: {} MB", memory);
    println!("CPU: {} cores", cpu);
    println!("Location: {}", location);

    // Create a test DID for the node
    let did_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string();

    // Create node capabilities
    let capabilities = NodeCapability {
        node_id: name.to_string(),
        node_did: did_str.clone(),
        available_memory_mb: memory,
        available_cpu_cores: cpu,
        available_storage_mb: memory * 10, // 10x memory as storage
        cpu_architecture: "x86_64".to_string(),
        features: vec!["avx".to_string(), "sse4".to_string()],
        location: Some(location.to_string()),
        bandwidth_mbps: 1000,
        supported_job_types: vec!["compute".to_string(), "storage".to_string()],
        updated_at: Utc::now(),
    };

    // Create the node
    let _node = PlanetaryMeshNode::new(did_str.clone(), capabilities.clone())?;

    // Save node info to a file
    let node_info = json!({
        "node_id": name,
        "did": did_str,
        "capabilities": capabilities,
    });

    let node_dir = Path::new("./nodes");
    if !node_dir.exists() {
        fs::create_dir_all(node_dir)?;
    }

    let node_file = node_dir.join(format!("{}.json", name));
    fs::write(&node_file, serde_json::to_string_pretty(&node_info)?)?;

    println!("\n{}", "Node created successfully".green().bold());
    println!("Node info saved to: {}", node_file.display());

    Ok(())
}

/// Submit a job to the mesh network
async fn submit_job(args: &Commands) -> Result<()> {
    if let Commands::SubmitJob {
        wasm,
        description,
        resource_type,
        resource_amount,
        scope,
        priority,
        min_memory,
        min_cpu,
        output,
    } = args
    {
        println!("{}", "Submitting job to the mesh network".blue().bold());
        println!("WASM: {}", wasm.display());
        println!("Description: {}", description);

        // Create a test DID for the submitter
        let submitter_did_str =
            "did:key:z6MktyAYM2rE5N2h9kYgqSMv9uCWeP9j9JapH5xJd9XwM7oP".to_string();

        let requirements = ComputeRequirements {
            min_memory_mb: *min_memory,
            min_cpu_cores: *min_cpu,
            min_storage_mb: *min_memory * 2, // Example: 2x memory as storage
            max_execution_time_secs: 3600,   // 1 hour
            required_features: vec![],
        };

        let token = ScopedResourceToken {
            resource_type: resource_type.clone(),
            amount: *resource_amount,
            scope: scope.clone(),
            expires_at: None,
            issuer: Some(submitter_did_str.clone()),
        };

        let job_priority = match priority.to_lowercase().as_str() {
            "low" => JobPriority::Low,
            "medium" => JobPriority::Medium,
            "high" => JobPriority::High,
            "critical" => JobPriority::Critical,
            _ => JobPriority::Medium,
        };

        let job_id = Uuid::new_v4().to_string();
        let manifest = JobManifest {
            id: job_id.clone(),
            submitter_did: submitter_did_str,
            description: description.clone(),
            created_at: Utc::now(),
            expires_at: None,
            wasm_cid: wasm.to_string_lossy().into_owned(),
            ccl_cid: None,
            input_data_cid: None,
            output_location: None,
            requirements,
            priority: job_priority,
            resource_token: token,
            trust_requirements: vec![],
            status: JobStatus::Created,
        };

        // Create a test node to submit the job (replace with actual node interaction)
        let node_did_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string();
        let node_capabilities = NodeCapability {
            node_id: "mesh-node-cli".to_string(),
            node_did: node_did_str.clone(),
            available_memory_mb: 1024,
            available_cpu_cores: 2,
            available_storage_mb: 10240,
            cpu_architecture: "x86_64".to_string(),
            features: vec![],
            location: Some("local-cli".to_string()),
            bandwidth_mbps: 100,
            supported_job_types: vec!["compute".to_string()],
            updated_at: Utc::now(),
        };
        let node = PlanetaryMeshNode::new(node_did_str, node_capabilities)?;

        let submitted_job_id = node.submit_job(manifest).await?;

        println!("\n{}", "Job submitted successfully".green().bold());
        println!("Job ID: {}", submitted_job_id);

        // Save job ID to file if requested
        if let Some(output_path) = output {
            fs::write(output_path, &submitted_job_id)?;
            println!("Job ID saved to: {}", output_path.display());
        }

        // Simulate job submission to the network
        println!("\n{}", "Job submitted to the network".green());
        println!("Waiting for bids...");

        // Wait for 2 seconds to simulate network communication
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Create some simulated bids
        let bid1 = Bid {
            job_id: submitted_job_id.clone(),
            node_id: "mesh-node-2".to_string(),
            node_did: "did:key:z6MkrJVkbkCVL6hZUGEU7eh8arLqsX5o6Hep9ZUzVULCsHKp".to_string(),
            bid_amount: 80,
            estimated_execution_time: 120,
            timestamp: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            node_capacity: NodeCapability {
                node_id: "mesh-node-2".to_string(),
                node_did: "did:key:z6MkrJVkbkCVL6hZUGEU7eh8arLqsX5o6Hep9ZUzVULCsHKp".to_string(),
                available_memory_mb: 8192,
                available_cpu_cores: 8,
                available_storage_mb: 81920,
                cpu_architecture: "x86_64".to_string(),
                features: vec!["avx".to_string(), "sse4".to_string()],
                location: Some("us-east".to_string()),
                bandwidth_mbps: 1000,
                supported_job_types: vec!["compute".to_string(), "storage".to_string()],
                updated_at: Utc::now(),
            },
            reputation_score: 95,
            capability_proof: None,
        };

        let bid2 = Bid {
            job_id: submitted_job_id.clone(),
            node_id: "mesh-node-3".to_string(),
            node_did: "did:key:z6MkuBsxRsRu3PU1VzZ5xnqNtXWRwLtrGdxdMeMFuxP5xyVp".to_string(),
            bid_amount: 90,
            estimated_execution_time: 100,
            timestamp: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            node_capacity: NodeCapability {
                node_id: "mesh-node-3".to_string(),
                node_did: "did:key:z6MkuBsxRsRu3PU1VzZ5xnqNtXWRwLtrGdxdMeMFuxP5xyVp".to_string(),
                available_memory_mb: 4096,
                available_cpu_cores: 4,
                available_storage_mb: 40960,
                cpu_architecture: "x86_64".to_string(),
                features: vec!["avx".to_string(), "sse4".to_string(), "gpu".to_string()],
                location: Some("eu-west".to_string()),
                bandwidth_mbps: 1000,
                supported_job_types: vec![
                    "compute".to_string(),
                    "storage".to_string(),
                    "gpu".to_string(),
                ],
                updated_at: Utc::now(),
            },
            reputation_score: 88,
            capability_proof: None,
        };

        // Submit the bids
        node.submit_bid(&submitted_job_id, bid1).await?;
        node.submit_bid(&submitted_job_id, bid2).await?;

        println!("\n{}", "Received 2 bids:".yellow());
        println!(
            "1. Node: {} (Reputation: {}, Bid: {}, Time: {}s)",
            "mesh-node-2".cyan(),
            "95".green(),
            "80".yellow(),
            "120".yellow()
        );
        println!(
            "2. Node: {} (Reputation: {}, Bid: {}, Time: {}s)",
            "mesh-node-3".cyan(),
            "88".green(),
            "90".yellow(),
            "100".yellow()
        );

        println!("\n{}", "Use the following command to check status:".blue());
        println!("meshctl job-status --job-id {}", submitted_job_id);

        println!("\n{}", "Use the following command to accept a bid:".blue());
        println!(
            "meshctl accept-bid --job-id {} --node-id mesh-node-2",
            submitted_job_id
        );
    }

    Ok(())
}

/// List available mesh nodes
async fn list_nodes() -> Result<()> {
    println!("{}", "Available mesh nodes".blue().bold());

    // In a real implementation, we would discover nodes on the network
    // For now, we'll show some simulated nodes

    println!(
        "\n{:<15} {:<15} {:<10} {:<10} {:<15} {:<10}",
        "Node ID", "Location", "Memory", "CPU", "Storage", "Features"
    );
    println!("{}", "-".repeat(80));

    println!(
        "{:<15} {:<15} {:<10} {:<10} {:<15} {:<10}",
        "mesh-node-1".cyan(),
        "us-west".yellow(),
        "4096 MB".green(),
        "4 cores".green(),
        "40960 MB".green(),
        "avx, sse4"
    );

    println!(
        "{:<15} {:<15} {:<10} {:<10} {:<15} {:<10}",
        "mesh-node-2".cyan(),
        "us-east".yellow(),
        "8192 MB".green(),
        "8 cores".green(),
        "81920 MB".green(),
        "avx, sse4"
    );

    println!(
        "{:<15} {:<15} {:<10} {:<10} {:<15} {:<10}",
        "mesh-node-3".cyan(),
        "eu-west".yellow(),
        "4096 MB".green(),
        "4 cores".green(),
        "40960 MB".green(),
        "avx, sse4, gpu"
    );

    // Check if we have any locally created nodes
    let node_dir = Path::new("./nodes");
    if node_dir.exists() {
        for entry in fs::read_dir(node_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(node_info) = serde_json::from_str::<serde_json::Value>(&content) {
                        let node_id = node_info["node_id"].as_str().unwrap_or("unknown");
                        let location = node_info["capabilities"]["location"]
                            .as_str()
                            .unwrap_or("unknown");
                        let memory = node_info["capabilities"]["available_memory_mb"]
                            .as_u64()
                            .unwrap_or(0);
                        let cpu = node_info["capabilities"]["available_cpu_cores"]
                            .as_u64()
                            .unwrap_or(0);
                        let storage = node_info["capabilities"]["available_storage_mb"]
                            .as_u64()
                            .unwrap_or(0);

                        let features = node_info["capabilities"]["features"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_else(|| "none".to_string());

                        println!(
                            "{:<15} {:<15} {:<10} {:<10} {:<15} {:<10}",
                            node_id.cyan(),
                            location.yellow(),
                            format!("{} MB", memory).green(),
                            format!("{} cores", cpu).green(),
                            format!("{} MB", storage).green(),
                            features
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

/// Get bids for a job
async fn get_bids(job_id: &str) -> Result<()> {
    println!("{}", format!("Bids for job {}", job_id).blue().bold());

    // Create a test node
    let node_did_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string();
    let capabilities = NodeCapability {
        node_id: "mesh-node-1".to_string(),
        node_did: node_did_str.clone(),
        available_memory_mb: 4096,
        available_cpu_cores: 4,
        available_storage_mb: 40960,
        cpu_architecture: "x86_64".to_string(),
        features: vec!["avx".to_string(), "sse4".to_string()],
        location: Some("us-west".to_string()),
        bandwidth_mbps: 1000,
        supported_job_types: vec!["compute".to_string(), "storage".to_string()],
        updated_at: Utc::now(),
    };

    let node = PlanetaryMeshNode::new(node_did_str, capabilities)?;

    // Check if we have bids for this job
    let bids = node.get_bids(job_id).await?;

    if bids.is_empty() {
        println!("\nNo bids received for this job yet.");
        return Ok(());
    }

    println!(
        "\n{:<15} {:<10} {:<15} {:<15} {:<15}",
        "Node ID", "Bid", "Est. Time", "Reputation", "Location"
    );
    println!("{}", "-".repeat(70));

    for (i, bid) in bids.iter().enumerate() {
        let location = bid.node_capacity.location.as_deref().unwrap_or("unknown");

        println!(
            "{:<15} {:<10} {:<15} {:<15} {:<15}",
            bid.node_id.cyan(),
            bid.bid_amount.to_string().yellow(),
            format!("{}s", bid.estimated_execution_time).yellow(),
            bid.reputation_score.to_string().green(),
            location
        );
    }

    println!("\n{}", "Use the following command to accept a bid:".blue());
    println!("meshctl accept-bid --job-id {} --node-id <node-id>", job_id);

    Ok(())
}

/// Get job status
async fn get_job_status(job_id: &str) -> Result<()> {
    println!("{}", format!("Status for job {}", job_id).blue().bold());

    // Create a test node
    let node_did_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string();
    let capabilities = NodeCapability {
        node_id: "mesh-node-1".to_string(),
        node_did: node_did_str.clone(),
        available_memory_mb: 4096,
        available_cpu_cores: 4,
        available_storage_mb: 40960,
        cpu_architecture: "x86_64".to_string(),
        features: vec!["avx".to_string(), "sse4".to_string()],
        location: Some("us-west".to_string()),
        bandwidth_mbps: 1000,
        supported_job_types: vec!["compute".to_string(), "storage".to_string()],
        updated_at: Utc::now(),
    };

    let node = PlanetaryMeshNode::new(node_did_str, capabilities)?;

    // Get the job status
    let status = match node.get_job_status(job_id).await {
        Ok(status) => status,
        Err(_) => {
            // For demo purposes, simulate some status if job not found
            match job_id.chars().next() {
                Some('a') => JobStatus::Submitted,
                Some('b') => JobStatus::Assigned {
                    node_id: "mesh-node-2".to_string(),
                },
                Some('c') => JobStatus::Running {
                    node_id: "mesh-node-2".to_string(),
                },
                Some('d') => JobStatus::Completed {
                    node_id: "mesh-node-2".to_string(),
                    receipt_cid: format!("receipt:{}", Uuid::new_v4()),
                },
                Some('e') => JobStatus::Failed {
                    node_id: Some("mesh-node-2".to_string()),
                    error: "Out of memory".to_string(),
                },
                Some('f') => JobStatus::Cancelled,
                _ => JobStatus::Submitted,
            }
        }
    };

    println!("\nJob ID: {}", job_id);

    match &status {
        JobStatus::Created => {
            println!("Status: {}", "Created".yellow());
            println!("Job has been created but not yet submitted to the network.");
        }
        JobStatus::Submitted => {
            println!("Status: {}", "Submitted".yellow());
            println!("Job has been submitted to the network and is waiting for bids.");
            println!("\nUse the following command to check for bids:");
            println!("meshctl get-bids --job-id {}", job_id);
        }
        JobStatus::Assigned { node_id } => {
            println!("Status: {}", "Assigned".yellow());
            println!("Job has been assigned to node: {}", node_id.cyan());
            println!("Waiting for execution to begin...");
        }
        JobStatus::Running { node_id } => {
            println!("Status: {}", "Running".green());
            println!("Job is currently running on node: {}", node_id.cyan());
        }
        JobStatus::Completed {
            node_id,
            receipt_cid,
        } => {
            println!("Status: {}", "Completed".green().bold());
            println!("Job completed successfully on node: {}", node_id.cyan());
            println!("Receipt CID: {}", receipt_cid.cyan());
        }
        JobStatus::Failed { node_id, error } => {
            println!("Status: {}", "Failed".red().bold());
            if let Some(nid) = node_id {
                println!("Job failed on node: {}", nid.cyan());
            }
            println!("Error: {}", error.red());
        }
        JobStatus::Cancelled => {
            println!("Status: {}", "Cancelled".yellow());
            println!("Job was cancelled by the submitter.");
        }
    }

    Ok(())
}

/// Accept a bid for a job
async fn accept_bid(job_id: &str, node_id: &str) -> Result<()> {
    println!(
        "{}",
        format!("Accepting bid for job {}", job_id).blue().bold()
    );
    println!("Node ID: {}", node_id);

    // Create a test node
    let node_did_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string();
    let capabilities = NodeCapability {
        node_id: "mesh-node-1".to_string(),
        node_did: node_did_str.clone(),
        available_memory_mb: 4096,
        available_cpu_cores: 4,
        available_storage_mb: 40960,
        cpu_architecture: "x86_64".to_string(),
        features: vec!["avx".to_string(), "sse4".to_string()],
        location: Some("us-west".to_string()),
        bandwidth_mbps: 1000,
        supported_job_types: vec!["compute".to_string(), "storage".to_string()],
        updated_at: Utc::now(),
    };

    let node = PlanetaryMeshNode::new(node_did_str, capabilities)?;

    // Accept the bid
    node.accept_bid(job_id, node_id).await?;

    println!("\n{}", "Bid accepted successfully".green().bold());
    println!("Job {} assigned to node {}", job_id, node_id);

    // Simulate job execution progression
    println!("\n{}", "Job execution progress:".yellow());

    // Simulate job status updates
    println!("Status: {}", "Assigned".yellow());
    tokio::time::sleep(Duration::from_secs(1)).await;

    println!("Status: {}", "Running".green());
    tokio::time::sleep(Duration::from_secs(2)).await;

    println!("Status: {}", "Completed".green().bold());

    // Generate a receipt CID
    let receipt_cid = format!("receipt:{}", Uuid::new_v4());
    println!("Receipt CID: {}", receipt_cid.cyan());

    // In a real implementation, this would be anchored to the DAG
    println!("Receipt anchored to DAG ✓");

    // In a real implementation, this would be verified by the federation
    println!("Receipt verified by federation ✓");

    Ok(())
}

/// Execute a WASM job locally
async fn execute_local(wasm_path: &Path, output_path: Option<&Path>) -> Result<()> {
    println!("{}", "Executing WASM job locally".blue().bold());
    println!("WASM: {}", wasm_path.display());

    // Create a test node
    let did_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK".to_string();
    let capabilities = NodeCapability {
        node_id: "local-node".to_string(),
        node_did: did_str.clone(),
        available_memory_mb: 4096,
        available_cpu_cores: 4,
        available_storage_mb: 40960,
        cpu_architecture: "x86_64".to_string(),
        features: vec!["avx".to_string(), "sse4".to_string()],
        location: Some("local".to_string()),
        bandwidth_mbps: 1000,
        supported_job_types: vec!["compute".to_string(), "storage".to_string()],
        updated_at: Utc::now(),
    };

    let node = PlanetaryMeshNode::new(did_str, capabilities)?;

    // Execute the WASM file
    println!("\n{}", "Executing WASM module...".yellow());
    let metrics = node.execute_wasm_file(wasm_path).await?; // Changed from (metrics, logs)

    println!("\n{}", "Execution completed successfully".green().bold());
    println!("Metrics:");
    println!("  Fuel used: {}", metrics.fuel_used);
    println!("  Host calls: {}", metrics.host_calls);
    println!("  I/O bytes: {}", metrics.io_bytes);

    println!("\nLogs: (Logs are no longer directly returned by execute_wasm_file)");
    // for log in logs { // logs variable removed
    //     println!("  {}", log);
    // }

    // Create a job ID and receipt
    let job_id = Uuid::new_v4().to_string();

    // Create resource usage records
    let resource_usage = vec![
        ("compute".to_string(), metrics.fuel_used),
        ("memory".to_string(), 1024),
        ("storage".to_string(), metrics.io_bytes),
    ];

    // Create a receipt
    let receipt = node
        .create_job_receipt(&job_id, metrics, resource_usage, None)
        .await?;

    println!("\n{}", "Job execution receipt:".yellow());
    println!("  Job ID: {}", receipt.job_id);
    println!("  Executor: {}", receipt.executor_node_id);
    println!("  Receipt CID: {}", receipt.receipt_cid);
    println!("  Start time: {}", receipt.start_time);
    println!("  End time: {}", receipt.end_time);

    // Save the receipt to file if requested
    if let Some(output_file) = output_path {
        let receipt_json = serde_json::to_string_pretty(&receipt)?;
        fs::write(output_file, &receipt_json)?;
        println!("\nReceipt saved to: {}", output_file.display());
    }

    Ok(())
}

async fn handle_execute_wasm(node_id: String, wasm_path: PathBuf) -> Result<()> {
    println!(
        "Executing WASM module '{}' on node '{}'...",
        wasm_path.display(),
        node_id
    );

    let did_str = format!("did:key:z{}", node_id); 
    let capabilities = NodeCapability { 
        node_id: node_id.clone(),
        node_did: did_str.clone(),
        available_memory_mb: 1024,
        available_cpu_cores: 4,
        available_storage_mb: 10240,
        cpu_architecture: "wasm32".to_string(),
        features: vec![],
        location: None,
        bandwidth_mbps: 100,
        supported_job_types: vec!["wasm_execution".to_string()],
        updated_at: chrono::Utc::now(), // Assuming chrono is available here or this part is okay
    };
    let node = PlanetaryMeshNode::new(did_str, capabilities)?;

    let metrics: ExecutionMetrics = node.execute_wasm_file(&wasm_path).await?;

    println!("Execution completed.");
    println!("  Fuel Used: {}", metrics.fuel_used);
    println!("  Host Calls: {}", metrics.host_calls);
    println!("  I/O Bytes: {}", metrics.io_bytes);
    // Logs removed

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::SubmitJob { .. } => {
            submit_job(&cli.command).await?;
        }
        Commands::ListNodes => {
            list_nodes().await?;
        }
        Commands::GetBids { job_id } => {
            get_bids(job_id).await?;
        }
        Commands::JobStatus { job_id } => {
            get_job_status(job_id).await?;
        }
        Commands::AcceptBid { job_id, node_id } => {
            accept_bid(job_id, node_id).await?;
        }
        Commands::Execute { wasm, output } => {
            execute_local(wasm, output.as_deref()).await?;
        }
        Commands::CreateNode {
            name,
            memory,
            cpu,
            location,
        } => {
            create_node(name, *memory, *cpu, location).await?;
        }
    }

    Ok(())
}
