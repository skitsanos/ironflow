mod handlers;
mod errors;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::nodes::NodeRegistry;
use crate::storage::json_store::JsonStateStore;

/// Shared application state accessible by all handlers.
pub struct AppState {
    pub registry: Arc<NodeRegistry>,
    pub store: Arc<JsonStateStore>,
    pub flows_dir: Option<PathBuf>,
}

/// Start the REST API server.
pub async fn serve(host: &str, port: u16, store_dir: PathBuf, flows_dir: Option<PathBuf>, max_body: usize) -> Result<()> {
    let registry = Arc::new(NodeRegistry::with_builtins());
    let store = Arc::new(JsonStateStore::new(store_dir));

    let state = Arc::new(AppState {
        registry,
        store,
        flows_dir,
    });

    let app = Router::new()
        .route("/flows/run", post(handlers::run_flow))
        .route("/flows/validate", post(handlers::validate_flow))
        .route("/runs", get(handlers::list_runs))
        .route("/runs/{id}", get(handlers::get_run))
        .route("/runs/{id}", delete(handlers::delete_run))
        .route("/nodes", get(handlers::list_nodes))
        .route("/health", get(handlers::health))
        .layer(DefaultBodyLimit::max(max_body))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    info!("IronFlow API server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
