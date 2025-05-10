use anyhow::Result;
use icn_types::{
    identity::{CredentialProof, CredentialSubject, TrustBundle, VerifiableCredential},
    trust::QuorumConfig,
};
use std::{
    collections::HashMap,
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use tokio::time::sleep;

const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(30);
const NODE_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn test_federation_bootstrap() -> Result<()> {
    // 1. Clean up any existing state
    cleanup_devnet()?;

    // 2. Generate federation and node keys
    let keys_dir = Path::new("devnet/examples/federation_keys");
    fs::create_dir_all(keys_dir)?;

    // Generate federation keys
    let federation_keys = generate_federation_keys(keys_dir)?;
    assert!(federation_keys.exists(), "Federation keys not generated");

    // Generate node keys
    for node_id in ["node-1", "node-2", "node-3"] {
        let node_keys = generate_node_keys(keys_dir, node_id)?;
        assert!(node_keys.exists(), "Node keys not generated for {}", node_id);
    }

    // 3. Start the federation nodes
    let compose_file = Path::new("devnet/docker-compose.yml");
    start_federation_nodes(compose_file)?;

    // 4. Wait for nodes to be ready
    wait_for_nodes_ready()?;

    // 5. Bootstrap the federation
    bootstrap_federation()?;

    // 6. Verify federation state
    verify_federation_state()?;

    // 7. Test federation join workflow
    test_federation_join()?;

    // 8. Clean up
    cleanup_devnet()?;

    Ok(())
}

fn cleanup_devnet() -> Result<()> {
    let compose_file = Path::new("devnet/docker-compose.yml");
    Command::new("docker")
        .args(["compose", "-f", compose_file.to_str().unwrap(), "down", "-v"])
        .status()?;
    Ok(())
}

fn generate_federation_keys(keys_dir: &Path) -> Result<std::path::PathBuf> {
    let output = keys_dir.join("federation.json");
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "federation",
            "keygen",
            "--output",
            output.to_str().unwrap(),
        ])
        .status()?;
    Ok(output)
}

fn generate_node_keys(keys_dir: &Path, node_id: &str) -> Result<std::path::PathBuf> {
    let output = keys_dir.join(format!("{}.json", node_id));
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "node",
            "keygen",
            "--node-id",
            node_id,
            "--output",
            output.to_str().unwrap(),
        ])
        .status()?;
    Ok(output)
}

fn start_federation_nodes(compose_file: &Path) -> Result<()> {
    Command::new("docker")
        .args([
            "compose",
            "-f",
            compose_file.to_str().unwrap(),
            "up",
            "-d",
            "--build",
        ])
        .status()?;
    Ok(())
}

fn wait_for_nodes_ready() -> Result<()> {
    let start = Instant::now();
    let mut all_ready = false;

    while !all_ready && start.elapsed() < NODE_STARTUP_TIMEOUT {
        all_ready = true;
        for port in [7001, 7002, 7003] {
            if !is_port_ready("localhost", port)? {
                all_ready = false;
                break;
            }
        }
        if !all_ready {
            sleep(Duration::from_secs(1)).await;
        }
    }

    assert!(all_ready, "Nodes failed to start within timeout");
    Ok(())
}

fn is_port_ready(host: &str, port: u16) -> Result<bool> {
    let output = Command::new("nc")
        .args(["-z", host, &port.to_string()])
        .output()?;
    Ok(output.status.success())
}

fn bootstrap_federation() -> Result<()> {
    let fed_toml = Path::new("devnet/federation.toml");
    let keys = Path::new("devnet/examples/federation_keys/federation.json");

    // Initialize federation
    Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "federation",
            "init",
            "--config",
            fed_toml.to_str().unwrap(),
            "--keys",
            keys.to_str().unwrap(),
            "--node-api",
            "http://localhost:7001",
        ])
        .status()?;

    // Register nodes
    for node_id in ["node-1", "node-2", "node-3"] {
        let node_keys = format!("devnet/examples/federation_keys/{}.json", node_id);
        Command::new("cargo")
            .args([
                "run",
                "-q",
                "-p",
                "icn-cli",
                "--",
                "node",
                "register",
                "--node-id",
                node_id,
                "--keys",
                &node_keys,
                "--node-api",
                "http://localhost:7001",
            ])
            .status()?;
    }

    Ok(())
}

fn verify_federation_state() -> Result<()> {
    // Verify federation status
    let output = Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "federation",
            "status",
            "--node-api",
            "http://localhost:7001",
        ])
        .output()?;

    assert!(output.status.success(), "Failed to get federation status");
    let status = String::from_utf8(output.stdout)?;
    assert!(status.contains("active"), "Federation not active");

    // Verify node registration
    for node_id in ["node-1", "node-2", "node-3"] {
        let output = Command::new("cargo")
            .args([
                "run",
                "-q",
                "-p",
                "icn-cli",
                "--",
                "node",
                "status",
                "--node-id",
                node_id,
                "--node-api",
                "http://localhost:7001",
            ])
            .output()?;

        assert!(output.status.success(), "Failed to get node status for {}", node_id);
        let status = String::from_utf8(output.stdout)?;
        assert!(status.contains("registered"), "Node {} not registered", node_id);
    }

    Ok(())
}

fn test_federation_join() -> Result<()> {
    // Create a test proposal
    let proposal = Path::new("devnet/examples/sample_proposal.ccl");
    assert!(proposal.exists(), "Sample proposal not found");

    // Submit proposal
    let output = Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "coop",
            "propose",
            "--file",
            proposal.to_str().unwrap(),
            "--api",
            "http://localhost:8080",
        ])
        .output()?;

    assert!(output.status.success(), "Failed to submit proposal");
    let response = String::from_utf8(output.stdout)?;
    assert!(response.contains("proposal_id"), "No proposal ID in response");

    // Wait for proposal to be processed
    sleep(Duration::from_secs(2)).await;

    // Verify proposal status
    let output = Command::new("cargo")
        .args([
            "run",
            "-q",
            "-p",
            "icn-cli",
            "--",
            "coop",
            "status",
            "--api",
            "http://localhost:8080",
        ])
        .output()?;

    assert!(output.status.success(), "Failed to get proposal status");
    let status = String::from_utf8(output.stdout)?;
    assert!(status.contains("open"), "Proposal not in open state");

    Ok(())
} 