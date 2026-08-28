use std::io;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use crate::nodes::http::helpers::ip_is_internal;

/// Resolve a hostname once, reject the complete answer when any address is
/// internal, and return only that validated answer to reqwest's connector.
#[derive(Clone, Debug, Default)]
pub(super) struct PublicDnsResolver;

impl Resolve for PublicDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .collect::<Vec<_>>();
            validate_addresses(addresses)
        })
    }
}

fn validate_addresses(
    addresses: Vec<std::net::SocketAddr>,
) -> Result<Addrs, Box<dyn std::error::Error + Send + Sync>> {
    if addresses.is_empty() {
        return Err(dns_error("DNS resolution returned no addresses"));
    }
    if addresses.iter().any(|address| ip_is_internal(address.ip())) {
        return Err(dns_error(
            "DNS resolution returned a private network address",
        ));
    }
    Ok(Box::new(addresses.into_iter()))
}

fn dns_error(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(io::Error::new(io::ErrorKind::PermissionDenied, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(ip: &str) -> std::net::SocketAddr {
        format!("{ip}:0").parse().unwrap()
    }

    #[test]
    fn rejects_private_and_mixed_dns_answers() {
        for addresses in [
            vec![address("127.0.0.1")],
            vec![address("93.184.216.34"), address("10.0.0.1")],
            vec![address("[::ffff:169.254.169.254]")],
        ] {
            let error = match validate_addresses(addresses) {
                Ok(_) => panic!("private DNS answer should be rejected"),
                Err(error) => error.to_string(),
            };
            assert!(error.contains("private network"), "{error}");
        }
    }

    #[test]
    fn preserves_the_validated_public_answer() {
        let expected = vec![
            address("93.184.216.34"),
            address("[2606:2800:220:1:248:1893:25c8:1946]"),
        ];
        let actual = validate_addresses(expected.clone())
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn rejects_empty_answers() {
        assert!(validate_addresses(Vec::new()).is_err());
    }
}
