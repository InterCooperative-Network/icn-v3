use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, EnvFilter};

// Import necessary items from the library crate
use icn_runtime::{
    config::RuntimeConfig,
    context::RuntimeContextBuilder,
    load_or_generate_keypair,
    reputation_integration::HttpReputationUpdater,
    sled_storage::SledStorage,
    Runtime,
};
use icn_economics::mana::{InMemoryManaLedger, ManaRegenerator, RegenerationPolicy};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the node configuration file.
    #[clap(short, long, value_parser, default_value = "config/node.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let args = Args::parse();

    // Load configuration from file
    info!("Loading configuration from: {:?}", args.config);
    let config_contents = fs::read_to_string(&args.config)
        .with_context(|| format!("Failed to read configuration file: {:?}", args.config))?;
    let config: RuntimeConfig = toml::from_str(&config_contents)
        .with_context(|| format!("Failed to parse configuration file: {:?}", args.config))?;

    // Initialize tracing subscriber based on config or default
    let log_level_str = config.log_level.as_deref().unwrap_or("info");
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(log_level_str))
        .unwrap_or_else(|_| EnvFilter::new(Level::INFO.to_string()));

    fmt::Subscriber::builder().with_env_filter(filter).init();

    info!("Starting ICN Runtime Node...");
    info!("Using Node DID: {}", config.node_did);
    info!("Storage Path: {:?}", config.storage_path);

    // --- Runtime Initialization ---
    let storage = Arc::new(
        SledStorage::open(&config.storage_path).context("Failed to initialize SledStorage")?,
    );

    let keypair = load_or_generate_keypair(config.key_path.as_deref())
        .context("Failed to load or generate keypair")?;

    let mana_ledger = Arc::new(InMemoryManaLedger::default());
    
    // Use policy from config or default
    let regeneration_policy = config.mana_regeneration_policy
        .clone()
        .unwrap_or(RegenerationPolicy::FixedRatePerTick(10)); // Default policy

    // ManaRegenerator::new takes 2 arguments: ledger and policy.
    let mana_regenerator = Arc::new(ManaRegenerator::new(
        mana_ledger.clone(),
        regeneration_policy,
    ));

    let runtime_context = Arc::new(
        RuntimeContextBuilder::<InMemoryManaLedger>::new()
            .with_identity(keypair.clone()) 
            .with_executor_id(config.node_did.clone()) 
            .with_mana_regenerator(mana_regenerator) 
            // .with_trust_validator(...) // TODO: Initialize TrustValidator if needed from config
            // .with_dag_store(...) // TODO: Initialize DagStore if needed
            .build(),
    );

    let mut runtime = Runtime::<InMemoryManaLedger>::with_context(storage, runtime_context);

    if let Some(reputation_url) = &config.reputation_service_url {
        if !reputation_url.is_empty() {
            info!("Using HTTP reputation updater: {}", reputation_url);
            // HttpReputationUpdater::new returns Self, not Result.
            let updater = HttpReputationUpdater::new(
                reputation_url.to_string(), 
                keypair.did.clone() // CORRECTED: Pass Did
            );
            runtime = runtime.with_reputation_updater(Arc::new(updater));
        }
    }

    info!("Runtime initialized successfully.");

    // --- Start the node's main loop/service ---
    tokio::select! {
        res = runtime.run_forever() => {
            if let Err(e) = res {
                error!("Runtime exited with error: {:?}", e);
            }
        }
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal (Ctrl+C).");
        }
    }

    info!("Shutting down ICN Runtime Node...");

    Ok(())
}
