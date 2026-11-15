//! The guard for destinations given as a host and port instead of a URL
//! (SMTP relays and database servers).

use std::net::{IpAddr, SocketAddr};

use anyhow::Result;

use super::{Guard, Refusal, Target};
use crate::util::sensitive_url::Connection;

impl Guard {
    /// First stage for a host given as text, as SMTP and database settings are.
    pub(crate) fn check_target(&self, host: &str, port: u16) -> Result<Target, Refusal> {
        let bare = host
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(host);
        match bare.parse::<IpAddr>() {
            Ok(ip) => self.check_literal(ip, port),
            Err(_) => self.check_name(host, port),
        }
    }

    /// Resolve `host` once and return only a validated answer, with `port`
    /// applied. A literal address is returned as is after the first stage.
    pub(crate) async fn resolve_target(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>> {
        let bare = host
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(host);
        self.check_target(bare, port)?;
        if let Ok(ip) = bare.parse::<IpAddr>() {
            return Ok(vec![SocketAddr::new(ip, port)]);
        }
        let addresses = self
            .lookup(bare, port)
            .await
            .map_err(|_| anyhow::anyhow!("DNS resolution failed"))?;
        Ok(self.validate_answer(bare, addresses)?)
    }
}

/// The same early refusal for a `host` and `port` that are not a URL.
pub(crate) async fn resolve_host_allowed(
    node: &str,
    host: &str,
    port: u16,
) -> Result<Option<Vec<SocketAddr>>> {
    let Some(guard) = Guard::resolve(false)? else {
        return Ok(None);
    };
    guard
        .resolve_target(host, port)
        .await
        .map(Some)
        .map_err(|error| {
            anyhow::anyhow!(
                "{node}: connection to {} refused: {error} ({})",
                Connection::new(host),
                guard.setting()
            )
        })
}
