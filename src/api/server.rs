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
}

impl PreparedServer {
    pub(crate) async fn start_run_lifecycle(
        self,
        store: Arc<dyn StateStore>,
    ) -> Result<RunningServer> {
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
    pub(crate) async fn serve(self) -> Result<()> {
        tracing::info!(
            "IronFlow API server listening on {}",
            self.prepared.bound_addr
        );
        axum::serve(self.prepared.listener, self.prepared.app).await?;
        Ok(())
    }
}

pub(crate) async fn prepare(
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    options: ServeOptions,
) -> Result<PreparedServer> {
    validate_execution_policy(options.allow_adhoc_flows, options.flows_dir.as_deref())?;
    super::admission::validate_configuration()?;
    let _ = crate::util::runtime_config::run_deadline()?;
    let max_concurrent_tasks =
        crate::util::runtime_config::max_concurrent_tasks(options.max_concurrent_tasks)?;

    let listener = bind_listener(&options.host, options.port).await?;
    let bound_addr = listener.local_addr()?;
    let auth = build_api_auth(
        options.api_key,
        options.allow_unauthenticated_api,
        bound_addr.ip().is_loopback(),
        &options.host,
    )?;
    let cors = cors_layer(options.cors_origins)?;

    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store,
        event_store,
        flows_dir: options.flows_dir,
        max_concurrent_tasks: Some(max_concurrent_tasks),
        listing_policy: options.listing_policy,
        webhooks: options.webhooks,
        allow_adhoc_flows: options.allow_adhoc_flows,
    });

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
        .layer(cors)
        .with_state(state);

    Ok(PreparedServer {
        listener,
        app,
        bound_addr,
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
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Arc;

    use crate::api::ServeOptions;
    use crate::storage::event_store::MemoryEventStore;
    use crate::storage::json_store::JsonStateStore;
    use crate::storage::{RunLease, StateStore};
    use crate::util::listing::ListingPolicy;

    #[test]
    fn restricted_file_execution_requires_a_confinement_root() {
        let error = super::validate_execution_policy(false, None).unwrap_err();
        assert!(error.to_string().contains("flows_dir"));
        super::validate_execution_policy(false, Some(Path::new("flows"))).unwrap();
        super::validate_execution_policy(true, None).unwrap();
    }

    #[tokio::test]
    async fn common_server_lifecycle_reconciles_expired_owners() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(JsonStateStore::new(directory.path()));
        store
            .init_run_owned(
                "expired",
                "flow",
                &HashMap::new(),
                &RunLease::at(
                    "dead-owner",
                    chrono::Utc::now() - chrono::Duration::seconds(1),
                ),
            )
            .await
            .unwrap();
        let options = ServeOptions {
            host: "127.0.0.1".to_string(),
            port: 0,
            flows_dir: None,
            max_body: 1024,
            max_concurrent_tasks: Some(1),
            listing_policy: ListingPolicy::default(),
            webhooks: HashMap::new(),
            allow_adhoc_flows: true,
            cors_origins: None,
            api_key: None,
            allow_unauthenticated_api: false,
        };

        let prepared = super::prepare(store.clone(), Arc::new(MemoryEventStore::new()), options)
            .await
            .unwrap();
        let running = prepared.start_run_lifecycle(store.clone()).await.unwrap();
        assert_eq!(
            store.get_run_info("expired").await.unwrap().status,
            crate::engine::types::RunStatus::Stalled
        );
        drop(running);
    }

    #[tokio::test]
    async fn hanging_startup_reconciliation_fails_within_its_budget() {
        let reconciliation = std::future::pending::<crate::storage::StorageResult<usize>>();
        let error = super::bounded_startup_reconciliation(
            reconciliation,
            std::time::Duration::from_millis(5),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("timed out"));
    }
}
