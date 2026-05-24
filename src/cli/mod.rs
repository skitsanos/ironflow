mod commands;
mod config;
mod store_factory;

pub use config::IronFlowConfig;
pub use store_factory::{create_event_store, create_store};

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;

#[derive(Parser)]
#[command(name = "ironflow", version, about = "Lightweight workflow engine")]
pub struct Cli {
    /// Path to a .env file to load (default: auto-detect .env in cwd)
    #[arg(long, global = true)]
    dotenv: Option<PathBuf>,

    /// Path to config file (default: auto-detect ironflow.yaml in cwd)
    #[arg(short = 'C', long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Execute a workflow from a Lua flow file
    Run {
        /// Path to the .lua flow file
        flow: PathBuf,

        /// Initial context as JSON string
        #[arg(short, long)]
        context: Option<String>,

        /// Enable verbose output
        #[arg(short, long)]
        verbose: bool,

        /// State store directory
        #[arg(long, default_value = "data/runs")]
        store_dir: PathBuf,
    },

    /// Validate a flow file without executing
    Validate {
        /// Path to the .lua flow file
        flow: PathBuf,
    },

    /// List past workflow runs
    List {
        /// Filter by status (pending, running, success, failed, stalled)
        #[arg(short, long)]
        status: Option<String>,

        /// State store directory
        #[arg(long, default_value = "data/runs")]
        store_dir: PathBuf,

        /// Output format (table, json)
        #[arg(long, default_value = "table")]
        format: String,
    },

    /// Inspect a specific run
    Inspect {
        /// Run ID
        run_id: String,

        /// State store directory
        #[arg(long, default_value = "data/runs")]
        store_dir: PathBuf,
    },

    /// List available nodes
    Nodes,

    /// Start the REST API server
    Serve {
        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0", env = "HOST")]
        host: String,

        /// Port to listen on
        #[arg(short, long, default_value = "3000", env = "PORT")]
        port: u16,

        /// State store directory
        #[arg(long, default_value = "data/runs", env = "IRONFLOW_STORE_DIR")]
        store_dir: PathBuf,

        /// Directory to look for .lua flow files
        #[arg(long, env = "FLOWS_DIR")]
        flows_dir: Option<PathBuf>,

        /// Maximum request body size in bytes (default: 1048576 = 1 MB)
        #[arg(long, default_value = "1048576", env = "MAX_BODY")]
        max_body: usize,
    },
}

pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();

    // Load .env file
    load_dotenv(cli.dotenv.as_deref());

    // Load config file (ironflow.yaml)
    let cfg = IronFlowConfig::load(cli.config.as_deref())?;

    match cli.command {
        Commands::Run {
            flow,
            context,
            verbose,
            store_dir,
        } => {
            let store_dir =
                commands::apply_config_path(store_dir, "data/runs", cfg.store_dir.as_deref());
            let store = create_store(&cfg, &store_dir).await?;
            commands::cmd_run(flow, context, verbose, store, cfg.max_concurrent_tasks).await
        }
        Commands::Validate { flow } => commands::cmd_validate(flow),
        Commands::List {
            status,
            store_dir,
            format,
        } => {
            let store_dir =
                commands::apply_config_path(store_dir, "data/runs", cfg.store_dir.as_deref());
            let store = create_store(&cfg, &store_dir).await?;
            commands::cmd_list(status, store, format).await
        }
        Commands::Inspect { run_id, store_dir } => {
            let store_dir =
                commands::apply_config_path(store_dir, "data/runs", cfg.store_dir.as_deref());
            let store = create_store(&cfg, &store_dir).await?;
            commands::cmd_inspect(run_id, store).await
        }
        Commands::Nodes => commands::cmd_nodes(),
        Commands::Serve {
            host,
            port,
            store_dir,
            flows_dir,
            max_body,
        } => {
            let store_dir =
                commands::apply_config_path(store_dir, "data/runs", cfg.store_dir.as_deref());
            let store = create_store(&cfg, &store_dir).await?;
            let event_store = create_event_store(&cfg, &store_dir).await?;
            commands::cmd_serve(host, port, flows_dir, max_body, store, event_store, &cfg).await
        }
    }
}

/// Load environment variables from a .env file.
/// If an explicit path is given, load from that path (error if missing).
/// Otherwise, auto-detect .env in the current working directory (silently skip if absent).
fn load_dotenv(explicit_path: Option<&std::path::Path>) {
    match explicit_path {
        Some(path) => match dotenvy::from_path(path) {
            Ok(()) => info!("Loaded env from {}", path.display()),
            Err(e) => {
                eprintln!(
                    "Warning: Failed to load dotenv file '{}': {}",
                    path.display(),
                    e
                );
            }
        },
        None => {
            // Auto-detect .env in current directory
            match dotenvy::dotenv() {
                Ok(path) => info!("Loaded env from {}", path.display()),
                Err(dotenvy::Error::Io(_)) => {
                    // No .env file found — that's fine, silently skip
                }
                Err(e) => {
                    eprintln!("Warning: Failed to parse .env file: {}", e);
                }
            }
        }
    }
}
