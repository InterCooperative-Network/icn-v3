use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use tokio::signal;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, EnvFilter};

// Import necessary items from the library crate
use icn_runtime::{config::RuntimeConfig, Runtime};

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
    // TODO: Replace `Runtime::from_config` with the actual implementation
    //       that uses the loaded `config`.
    match Runtime::from_config(config.clone()).await {
        Ok(runtime) => {
            info!("Runtime initialized successfully.");

            // --- Start the node's main loop/service ---
            // This will likely involve starting background tasks, listening for jobs, etc.
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
        }
        Err(e) => {
            error!("Failed to initialize runtime: {:?}", e);
            return Err(e);
        }
    }

    Ok(())
}
