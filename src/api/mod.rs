mod admission;
pub mod errors;
pub mod handlers;
mod idempotency;
mod lifecycle;
mod server;
mod static_files;
mod webhook_config;
mod webhook_signature;

pub use lifecycle::ServiceLifecycle;
pub use static_files::StaticFilesConfig;
pub use webhook_config::WebhookConfig;
pub use webhook_signature::WebhookSignatureConfig;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::http::{HeaderMap, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing::warn;

use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;
use crate::util::listing::ListingPolicy;

/// Shared application state accessible by all handlers.
pub struct AppState {
    pub registry: Arc<NodeRegistry>,
    pub store: Arc<dyn StateStore>,
    pub event_store: Arc<dyn EventStore>,
    pub flows_dir: Option<PathBuf>,
    pub max_concurrent_tasks: Option<usize>,
    pub listing_policy: ListingPolicy,
    /// Named webhook route definitions from config.
    pub webhooks: HashMap<String, WebhookConfig>,
    /// When false, `/flows/run` and `/flows/validate` refuse inline flow
    /// source. See `ServeOptions::allow_adhoc_flows`.
    pub allow_adhoc_flows: bool,
    /// Process lifecycle used to reject new execution while draining and to
    /// track accepted runs through graceful shutdown.
    pub lifecycle: ServiceLifecycle,
    /// Process-local operator metrics. `None` keeps the metrics surface and
    /// all recording overhead disabled.
    pub metrics: Option<Arc<crate::metrics::Metrics>>,
}

pub(crate) use admission::{
    acquire_flow_load_permit, acquire_run_permit, supervise_flow_load, wait_for_admitted_run,
};

/// Configuration for the REST API server.
pub struct ServeOptions {
    pub host: String,
    pub port: u16,
    pub flows_dir: Option<PathBuf>,
    pub max_body: usize,
    pub max_concurrent_tasks: Option<usize>,
    pub listing_policy: ListingPolicy,
    pub webhooks: HashMap<String, WebhookConfig>,
    /// When false, `/flows/run` and `/flows/validate` refuse inline `source` /
    /// `source_base64` and only accept flow files already present under
    /// `flows_dir`.
    pub allow_adhoc_flows: bool,
    pub cors_origins: Option<Vec<String>>,
    pub api_key: Option<String>,
    pub allow_unauthenticated_api: bool,
    /// Expose the authenticated, process-local `GET /metrics` endpoint.
    pub metrics_enabled: bool,
    /// Optional public static-file root and browser fallback policy.
    pub static_files: Option<StaticFilesConfig>,
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

pub(crate) use server::prepare;

/// Start the REST API server.
pub async fn serve(
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    options: ServeOptions,
) -> Result<()> {
    prepare(store, event_store, options)
        .await?
        .start_run_lifecycle()
        .await?
        .serve()
        .await
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
    is_loopback: bool,
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

    if is_loopback {
        warn!("API authentication is not configured; allowing unauthenticated loopback server");
        return Ok(None);
    }

    anyhow::bail!(
        "API authentication is required when binding to '{}'. Set IRONFLOW_API_KEY, or set IRONFLOW_ALLOW_UNAUTHENTICATED_API=true to opt out.",
        host
    );
}

pub async fn require_api_key(
    axum::extract::State(auth): axum::extract::State<ApiAuth>,
    mut req: Request,
    next: Next,
) -> Response {
    if request_has_api_key(req.headers(), &auth.api_key) {
        // Authentication credentials authorize access to IronFlow itself.
        // Consume both supported forms before webhook workflow ingress so
        // handlers cannot accidentally treat a platform secret as business
        // input.
        req.headers_mut().remove(axum::http::header::AUTHORIZATION);
        req.headers_mut().remove("x-api-key");
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
        .is_some_and(|token| {
            crate::util::authentication::constant_time_eq(token.as_bytes(), expected.as_bytes())
        });

    let api_key = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|token| {
            crate::util::authentication::constant_time_eq(token.as_bytes(), expected.as_bytes())
        });

    bearer || api_key
}

#[cfg(test)]
mod tests {
    use super::request_has_api_key;
    use axum::http::HeaderMap;

    #[tokio::test]
    async fn bind_listener_accepts_hostname_and_ipv4_loopback() {
        for host in ["localhost", "127.0.0.1"] {
            let listener = super::server::bind_listener(host, 0).await.unwrap();
            assert!(listener.local_addr().unwrap().ip().is_loopback());
        }
    }

    #[tokio::test]
    async fn bind_listener_accepts_unbracketed_ipv6_loopback() {
        let listener = super::server::bind_listener("::1", 0).await.unwrap();
        assert!(listener.local_addr().unwrap().ip().is_loopback());
    }

    #[test]
    fn request_has_api_key_accepts_valid_and_rejects_invalid() {
        let expected = "s3cr3t-key";

        let mut bearer = HeaderMap::new();
        bearer.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer s3cr3t-key".parse().unwrap(),
        );
        assert!(request_has_api_key(&bearer, expected));

        let mut api_key = HeaderMap::new();
        api_key.insert("x-api-key", "s3cr3t-key".parse().unwrap());
        assert!(request_has_api_key(&api_key, expected));

        let mut wrong = HeaderMap::new();
        wrong.insert("x-api-key", "wrong-key!!".parse().unwrap());
        assert!(!request_has_api_key(&wrong, expected));

        assert!(!request_has_api_key(&HeaderMap::new(), expected));
    }
}
