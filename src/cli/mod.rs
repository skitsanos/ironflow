mod bootstrap;
mod commands;
mod config;
mod replica_config;
mod resolution;
mod store_factory;

pub use config::IronFlowConfig;
pub use store_factory::{create_event_store, create_store};

use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

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
        #[arg(long, default_value = "data/runs", env = "IRONFLOW_STORE_DIR")]
        store_dir: PathBuf,
    },

    /// Validate a flow file without executing
    Validate {
        /// Path to the .lua flow file
        flow: PathBuf,

        /// Treat Lua handler warnings as validation failures
        #[arg(long)]
        strict: bool,
    },

    /// List past workflow runs
    List {
        /// Filter by status (pending, running, success, failed, stalled, cancelled)
        #[arg(short, long)]
        status: Option<String>,

        /// State store directory
        #[arg(long, default_value = "data/runs", env = "IRONFLOW_STORE_DIR")]
        store_dir: PathBuf,

        /// Output format (table, json)
        #[arg(long, default_value = "table")]
        format: String,

        /// Maximum records to return (capped by IRONFLOW_MAX_LIST_RECORDS)
        #[arg(long)]
        limit: Option<usize>,

        /// Opaque cursor returned by a previous list page
        #[arg(long)]
        after: Option<String>,
    },

    /// Inspect a specific run
    Inspect {
        /// Run ID
        run_id: String,

        /// State store directory
        #[arg(long, default_value = "data/runs", env = "IRONFLOW_STORE_DIR")]
        store_dir: PathBuf,
    },

    /// List available nodes
    Nodes,

    /// Inspect or prune content-addressed workflow artifacts
    Artifacts {
        #[command(subcommand)]
        command: ArtifactCommands,
    },

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

#[derive(Subcommand)]
pub enum ArtifactCommands {
    /// Delete old artifacts not referenced by any retained run
    Prune {
        /// Only inspect artifacts last modified before this RFC 3339 timestamp
        #[arg(long)]
        before: String,

        /// Maximum candidate artifacts to inspect (1-100)
        #[arg(long, default_value = "100")]
        limit: usize,

        /// Assert that every IronFlow writer sharing the stores is stopped
        #[arg(long)]
        confirm_offline: bool,

        /// State store directory
        #[arg(long, default_value = "data/runs", env = "IRONFLOW_STORE_DIR")]
        store_dir: PathBuf,
    },
}

/// Load the selected dotenv file before tracing or the async runtime starts.
///
/// Argument parsing is intentionally repeated by [`run_cli`]: this first pass
/// discovers `--dotenv`, while the second pass lets Clap observe the newly
/// loaded environment and retain each value's source.
///
/// # Safety
///
/// No other threads may be running or concurrently accessing the process
/// environment. Call this once at the start of the process, before creating an
/// async runtime or initializing libraries that may start worker threads.
pub unsafe fn bootstrap_environment() -> Result<Option<PathBuf>> {
    let cli = Cli::parse();
    // SAFETY: the caller upholds this function's process-wide environment
    // exclusivity contract.
    unsafe { bootstrap::load_dotenv(cli.dotenv.as_deref()) }
}

pub async fn run_cli() -> Result<()> {
    let matches = Cli::command().get_matches();
    let sources = resolution::CommandValueSources::from_matches(&matches);
    let cli = Cli::from_arg_matches(&matches)?;

    // Load config file (ironflow.yaml)
    let cfg = IronFlowConfig::load(cli.config.as_deref())?;

    match cli.command {
        Commands::Run {
            flow,
            context,
            verbose,
            store_dir,
        } => {
            resolution::validate_run_deadline_environment()?;
            let store_dir = resolution::with_config(
                store_dir,
                sources.store_dir,
                cfg.store_dir.as_deref().map(PathBuf::from),
            );
            let max_concurrent_tasks = resolution::resolve_max_concurrent_tasks(&cfg)?;
            let store = create_store(&cfg, &store_dir).await?;
            commands::cmd_run(flow, context, verbose, store, max_concurrent_tasks).await
        }
        Commands::Validate { flow, strict } => commands::cmd_validate(flow, strict),
        Commands::List {
            status,
            store_dir,
            format,
            limit,
            after,
        } => {
            let listing_policy = crate::util::listing::ListingPolicy::from_env()?;
            let prepared = commands::prepare_list(status, format, limit, after, listing_policy)?;
            let store_dir = resolution::with_config(
                store_dir,
                sources.store_dir,
                cfg.store_dir.as_deref().map(PathBuf::from),
            );
            let store = create_store(&cfg, &store_dir).await?;
            commands::cmd_list(store, prepared).await
        }
        Commands::Inspect { run_id, store_dir } => {
            let store_dir = resolution::with_config(
                store_dir,
                sources.store_dir,
                cfg.store_dir.as_deref().map(PathBuf::from),
            );
            let store = create_store(&cfg, &store_dir).await?;
            commands::cmd_inspect(run_id, store).await
        }
        Commands::Nodes => commands::cmd_nodes(),
        Commands::Artifacts { command } => match command {
            ArtifactCommands::Prune {
                before,
                limit,
                confirm_offline,
                store_dir,
            } => {
                let store_dir = resolution::with_config(
                    store_dir,
                    sources.store_dir,
                    cfg.store_dir.as_deref().map(PathBuf::from),
                );
                let store = create_store(&cfg, &store_dir).await?;
                commands::cmd_artifact_prune(store, before, limit, confirm_offline).await
            }
        },
        Commands::Serve {
            host,
            port,
            store_dir,
            flows_dir,
            max_body,
        } => {
            let listing_policy = crate::util::listing::ListingPolicy::from_env()?;
            let server_config = resolution::ServerConfig::resolve(&cfg)?;
            replica_config::validate(&cfg, server_config.replica_mode)?;
            let host = resolution::with_config(host, sources.host, cfg.host.clone());
            let port = resolution::with_config(port, sources.port, cfg.port);
            let store_dir = resolution::with_config(
                store_dir,
                sources.store_dir,
                cfg.store_dir.as_deref().map(PathBuf::from),
            );
            let flows_dir = resolution::optional_with_config(
                flows_dir,
                sources.flows_dir,
                cfg.flows_dir.as_deref().map(PathBuf::from),
            );
            let max_body = resolution::with_config(max_body, sources.max_body, cfg.max_body);
            let options = crate::api::ServeOptions {
                host,
                port,
                flows_dir,
                max_body,
                max_concurrent_tasks: server_config.max_concurrent_tasks,
                listing_policy,
                webhooks: cfg.webhooks.clone().unwrap_or_default(),
                allow_adhoc_flows: server_config.allow_adhoc_flows,
                cors_origins: server_config.cors_origins,
                api_key: server_config.api_key,
                allow_unauthenticated_api: server_config.allow_unauthenticated_api,
                metrics_enabled: server_config.metrics_enabled,
            };
            let store = create_store(&cfg, &store_dir).await?;
            let event_store = create_event_store(&cfg, &store_dir).await?;
            commands::cmd_serve(
                store,
                event_store,
                options,
                cfg.schedules.clone().unwrap_or_default(),
            )
            .await
        }
    }
}
