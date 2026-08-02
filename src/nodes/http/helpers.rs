use std::net::IpAddr;

use crate::engine::types::Context;
use crate::lua::interpolate::interpolate_value;
use anyhow::Result;

/// Recursively interpolate context templates in all JSON string values.
pub(super) fn interpolate_json_value(
    value: &serde_json::Value,
    ctx: &Context,
) -> serde_json::Value {
    interpolate_value(value, ctx)
}

/// True for IP addresses that belong to the local host or a private/internal
/// network, used by the opt-in SSRF guard.
fn ip_is_internal(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped IPv6 literals retain the security properties of the
            // embedded IPv4 address. Without this normalization,
            // `::ffff:127.0.0.1` bypasses the IPv6-only range checks below.
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return ip_is_internal(IpAddr::V4(mapped));
            }

            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (first & 0xfe00) == 0xfc00 // unique local fc00::/7
                || (first & 0xffc0) == 0xfe80 // link local fe80::/10
        }
    }
}

/// True if a URL's host targets the local host or a private network. Literal
/// IP addresses (including the cloud-metadata link-local range) and
/// `localhost` are recognized; hostnames that resolve to internal addresses via
/// DNS are not (that requires connection-level control and is out of scope).
pub(super) fn url_targets_internal_network(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => {
            let domain = domain.to_ascii_lowercase();
            domain == "localhost" || domain.ends_with(".localhost")
        }
        Some(url::Host::Ipv4(ip)) => ip_is_internal(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => ip_is_internal(IpAddr::V6(ip)),
        None => false,
    }
}

/// Simple percent-encoding for form body values.
pub(super) fn percent_encode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

pub(super) fn body_value_to_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(super) fn build_form_body(body: &serde_json::Value) -> Result<String> {
    let object = body
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("body_type='form' requires 'body' to be an object"))?;

    let mut pairs = Vec::with_capacity(object.len());
    for (key, value) in object {
        pairs.push(format!(
            "{}={}",
            percent_encode(key),
            percent_encode(&body_value_to_text(value))
        ));
    }
    Ok(pairs.join("&"))
}

#[cfg(test)]
mod tests {
    use super::url_targets_internal_network;

    fn internal(u: &str) -> bool {
        url_targets_internal_network(&url::Url::parse(u).unwrap())
    }

    #[test]
    fn flags_local_and_private_hosts() {
        assert!(internal("http://localhost/x"));
        assert!(internal("http://127.0.0.1/x"));
        assert!(internal("http://10.0.0.5/x"));
        assert!(internal("http://192.168.1.1/x"));
        assert!(internal("http://172.16.9.9/x"));
        assert!(internal("http://169.254.169.254/latest/meta-data")); // cloud metadata
        assert!(internal("http://[::1]/x"));
        assert!(internal("http://[fd00::1]/x")); // unique local
        assert!(internal("http://[::ffff:127.0.0.1]/x"));
        assert!(internal("http://[::ffff:10.0.0.5]/x"));
        assert!(internal("http://[::ffff:169.254.169.254]/x"));
    }

    #[test]
    fn allows_public_hosts() {
        assert!(!internal("http://example.com/x"));
        assert!(!internal("https://8.8.8.8/x"));
        assert!(!internal("http://93.184.216.34/x"));
        assert!(!internal("http://[::ffff:8.8.8.8]/x"));
    }
}
