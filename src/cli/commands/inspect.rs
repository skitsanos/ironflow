use std::sync::Arc;

use anyhow::Result;

use crate::storage::{StateStore, StorageErrorKind};

pub(crate) async fn cmd_inspect(run_id: String, store: Arc<dyn StateStore>) -> Result<()> {
    let info = match store.get_run_info(&run_id).await {
        Ok(info) => info,
        // The record exists but the store cannot read it back. Say so and
        // name the remedy: deletion does not need to decode it (IF-220).
        Err(error) if error.kind() == StorageErrorKind::Corruption => anyhow::bail!(
            "run '{run_id}' has a stored record that cannot be decoded ({error}); \
             inspect cannot show it. Remove it with DELETE /runs/{run_id} on a server \
             that uses this store"
        ),
        Err(error) => return Err(error.into()),
    };

    let mut value = serde_json::to_value(&info)?;
    crate::util::redaction::redact_legacy_webhook_record(&mut value);
    println!("{}", serde_json::to_string_pretty(&value)?);

    Ok(())
}
