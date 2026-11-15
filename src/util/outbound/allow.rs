//! The operator allowlist of internal destinations (IF-212).
//!
//! Entries come only from `IRONFLOW_PRIVATE_NETWORK_ALLOWLIST` or the
//! `private_network_allowlist` config key. Nothing in a flow can add one.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use anyhow::{Result, bail};

/// A bound on parsing and matching work; a real list is a handful of entries.
pub(crate) const MAX_ENTRIES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
struct NameEntry {
    name: String,
    port: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NetEntry {
    network: IpAddr,
    prefix: u8,
    port: Option<u16>,
}

/// How a requested hostname relates to the name entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NameMatch {
    /// No entry names this host.
    Unlisted,
    /// An entry names this host and admits the requested port.
    Allowed,
    /// Entries name this host, but only for other ports.
    OtherPortsOnly,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AllowList {
    names: Vec<NameEntry>,
    nets: Vec<NetEntry>,
}

impl AllowList {
    /// Parse every entry; the first malformed one fails the whole list. The
    /// error names the entry position, never the configured value's source.
    pub(crate) fn parse<'a>(entries: impl IntoIterator<Item = &'a str>) -> Result<Self> {
        let mut list = Self::default();
        for (index, raw) in entries.into_iter().enumerate() {
            if index >= MAX_ENTRIES {
                bail!("the private-network allowlist has more than {MAX_ENTRIES} entries");
            }
            let entry = raw.trim();
            parse_entry(entry, &mut list)
                .map_err(|error| anyhow::anyhow!("allowlist entry {}: {error}", index + 1))?;
        }
        Ok(list)
    }

    pub(crate) fn len(&self) -> usize {
        self.names.len() + self.nets.len()
    }

    /// Match the requested host name exactly (ASCII case-insensitive, one
    /// trailing dot ignored). There is no suffix or wildcard matching.
    pub(crate) fn match_name(&self, host: &str, port: u16) -> NameMatch {
        let host = normalize_name(host);
        let mut listed = false;
        for entry in self.names.iter().filter(|entry| entry.name == host) {
            if entry.port.is_none_or(|allowed| allowed == port) {
                return NameMatch::Allowed;
            }
            listed = true;
        }
        if listed {
            NameMatch::OtherPortsOnly
        } else {
            NameMatch::Unlisted
        }
    }

    /// True when any entry names this host, on whatever port.
    pub(crate) fn names_host(&self, host: &str) -> bool {
        let host = normalize_name(host);
        self.names.iter().any(|entry| entry.name == host)
    }

    /// A literal destination address: an address or range entry admits it
    /// when the entry has no port or the same port.
    pub(crate) fn allows_literal(&self, ip: IpAddr, port: u16) -> bool {
        self.nets
            .iter()
            .any(|net| net.port.is_none_or(|allowed| allowed == port) && net.contains(ip))
    }

    /// A DNS answer: only entries without a port apply, because the resolver
    /// is not told which port the connection will use.
    pub(crate) fn allows_resolved(&self, ip: IpAddr) -> bool {
        self.nets
            .iter()
            .any(|net| net.port.is_none() && net.contains(ip))
    }
}

impl NetEntry {
    fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip.to_canonical()) {
            (IpAddr::V4(network), IpAddr::V4(ip)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                u32::from(network) == u32::from(ip) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(ip)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                u128::from(network) == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

pub(crate) fn normalize_name(host: &str) -> String {
    host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase()
}

fn parse_entry(entry: &str, list: &mut AllowList) -> Result<()> {
    if entry.is_empty() {
        bail!("is empty");
    }
    if entry.contains("://") {
        bail!("must be a host, host:port or CIDR range, not a URL");
    }
    if let Some(rest) = entry.strip_prefix('[') {
        let Some((address, tail)) = rest.split_once(']') else {
            bail!("has an unclosed '['");
        };
        let address: Ipv6Addr = address
            .parse()
            .map_err(|_| anyhow::anyhow!("has an invalid IPv6 address in brackets"))?;
        let port = match tail.strip_prefix(':') {
            Some(port) => Some(parse_port(port)?),
            None if tail.is_empty() => None,
            None => bail!("has unexpected text after ']'"),
        };
        list.nets.push(net(IpAddr::V6(address), 128, port)?);
        return Ok(());
    }
    if let Some((address, prefix)) = entry.split_once('/') {
        let address: IpAddr = address
            .parse()
            .map_err(|_| anyhow::anyhow!("has an invalid CIDR network address"))?;
        let prefix: u8 = prefix
            .parse()
            .map_err(|_| anyhow::anyhow!("has an invalid CIDR prefix length"))?;
        list.nets.push(net(address, prefix, None)?);
        return Ok(());
    }
    if let Ok(address) = entry.parse::<IpAddr>() {
        let prefix = if address.is_ipv4() { 32 } else { 128 };
        list.nets.push(net(address, prefix, None)?);
        return Ok(());
    }
    let (host, port) = match entry.rsplit_once(':') {
        Some((host, port)) => (host, Some(parse_port(port)?)),
        None => (entry, None),
    };
    if let Ok(address) = host.parse::<Ipv4Addr>() {
        list.nets.push(net(IpAddr::V4(address), 32, port)?);
        return Ok(());
    }
    list.names.push(NameEntry {
        name: parse_name(host)?,
        port,
    });
    Ok(())
}

fn net(network: IpAddr, prefix: u8, port: Option<u16>) -> Result<NetEntry> {
    let width = if network.is_ipv4() { 32 } else { 128 };
    if prefix > width {
        bail!("has a CIDR prefix length above {width}");
    }
    let entry = NetEntry {
        network,
        prefix,
        port,
    };
    // `10.0.0.5/8` is far more likely a typo than an intended `10.0.0.0/8`.
    if !entry.contains(network) {
        bail!("has address bits set beyond its CIDR prefix length");
    }
    Ok(entry)
}

fn parse_port(port: &str) -> Result<u16> {
    // `u16::from_str` accepts a leading `+`; a port is plain digits.
    let digits = !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit());
    match port.parse::<u16>() {
        Ok(port) if digits && port > 0 => Ok(port),
        _ => bail!("has an invalid port"),
    }
}

fn parse_name(host: &str) -> Result<String> {
    if host.contains('*') {
        bail!("uses a wildcard; list each host name or a CIDR range");
    }
    let name = normalize_name(host);
    let valid_label = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    };
    if name.is_empty() || name.len() > 253 || !name.split('.').all(valid_label) {
        bail!("is not a valid host name, host:port, address or CIDR range");
    }
    // An all-numeric name would be read by a URL parser as an IPv4 address.
    if name
        .split('.')
        .all(|label| label.bytes().all(|byte| byte.is_ascii_digit()))
    {
        bail!("is not a valid host name, host:port, address or CIDR range");
    }
    Ok(name)
}

#[cfg(test)]
#[path = "allow_tests.rs"]
mod tests;
