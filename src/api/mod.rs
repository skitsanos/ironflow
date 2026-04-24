pub mod errors;
pub mod handlers;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::http::{HeaderMap, HeaderValue};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

/// Shared application state accessible by all handlers.
pub struct AppState {
    pub registry: Arc<NodeRegistry>,
    pub store: Arc<dyn StateStore>,
    pub event_store: Arc<dyn EventStore>,
    pub flows_dir: Option<PathBuf>,
    pub max_concurrent_tasks: Option<usize>,
    /// Webhook name → flow file path mappings from config.
    pub webhooks: HashMap<String, String>,
}

/// Configuration for the REST API server.
pub struct ServeOptions {
    pub host: String,
    pub port: u16,
    pub flows_dir: Option<PathBuf>,
    pub max_body: usize,
    pub max_concurrent_tasks: Option<usize>,
    pub webhooks: HashMap<String, String>,
    pub cors_origins: Option<Vec<String>>,
    pub api_key: Option<String>,
    pub allow_unauthenticated_api: bool,
}

#[derive(Clone)]
pub struct ApiAuth {
    api_key: String,
}

impl ApiAuth {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }
}

/// Start the REST API server.
pub async fn serve(
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    options: ServeOptions,
) -> Result<()> {
    let registry = Arc::new(NodeRegistry::with_builtins());

    let state = Arc::new(AppState {
        registry,
        store,
        event_store,
        flows_dir: options.flows_dir,
        max_concurrent_tasks: options.max_concurrent_tasks,
        webhooks: options.webhooks,
    });

    let auth = build_api_auth(
        options.api_key,
        options.allow_unauthenticated_api,
        &options.host,
    )?;

    let protected_routes = Router::new()
        .route("/flows/run", post(handlers::run_flow))
        .route("/flows/validate", post(handlers::validate_flow))
        .route("/runs", get(handlers::list_runs))
        .route("/runs/{id}", get(handlers::get_run))
        .route("/runs/{id}/events", get(handlers::run_events))
        .route("/runs/{id}", delete(handlers::delete_run))
        .route("/nodes", get(handlers::list_nodes))
        .route("/webhooks/{name}", post(handlers::run_webhook));

    let protected_routes = if let Some(auth) = auth {
        protected_routes.layer(middleware::from_fn_with_state(auth, require_api_key))
    } else {
        protected_routes
    };

    let app = Router::new()
        .route("/health", get(handlers::health))
        .merge(protected_routes)
        .layer(DefaultBodyLimit::max(options.max_body))
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer(options.cors_origins)?)
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", options.host, options.port).parse()?;
    info!("IronFlow API server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Build the CORS policy for the API server.
///
/// - `None` or an empty list: no browser origins are allowed.
/// - `["*"]`: explicitly allow any origin.
/// - otherwise: allow only the exact origin strings provided.
pub fn cors_layer(origins: Option<Vec<String>>) -> Result<CorsLayer> {
    let origins = origins
        .unwrap_or_default()
        .into_iter()
        .map(|origin| origin.trim().to_string())
        .filter(|origin| !origin.is_empty())
        .collect::<Vec<_>>();

    let base = CorsLayer::new()
        .allow_headers(Any)
        .allow_methods(Any)
        .expose_headers(Any);

    if origins.is_empty() {
        warn!("CORS origins are not configured; browser cross-origin requests will be denied");
        return Ok(base);
    }

    if origins.iter().any(|origin| origin == "*") {
        if origins.len() > 1 {
            anyhow::bail!("CORS wildcard '*' cannot be combined with explicit origins");
        }
        warn!("CORS is configured to allow any origin via '*'");
        return Ok(base.allow_origin(AllowOrigin::any()));
    }

    let mut values = Vec::with_capacity(origins.len());
    for origin in origins {
        let value = HeaderValue::from_str(&origin)
            .map_err(|e| anyhow::anyhow!("Invalid CORS origin '{}': {}", origin, e))?;
        values.push(value);
    }

    Ok(base.allow_origin(AllowOrigin::list(values)))
}

fn build_api_auth(
    api_key: Option<String>,
    allow_unauthenticated_api: bool,
    host: &str,
) -> Result<Option<ApiAuth>> {
    let api_key = api_key.map(|value| value.trim().to_string());
    if let Some(api_key) = api_key.filter(|value| !value.is_empty()) {
        return Ok(Some(ApiAuth::new(api_key)));
    }

    if allow_unauthenticated_api {
        warn!("API authentication is disabled by explicit configuration");
        return Ok(None);
    }

    if is_loopback_host(host) {
        warn!("API authentication is not configured; allowing unauthenticated loopback server");
        return Ok(None);
    }

    anyhow::bail!(
        "API authentication is required when binding to '{}'. Set IRONFLOW_API_KEY, or set IRONFLOW_ALLOW_UNAUTHENTICATED_API=true to opt out.",
        host
    );
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

pub async fn require_api_key(
    axum::extract::State(auth): axum::extract::State<ApiAuth>,
    req: Request,
    next: Next,
) -> Response {
    if request_has_api_key(req.headers(), &auth.api_key) {
        return next.run(req).await;
    }

    (
        StatusCode::UNAUTHORIZED,
        "missing or invalid API authentication",
    )
        .into_response()
}

fn request_has_api_key(headers: &HeaderMap, expected: &str) -> bool {
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected);

    let api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|token| token == expected);

    bearer || api_key
}
