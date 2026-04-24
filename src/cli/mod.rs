mod config;
pub use config::IronFlowConfig;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use tracing::info;

use crate::engine::WorkflowEngine;
use crate::engine::types::Context;
use crate::lua::LuaRuntime;
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
#[cfg(feature = "redis")]
use crate::storage::event_store::RedisEventStore;
use crate::storage::event_store::{EventStore, MemoryEventStore, SqlEventStore};
use crate::storage::json_store::JsonStateStore;
#[cfg(feature = "redis")]
use crate::storage::redis_store::RedisStateStore;
use crate::storage::sql_store::SqlStateStore;

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
            let store_dir = apply_config_path(store_dir, "data/runs", cfg.store_dir.as_deref());
            let store = create_store(&cfg, &store_dir).await?;
            cmd_run(flow, context, verbose, store, cfg.max_concurrent_tasks).await
        }
        Commands::Validate { flow } => cmd_validate(flow),
        Commands::List {
            status,
            store_dir,
            format,
        } => {
            let store_dir = apply_config_path(store_dir, "data/runs", cfg.store_dir.as_deref());
            let store = create_store(&cfg, &store_dir).await?;
            cmd_list(status, store, format).await
        }
        Commands::Inspect { run_id, store_dir } => {
            let store_dir = apply_config_path(store_dir, "data/runs", cfg.store_dir.as_deref());
            let store = create_store(&cfg, &store_dir).await?;
            cmd_inspect(run_id, store).await
        }
        Commands::Nodes => cmd_nodes(),
        Commands::Serve {
            host,
            port,
            store_dir,
            flows_dir,
            max_body,
        } => {
            let store_dir = apply_config_path(store_dir, "data/runs", cfg.store_dir.as_deref());
            let store = create_store(&cfg, &store_dir).await?;
            let event_store = create_event_store(&cfg, &store_dir).await?;
            let host = if host == "0.0.0.0" {
                cfg.host.unwrap_or(host)
            } else {
                host
            };
            let port = if port == 3000 {
                cfg.port.unwrap_or(port)
            } else {
                port
            };
            let flows_dir = flows_dir.or_else(|| cfg.flows_dir.map(PathBuf::from));
            let max_body = if max_body == 1048576 {
                cfg.max_body.unwrap_or(max_body)
            } else {
                max_body
            };
            let api_key = resolve_api_key(cfg.api_key);
            let allow_unauthenticated_api =
                resolve_allow_unauthenticated_api(cfg.allow_unauthenticated_api.unwrap_or(false));
            let cors_origins = resolve_cors_origins(cfg.cors_origins);
            let webhooks = cfg.webhooks.unwrap_or_default();
            crate::api::serve(
                store,
                event_store,
                crate::api::ServeOptions {
                    host,
                    port,
                    flows_dir,
                    max_body,
                    max_concurrent_tasks: cfg.max_concurrent_tasks,
                    webhooks,
                    cors_origins,
                    api_key,
                    allow_unauthenticated_api,
                },
            )
            .await
        }
    }
}

/// If the CLI value matches the hard-coded default, use the config value instead (if set).
fn apply_config_path(cli_value: PathBuf, default: &str, config_value: Option<&str>) -> PathBuf {
    if cli_value == Path::new(default) {
        config_value.map(PathBuf::from).unwrap_or(cli_value)
    } else {
        cli_value
    }
}

fn resolve_cors_origins(config_value: Option<Vec<String>>) -> Option<Vec<String>> {
    std::env::var("IRONFLOW_CORS_ORIGINS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(str::to_string)
                .collect()
        })
        .or(config_value)
}

fn resolve_api_key(config_value: Option<String>) -> Option<String> {
    std::env::var("IRONFLOW_API_KEY").ok().or(config_value)
}

fn resolve_allow_unauthenticated_api(config_value: bool) -> bool {
    std::env::var("IRONFLOW_ALLOW_UNAUTHENTICATED_API")
        .ok()
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(config_value)
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

/// Create a state store based on configuration.
///
/// Selects a state store backend.
///
/// Config fields can be overridden by environment variables:
/// `IRONFLOW_STORE`, `IRONFLOW_STORE_URL`, `REDIS_URL`, `REDIS_PREFIX`, `REDIS_TTL`.
pub async fn create_store(cfg: &IronFlowConfig, store_dir: &Path) -> Result<Arc<dyn StateStore>> {
    let backend = std::env::var("IRONFLOW_STORE")
        .ok()
        .or_else(|| cfg.store_backend.clone())
        .unwrap_or_else(|| "json".to_string());

    match backend.as_str() {
        "json" => {
            info!("Using JSON state store at {}", store_dir.display());
            Ok(Arc::new(JsonStateStore::new(store_dir)))
        }
        "sqlite" => {
            let url = resolve_sql_store_url(cfg, store_dir, "sqlite")?;
            let table_prefix = resolve_sql_table_prefix(cfg);
            info!("Using SQLite state store at {}", url);
            Ok(Arc::new(
                SqlStateStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        "postgres" => {
            let url = resolve_sql_store_url(cfg, store_dir, "postgres")?;
            let table_prefix = resolve_sql_table_prefix(cfg);
            info!("Using Postgres state store");
            Ok(Arc::new(
                SqlStateStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        #[cfg(feature = "redis")]
        "redis" => {
            let url = std::env::var("REDIS_URL")
                .ok()
                .or_else(|| cfg.redis_url.clone())
                .unwrap_or_else(|| "redis://127.0.0.1:6379".to_string());

            let prefix = std::env::var("REDIS_PREFIX")
                .ok()
                .or_else(|| cfg.redis_prefix.clone());

            let ttl = std::env::var("REDIS_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(cfg.redis_ttl);

            info!("Using Redis state store at {}", url);
            let store = RedisStateStore::new(&url, prefix, ttl).await?;
            Ok(Arc::new(store))
        }
        #[cfg(not(feature = "redis"))]
        "redis" => {
            anyhow::bail!(
                "Redis backend requested but the 'redis' feature is not enabled. \
                 Rebuild with: cargo build --features redis"
            );
        }
        other => {
            anyhow::bail!(
                "Unknown state store backend '{}'. Use one of: json, sqlite, postgres, redis",
                other
            );
        }
    }
}

/// Create an event store based on configuration.
///
/// Event backend selection is deliberately separate from run state storage:
/// `IRONFLOW_EVENT_STORE`, `IRONFLOW_EVENT_STORE_URL`.
pub async fn create_event_store(
    cfg: &IronFlowConfig,
    store_dir: &Path,
) -> Result<Arc<dyn EventStore>> {
    let backend = std::env::var("IRONFLOW_EVENT_STORE")
        .ok()
        .or_else(|| cfg.event_store.clone())
        .unwrap_or_else(|| "memory".to_string());

    match backend.as_str() {
        "memory" => {
            info!("Using in-memory event store");
            Ok(Arc::new(MemoryEventStore::new()))
        }
        "sqlite" => {
            let url = resolve_sql_event_store_url(cfg, store_dir, "sqlite")?;
            let table_prefix = resolve_sql_table_prefix(cfg);
            info!("Using SQLite event store at {}", url);
            Ok(Arc::new(
                SqlEventStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        "postgres" => {
            let url = resolve_sql_event_store_url(cfg, store_dir, "postgres")?;
            let table_prefix = resolve_sql_table_prefix(cfg);
            info!("Using Postgres event store");
            Ok(Arc::new(
                SqlEventStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        #[cfg(feature = "redis")]
        "redis" => {
            let url = std::env::var("REDIS_URL")
                .ok()
                .or_else(|| cfg.redis_url.clone())
                .unwrap_or_else(|| "redis://127.0.0.1:6379".to_string());

            let prefix = std::env::var("REDIS_PREFIX")
                .ok()
                .or_else(|| cfg.redis_prefix.clone());

            let ttl = std::env::var("REDIS_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(cfg.redis_ttl);

            info!("Using Redis event store at {}", url);
            Ok(Arc::new(RedisEventStore::new(&url, prefix, ttl).await?))
        }
        #[cfg(not(feature = "redis"))]
        "redis" => {
            anyhow::bail!(
                "Redis event backend requested but the 'redis' feature is not enabled. \
                 Rebuild with: cargo build --features redis"
            );
        }
        other => {
            anyhow::bail!(
                "Unknown event store backend '{}'. Use one of: memory, sqlite, postgres, redis",
                other
            );
        }
    }
}

fn resolve_sql_table_prefix(cfg: &IronFlowConfig) -> Option<String> {
    std::env::var("IRONFLOW_SQL_TABLE_PREFIX")
        .ok()
        .or_else(|| cfg.sql_table_prefix.clone())
}

fn resolve_sql_store_url(cfg: &IronFlowConfig, store_dir: &Path, backend: &str) -> Result<String> {
    if let Some(url) = std::env::var("IRONFLOW_STORE_URL")
        .ok()
        .or_else(|| cfg.store_url.clone())
    {
        return Ok(url);
    }

    match backend {
        "sqlite" => {
            std::fs::create_dir_all(store_dir)
                .with_context(|| format!("Failed to create store dir: {}", store_dir.display()))?;
            let path = store_dir.join("ironflow.sqlite");
            Ok(format!("sqlite://{}?mode=rwc", path.to_string_lossy()))
        }
        "postgres" => {
            anyhow::bail!("Postgres state store requires IRONFLOW_STORE_URL or store_url in config")
        }
        _ => anyhow::bail!("Unsupported SQL state store backend '{}'", backend),
    }
}

fn resolve_sql_event_store_url(
    cfg: &IronFlowConfig,
    store_dir: &Path,
    backend: &str,
) -> Result<String> {
    if let Some(url) = std::env::var("IRONFLOW_EVENT_STORE_URL")
        .ok()
        .or_else(|| cfg.event_store_url.clone())
    {
        return Ok(url);
    }

    match backend {
        "sqlite" => {
            std::fs::create_dir_all(store_dir)
                .with_context(|| format!("Failed to create store dir: {}", store_dir.display()))?;
            let path = store_dir.join("ironflow-events.sqlite");
            Ok(format!("sqlite://{}?mode=rwc", path.to_string_lossy()))
        }
        "postgres" => {
            anyhow::bail!(
                "Postgres event store requires IRONFLOW_EVENT_STORE_URL or event_store_url in config"
            )
        }
        _ => anyhow::bail!("Unsupported SQL event store backend '{}'", backend),
    }
}

async fn cmd_run(
    flow_path: PathBuf,
    context_json: Option<String>,
    verbose: bool,
    store: Arc<dyn StateStore>,
    max_concurrent_tasks: Option<usize>,
) -> Result<()> {
    let registry = NodeRegistry::with_builtins();

    let flow_str = flow_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid flow path"))?;

    let flow = LuaRuntime::load_flow(flow_str, &registry)
        .with_context(|| format!("Failed to load flow: {}", flow_path.display()))?;

    println!("Flow: {} ({} steps)", flow.name, flow.steps.len());

    if verbose {
        println!("\nSteps:");
        for step in &flow.steps {
            let deps = if step.dependencies.is_empty() {
                String::from("none")
            } else {
                step.dependencies.join(", ")
            };
            println!("  {} [{}] deps: {}", step.name, step.node_type, deps);
            if step.retry.max_retries > 0 {
                println!(
                    "    retries: {}, backoff: {}s",
                    step.retry.max_retries, step.retry.backoff_s
                );
            }
            if let Some(t) = step.timeout_s {
                println!("    timeout: {}s", t);
            }
            if let Some(ref r) = step.route {
                println!("    route: {}", r);
            }
        }
    }

    // Parse initial context
    let mut initial_ctx: Context = match context_json {
        Some(json) => {
            serde_json::from_str(&json).with_context(|| "Failed to parse --context JSON")?
        }
        None => Context::new(),
    };

    // Inject _flow_dir so subworkflow nodes can resolve relative paths
    if let Some(flow_dir) = flow_path.canonicalize()?.parent() {
        initial_ctx.insert(
            "_flow_dir".to_string(),
            serde_json::Value::String(flow_dir.to_string_lossy().to_string()),
        );
    }

    let engine = WorkflowEngine::new(Arc::new(registry), store.clone(), max_concurrent_tasks);

    let run_id = engine.execute(&flow, initial_ctx).await?;

    // Print results
    let run_info = store.get_run_info(&run_id).await?;
    println!("\nRun ID: {}", run_id);
    println!("Status: {}", run_info.status);

    println!("\nTasks:");
    for (name, task) in &run_info.tasks {
        let status_icon = match task.status {
            crate::engine::types::TaskStatus::Success => "✓",
            crate::engine::types::TaskStatus::Failed => "✗",
            crate::engine::types::TaskStatus::Skipped => "⊘",
            crate::engine::types::TaskStatus::Running => "⟳",
            crate::engine::types::TaskStatus::Pending => "○",
        };
        println!(
            "  {} {} [{}] (attempt {})",
            status_icon, name, task.node_type, task.attempt
        );
        if verbose && let (Some(s), Some(f)) = (&task.started, &task.finished) {
            let duration = *f - *s;
            println!("    Duration: {}ms", duration.num_milliseconds());
        }
        if let Some(ref err) = task.error {
            println!("    Error: {}", err);
        }
        if verbose && let Some(ref output) = task.output {
            println!("    Output: {}", output);
        }
    }

    if !run_info.ctx.is_empty() {
        // Only print non-internal context keys
        let user_ctx: Context = run_info
            .ctx
            .iter()
            .filter(|(k, _)| !k.starts_with('_'))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if !user_ctx.is_empty() {
            println!("\nContext:");
            println!("{}", serde_json::to_string_pretty(&user_ctx)?);
        }
    }

    Ok(())
}

fn cmd_validate(flow_path: PathBuf) -> Result<()> {
    let registry = NodeRegistry::with_builtins();

    let flow_str = flow_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid flow path"))?;

    let flow = LuaRuntime::load_flow(flow_str, &registry)
        .with_context(|| format!("Failed to load flow: {}", flow_path.display()))?;

    println!("Flow: {}", flow.name);
    println!("Steps: {}", flow.steps.len());

    // Validate all node types exist
    let mut errors = Vec::new();
    for step in &flow.steps {
        if registry.get(&step.node_type).is_none() {
            errors.push(format!(
                "Step '{}' uses unknown node type '{}'",
                step.name, step.node_type
            ));
        }
    }

    // Validate DAG (dependencies + cycle detection)
    errors.extend(flow.validate_dag());

    if errors.is_empty() {
        println!("Validation: OK");

        println!("\nExecution order:");
        for step in &flow.steps {
            let deps = if step.dependencies.is_empty() {
                String::from("(no dependencies)")
            } else {
                format!("depends on: {}", step.dependencies.join(", "))
            };
            println!("  {} [{}] {}", step.name, step.node_type, deps);
        }
    } else {
        println!("Validation: FAILED");
        for err in &errors {
            println!("  - {}", err);
        }
        anyhow::bail!("{} validation error(s) found", errors.len());
    }

    Ok(())
}

async fn cmd_list(
    status_filter: Option<String>,
    store: Arc<dyn StateStore>,
    format: String,
) -> Result<()> {
    let status = status_filter
        .as_deref()
        .map(|s| match s {
            "pending" => Ok(crate::engine::types::RunStatus::Pending),
            "running" => Ok(crate::engine::types::RunStatus::Running),
            "success" => Ok(crate::engine::types::RunStatus::Success),
            "failed" => Ok(crate::engine::types::RunStatus::Failed),
            "stalled" => Ok(crate::engine::types::RunStatus::Stalled),
            _ => Err(anyhow::anyhow!("Invalid status filter: {}", s)),
        })
        .transpose()?;

    let runs = store.list_runs(status).await?;

    if runs.is_empty() {
        println!("No runs found.");
        return Ok(());
    }

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&runs)?);
        return Ok(());
    }

    // Table format
    println!(
        "{:<38} {:<20} {:<10} {:<24}",
        "RUN ID", "FLOW", "STATUS", "STARTED"
    );
    println!("{}", "-".repeat(92));

    for run in &runs {
        let started = run
            .started
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:<38} {:<20} {:<10} {:<24}",
            run.id, run.flow_name, run.status, started
        );
    }

    println!("\nTotal: {} run(s)", runs.len());
    Ok(())
}

async fn cmd_inspect(run_id: String, store: Arc<dyn StateStore>) -> Result<()> {
    let info = store
        .get_run_info(&run_id)
        .await
        .with_context(|| format!("Run '{}' not found", run_id))?;

    println!("{}", serde_json::to_string_pretty(&info)?);

    Ok(())
}

fn cmd_nodes() -> Result<()> {
    let registry = NodeRegistry::with_builtins();
    let nodes = registry.list();

    println!("{:<20} DESCRIPTION", "NODE TYPE");
    println!("{}", "-".repeat(60));

    for (name, desc) in &nodes {
        println!("{:<20} {}", name, desc);
    }

    println!("\nTotal: {} node(s)", nodes.len());
    Ok(())
}
