use std::future::IntoFuture;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{delete, get, post};
use tower_http::trace::TraceLayer;

use super::{AppState, ServeOptions, build_api_auth, cors_layer, handlers, require_api_key};
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

/// A fully validated and bound API server. Constructing this is the startup
/// barrier after which background schedulers may safely begin firing work.
pub(crate) struct PreparedServer {
    listener: tokio::net::TcpListener,
    app: Router,
    bound_addr: std::net::SocketAddr,
    lifecycle: super::ServiceLifecycle,
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    metrics: Option<Arc<crate::metrics::Metrics>>,
}

impl PreparedServer {
    pub(crate) async fn start_run_lifecycle(self) -> Result<RunningServer> {
        let store = self.store.clone();
        let reconciliation = crate::storage::reconcile_nonterminal_runs(store.as_ref());
        match bounded_startup_reconciliation(reconciliation, crate::storage::RUN_LEASE_REFRESH)
            .await?
        {
            0 => {}
            count => tracing::info!(
                count,
                "reconciled non-terminal runs as Stalled after restart"
            ),
        }
        let reaper = crate::storage::spawn_run_lease_reaper(store);
        Ok(RunningServer {
            prepared: self,
            _reaper: reaper,
        })
    }
}

async fn bounded_startup_reconciliation<F>(
    reconciliation: F,
    timeout: std::time::Duration,
) -> Result<usize>
where
    F: std::future::Future<Output = crate::storage::StorageResult<usize>>,
{
    match tokio::time::timeout(timeout, reconciliation).await {
        Ok(result) => Ok(result?),
        Err(_) => anyhow::bail!(
            "startup run-lease reconciliation timed out after {}ms",
            timeout.as_millis()
        ),
    }
}

/// A bound server whose immediate and periodic ownership reconciliation is
/// active. Both the public API entry point and CLI scheduler use this typestate
/// so embedded servers cannot accidentally omit replica recovery.
pub(crate) struct RunningServer {
    prepared: PreparedServer,
    _reaper: crate::storage::RunLeaseReaper,
}

impl RunningServer {
    pub(crate) fn lifecycle(&self) -> super::ServiceLifecycle {
        self.prepared.lifecycle.clone()
    }

    pub(crate) fn store(&self) -> Arc<dyn StateStore> {
        self.prepared.store.clone()
    }

    pub(crate) fn event_store(&self) -> Arc<dyn EventStore> {
        self.prepared.event_store.clone()
    }

    pub(crate) fn metrics(&self) -> Option<Arc<crate::metrics::Metrics>> {
        self.prepared.metrics.clone()
    }

    pub(crate) async fn serve(self) -> Result<()> {
        let grace = crate::util::runtime_config::shutdown_grace()?;
        self.serve_until(shutdown_signal(), grace).await
    }

    async fn serve_until<F>(self, shutdown: F, grace: std::time::Duration) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        tracing::info!(
            "IronFlow API server listening on {}",
            self.prepared.bound_addr
        );
        let lifecycle = self.prepared.lifecycle.clone();
        let graceful_lifecycle = lifecycle.clone();
        let server = axum::serve(self.prepared.listener, self.prepared.app)
            .with_graceful_shutdown(async move { graceful_lifecycle.wait_for_draining().await })
            .into_future();
        tokio::pin!(server);
        tokio::pin!(shutdown);

        let completed = tokio::select! {
            result = &mut server => Some(result),
            () = &mut shutdown => {
                lifecycle.begin_draining();
                None
            }
        };

        lifecycle.begin_draining();
        lifecycle.drain(grace).await;

        if let Some(result) = completed {
            result?;
            return Ok(());
        }

        match tokio::time::timeout(std::time::Duration::from_secs(5), &mut server).await {
            Ok(result) => result?,
            Err(_) => tracing::warn!(
                "HTTP connections did not close after the run drain; forcing process shutdown"
            ),
        }
        Ok(())
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler can be installed");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .expect("shutdown signal handler can be installed");
}

pub(crate) async fn prepare(
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    options: ServeOptions,
) -> Result<PreparedServer> {
    validate_execution_policy(options.allow_adhoc_flows, options.flows_dir.as_deref())?;
    super::admission::validate_configuration()?;
    let _ = crate::util::runtime_config::run_deadline()?;
    let _ = crate::util::runtime_config::shutdown_grace()?;
    let max_concurrent_tasks =
        crate::util::runtime_config::max_concurrent_tasks(options.max_concurrent_tasks)?;

    let metrics = options
        .metrics_enabled
        .then(|| Arc::new(crate::metrics::Metrics::new()));
    let store = match &metrics {
        Some(metrics) => crate::metrics::observe_state_store(store, metrics.clone()),
        None => store,
    };
    let event_store = match &metrics {
        Some(metrics) => crate::metrics::observe_event_store(event_store, metrics.clone()),
        None => event_store,
    };

    let listener = bind_listener(&options.host, options.port).await?;
    let bound_addr = listener.local_addr()?;
    let auth = build_api_auth(
        options.api_key,
        options.allow_unauthenticated_api,
        bound_addr.ip().is_loopback(),
        &options.host,
    )?;
    let cors = cors_layer(options.cors_origins)?;
    super::webhook_config::validate_runtime_configs(&options.webhooks)?;
    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store,
        event_store,
        flows_dir: options.flows_dir,
        max_concurrent_tasks: Some(max_concurrent_tasks),
        listing_policy: options.listing_policy,
        webhooks: options.webhooks,
        allow_adhoc_flows: options.allow_adhoc_flows,
        lifecycle: super::ServiceLifecycle::default(),
        metrics,
    });

    let mut protected_routes = Router::new()
        .route("/flows/run", post(handlers::run_flow))
        .route("/flows/validate", post(handlers::validate_flow))
        .route("/runs", get(handlers::list_runs))
        .route("/runs/{id}", get(handlers::get_run))
        .route("/runs/{id}/events", get(handlers::run_events))
        .route("/runs/{id}", delete(handlers::delete_run))
        .route("/nodes", get(handlers::list_nodes))
        .route("/webhooks/{name}", post(handlers::run_webhook));
    if options.metrics_enabled {
        protected_routes = protected_routes.route("/metrics", get(handlers::metrics));
    }
    let protected_routes = if let Some(auth) = auth {
        protected_routes.layer(middleware::from_fn_with_state(auth, require_api_key))
    } else {
        protected_routes
    };

    let app = Router::new()
        .route("/health", get(handlers::health))
        .route("/health/live", get(handlers::liveness))
        .route("/health/ready", get(handlers::readiness))
        .merge(protected_routes)
        .layer(DefaultBodyLimit::max(options.max_body))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state.clone());

    Ok(PreparedServer {
        listener,
        app,
        bound_addr,
        lifecycle: state.lifecycle.clone(),
        store: state.store.clone(),
        event_store: state.event_store.clone(),
        metrics: state.metrics.clone(),
    })
}

fn validate_execution_policy(
    allow_adhoc_flows: bool,
    flows_dir: Option<&std::path::Path>,
) -> Result<()> {
    if !allow_adhoc_flows && flows_dir.is_none() {
        anyhow::bail!(
            "allow_adhoc_flows=false requires a configured flows_dir so file execution remains confined"
        );
    }
    Ok(())
}

pub(super) async fn bind_listener(host: &str, port: u16) -> Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind((host, port))
        .await
        .map_err(|error| anyhow::anyhow!("failed to bind API server to {host}:{port}: {error}"))
}

#[cfg(test)]
mod tests;
