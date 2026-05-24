use std::sync::Arc;

use anyhow::{Context as _, Result};

use crate::storage::StateStore;

pub(crate) async fn cmd_inspect(run_id: String, store: Arc<dyn StateStore>) -> Result<()> {
    let info = store
        .get_run_info(&run_id)
        .await
        .with_context(|| format!("Run '{}' not found", run_id))?;

    println!("{}", serde_json::to_string_pretty(&info)?);

    Ok(())
}
