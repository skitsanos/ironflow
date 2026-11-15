//! The outbound destination guard shared by every covered node (IF-191,
//! IF-212).
//!
//! A [`Guard`] exists for one node execution when the operator forced the
//! policy on (`IRONFLOW_BLOCK_PRIVATE_NETWORK`) or the node opted in
//! (`block_private_network` on the HTTP nodes). It refuses a destination that
//! is not a public unicast address in two stages:
//!
//! 1. [`Guard::check_target`] sees the requested host and port: a literal
//!    address is classified at once, `localhost` is refused by name, and any
//!    other name goes on to DNS.
//! 2. [`Guard::validate_answer`] sees the complete DNS answer and refuses it
//!    when any address is internal. HTTP clients install
//!    [`Guard::dns_resolver`], so the validated answer is the one connected to.
//!
//! The operator allowlist is the only exception, and it is matched as
//! follows. A host-name entry matches the requested host exactly; the
//! addresses that name resolves to are then permitted for that connection
//! only, so another name resolving to the same address is still refused. A
//! name listed only with ports is refused on every other port. An address or
//! CIDR entry matches a literal destination and, when it has no port, a DNS
//! answer.

pub(crate) mod address;
pub(crate) mod allow;
#[cfg(test)]
pub(crate) mod node_checks;
pub(crate) mod policy;
mod resolver;
#[cfg(test)]
pub(crate) mod test_listener;
#[cfg(test)]
pub(crate) mod test_scope;
#[cfg(test)]
pub(crate) mod test_support;

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use anyhow::Result;

use address::ip_is_internal;
use allow::{AllowList, NameMatch};
pub(crate) use resolver::GuardedResolver;

/// Why the guard is active, for the diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Process,
    Node,
}

/// Why a destination was refused. The text never contains the destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    PrivateAddress,
    PrivateDnsAnswer,
    EmptyDnsAnswer,
    OtherPortsOnly,
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PrivateAddress => "target is a private network address",
            Self::PrivateDnsAnswer => "DNS resolution returned a private network address",
            Self::EmptyDnsAnswer => "DNS resolution returned no addresses",
            Self::OtherPortsOnly => {
                "the private-network allowlist permits this host on other ports only"
            }
        })
    }
}

impl std::error::Error for Refusal {}

/// The first-stage decision for a requested host and port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// A public literal, or a literal or name the allowlist admits.
    Permitted,
    /// A name whose DNS answer must pass [`Guard::validate_answer`].
    NeedsResolution,
}

#[derive(Clone, Debug)]
pub(crate) struct Guard {
    allow: Arc<AllowList>,
    source: Source,
    #[cfg(test)]
    hosts: Arc<test_scope::Hosts>,
}

impl Guard {
    /// The guard for a node execution, or `None` when neither the process
    /// policy nor the node asks for one. `node_opt_in` can only tighten.
    pub(crate) fn resolve(node_opt_in: bool) -> Result<Option<Self>> {
        let policy = policy::current()?;
        let source = if policy.block_private_network {
            Source::Process
        } else if node_opt_in {
            Source::Node
        } else {
            return Ok(None);
        };
        Ok(Some(Self {
            allow: policy.allow,
            source,
            #[cfg(test)]
            hosts: test_scope::hosts(),
        }))
    }

    /// A guard as a node opt-in creates it: no process policy, no allowlist.
    #[cfg(test)]
    pub(crate) fn node_only() -> Self {
        Self {
            allow: Arc::default(),
            source: Source::Node,
            hosts: Arc::default(),
        }
    }

    /// The setting to name in a refusal.
    pub(crate) fn setting(&self) -> &'static str {
        match self.source {
            Source::Process => {
                "IRONFLOW_BLOCK_PRIVATE_NETWORK is enabled; the operator can permit a destination with IRONFLOW_PRIVATE_NETWORK_ALLOWLIST"
            }
            Source::Node => "block_private_network is enabled",
        }
    }

    pub(crate) fn check_url(&self, url: &url::Url) -> Result<Target, Refusal> {
        let port = url.port_or_known_default().unwrap_or(0);
        match url.host() {
            Some(url::Host::Domain(domain)) => self.check_name(domain, port),
            Some(url::Host::Ipv4(ip)) => self.check_literal(IpAddr::V4(ip), port),
            Some(url::Host::Ipv6(ip)) => self.check_literal(IpAddr::V6(ip), port),
            None => Ok(Target::Permitted),
        }
    }

    pub(super) fn check_literal(&self, ip: IpAddr, port: u16) -> Result<Target, Refusal> {
        if !ip_is_internal(ip) || self.allow.allows_literal(ip, port) {
            Ok(Target::Permitted)
        } else {
            Err(Refusal::PrivateAddress)
        }
    }

    pub(super) fn check_name(&self, name: &str, port: u16) -> Result<Target, Refusal> {
        match self.allow.match_name(name, port) {
            NameMatch::Allowed => return Ok(Target::Permitted),
            NameMatch::OtherPortsOnly => return Err(Refusal::OtherPortsOnly),
            NameMatch::Unlisted => {}
        }
        let name = allow::normalize_name(name);
        if name == "localhost" || name.ends_with(".localhost") {
            return Err(Refusal::PrivateAddress);
        }
        Ok(Target::NeedsResolution)
    }

    /// Second stage: accept or refuse one complete DNS answer. An allowlisted
    /// name keeps whatever it resolved to; the first stage has already checked
    /// its port.
    pub(crate) fn validate_answer(
        &self,
        name: &str,
        addresses: Vec<SocketAddr>,
    ) -> Result<Vec<SocketAddr>, Refusal> {
        if addresses.is_empty() {
            return Err(Refusal::EmptyDnsAnswer);
        }
        if self.allow.names_host(name) {
            return Ok(addresses);
        }
        let refused = |address: &SocketAddr| {
            ip_is_internal(address.ip()) && !self.allow.allows_resolved(address.ip())
        };
        if addresses.iter().any(refused) {
            return Err(Refusal::PrivateDnsAnswer);
        }
        Ok(addresses)
    }

    /// The reqwest resolver that enforces the second stage on every
    /// connection of the client it is installed on.
    pub(crate) fn dns_resolver(&self) -> GuardedResolver {
        GuardedResolver::new(self.clone())
    }

    pub(crate) async fn lookup(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        #[cfg(test)]
        if let Some(answer) = self.hosts.get(&allow::normalize_name(host)) {
            return Ok(answer.iter().map(|ip| SocketAddr::new(*ip, port)).collect());
        }
        Ok(tokio::net::lookup_host((host, port)).await?.collect())
    }
}

#[cfg(test)]
#[path = "outbound/guard_tests.rs"]
mod guard_tests;
#[cfg(test)]
#[path = "outbound/http_tests.rs"]
mod http_tests;
