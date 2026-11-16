//! A task-scoped policy and DNS table for in-crate guard tests.
//!
//! The process policy is read from the environment, which parallel unit
//! tests must not mutate. A test wraps the node call in [`with_policy`]
//! instead; nothing here exists outside `cfg(test)`.

use std::collections::HashMap;
use std::future::Future;
use std::net::IpAddr;
use std::sync::Arc;

use super::allow::AllowList;
use super::policy::OutboundPolicy;

pub(crate) type Hosts = HashMap<String, Vec<IpAddr>>;

#[derive(Clone)]
struct Scope {
    policy: OutboundPolicy,
    hosts: Arc<Hosts>,
}

tokio::task_local! {
    static SCOPE: Scope;
}

pub(super) fn policy() -> Option<OutboundPolicy> {
    SCOPE.try_with(|scope| scope.policy.clone()).ok()
}

pub(super) fn hosts() -> Arc<Hosts> {
    SCOPE
        .try_with(|scope| scope.hosts.clone())
        .unwrap_or_default()
}

/// Run `future` under a process policy. `hosts` maps a name to the DNS
/// answer the guard sees for it; no real lookup is made for those names.
pub(crate) async fn with_policy<F: Future>(
    block_private_network: bool,
    allowlist: &[&str],
    hosts: &[(&str, &[&str])],
    future: F,
) -> F::Output {
    let scope = Scope {
        policy: OutboundPolicy {
            block_private_network,
            allow: Arc::new(AllowList::parse(allowlist.iter().copied()).unwrap()),
        },
        hosts: Arc::new(
            hosts
                .iter()
                .map(|(name, answer)| {
                    let answer = answer.iter().map(|ip| ip.parse().unwrap()).collect();
                    ((*name).to_owned(), answer)
                })
                .collect(),
        ),
    };
    SCOPE.scope(scope, future).await
}
