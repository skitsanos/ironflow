//! DNS pinning for guarded HTTP clients.

use std::io;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use super::Guard;

/// Resolves a host name once, refuses the complete answer when the guard
/// does, and hands only the validated answer to the connector, so the address
/// that was checked is the address that is connected to.
#[derive(Clone, Debug)]
pub(crate) struct GuardedResolver {
    guard: Guard,
}

impl GuardedResolver {
    pub(super) fn new(guard: Guard) -> Self {
        Self { guard }
    }
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let guard = self.guard.clone();
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses = guard.lookup(&host, 0).await?;
            let validated = guard.validate_answer(&host, addresses).map_err(denied)?;
            Ok(Box::new(validated.into_iter()) as Addrs)
        })
    }
}

fn denied(refusal: super::Refusal) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(io::Error::new(io::ErrorKind::PermissionDenied, refusal))
}
