//! The early refusal a node reports before it builds a guarded client.

use anyhow::Result;

use super::{Guard, Refusal, Target};
use crate::util::sensitive_url::SecretEndpoint;

/// The early, readable refusal for a node about to call `url`: the first
/// stage, then one lookup so a refused DNS answer is reported by the node and
/// not as an opaque transport error. Enforcement stays with the client the
/// guard is installed on. Does nothing when no guard applies or the URL does
/// not parse; the client reports a malformed URL in its own words.
pub(crate) async fn ensure_url_allowed(node: &str, url: &str) -> Result<()> {
    let Some(guard) = Guard::resolve(false)? else {
        return Ok(());
    };
    let Ok(parsed) = url::Url::parse(url) else {
        return Ok(());
    };
    let refuse = |refusal: Refusal| {
        anyhow::anyhow!(
            "{node}: request to {} blocked: {refusal} ({})",
            SecretEndpoint::new(url),
            guard.setting()
        )
    };
    if guard.check_url(&parsed).map_err(refuse)? == Target::NeedsResolution
        && let Some(host) = parsed.host_str()
        && let Ok(addresses) = guard.lookup(host, 0).await
    {
        guard.validate_answer(host, addresses).map_err(refuse)?;
    }
    Ok(())
}
