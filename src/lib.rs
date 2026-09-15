pub mod api;
pub mod artifacts;
pub mod cli;
pub mod engine;
pub mod lua;
pub mod metrics;
pub mod nodes;
pub mod scheduler;
pub mod storage;
pub mod util;

/// Install the process-wide Rustls crypto provider before any TLS client is
/// built.
///
/// The `postgres` and `redis` features pull both `ring` and `aws-lc-rs` into
/// the dependency graph, so Rustls cannot pick a default on its own and panics
/// inside the first certificate verifier (for example a PostgreSQL
/// `sslmode=verify-ca` connection from a `db_query` step). The CLI binary
/// calls this once at startup, and the library calls it again at each public
/// storage boundary (`SqlStateStore`, `SqlEventStore`, the Redis stores) and
/// in the SQL nodes, so an embedder gets a connection error rather than a
/// panic even without calling it. Calling it early is still recommended when
/// the embedding application builds other Rustls clients first. The call is
/// idempotent, preserves a provider the embedder already installed, and is a
/// no-op when neither storage TLS feature is enabled.
pub fn initialize_tls_provider() {
    #[cfg(any(feature = "postgres", feature = "redis"))]
    storage::tls::initialize_provider();
}
