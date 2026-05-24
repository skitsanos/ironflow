use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

/// Execute the `serve` subcommand.
pub(crate) async fn cmd_serve(
    host: String,
    port: u16,
    flows_dir: Option<PathBuf>,
    max_body: usize,
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    cfg: &crate::cli::IronFlowConfig,
) -> Result<()> {
    let host = if host == "0.0.0.0" {
        cfg.host.clone().unwrap_or(host)
    } else {
        host
    };
    let port = if port == 3000 {
        cfg.port.unwrap_or(port)
    } else {
        port
    };
    let flows_dir = flows_dir.or_else(|| cfg.flows_dir.as_deref().map(PathBuf::from));
    let max_body = if max_body == 1048576 {
        cfg.max_body.unwrap_or(max_body)
    } else {
        max_body
    };
    let api_key = resolve_api_key(cfg.api_key.clone());
    let allow_unauthenticated_api =
        resolve_allow_unauthenticated_api(cfg.allow_unauthenticated_api.unwrap_or(false));
    let cors_origins = resolve_cors_origins(cfg.cors_origins.clone());
    let webhooks = cfg.webhooks.clone().unwrap_or_default();
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

/// If the CLI value matches the hard-coded default, use the config value instead (if set).
pub(crate) fn apply_config_path(
    cli_value: PathBuf,
    default: &str,
    config_value: Option<&str>,
) -> PathBuf {
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
