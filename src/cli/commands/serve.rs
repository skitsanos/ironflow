use std::sync::Arc;

use anyhow::Result;

use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

/// Execute the `serve` subcommand.
pub(crate) async fn cmd_serve(
    store: Arc<dyn StateStore>,
    event_store: Arc<dyn EventStore>,
    options: crate::api::ServeOptions,
) -> Result<()> {
    crate::api::serve(store, event_store, options).await
}
