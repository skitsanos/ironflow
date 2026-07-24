use std::sync::Arc;

use anyhow::Result;

use crate::storage::StateStore;

pub(crate) async fn cmd_inspect(run_id: String, store: Arc<dyn StateStore>) -> Result<()> {
    let info = store.get_run_info(&run_id).await?;

    let mut value = serde_json::to_value(&info)?;
    crate::util::redaction::redact_legacy_webhook_record(&mut value);
    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}
