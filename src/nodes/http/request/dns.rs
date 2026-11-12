use std::io;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use crate::nodes::http::address::ip_is_internal;

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

    // IF-191: a hostname whose answer contains any address in a refused
    // range is rejected as a whole, alone or mixed with a public address.
    #[test]
    fn rejects_dns_answers_in_every_refused_range() {
        use crate::nodes::http::test_support::REFUSED;

        let mut accepted = Vec::new();
        for (range, ip) in REFUSED {
            let answer: std::net::SocketAddr = (ip.parse::<std::net::IpAddr>().unwrap(), 0).into();
            for addresses in [vec![answer], vec![address("93.184.216.34"), answer]] {
                if validate_addresses(addresses).is_ok() {
                    accepted.push((range, ip));
                }
            }
        }
        assert!(accepted.is_empty(), "DNS answer accepted: {accepted:#?}");
    }

    #[test]
    fn accepts_public_dns_answers_adjacent_to_refused_ranges() {
        use crate::nodes::http::test_support::ALLOWED;

        let expected: Vec<std::net::SocketAddr> = ALLOWED
            .iter()
            .map(|ip| (ip.parse::<std::net::IpAddr>().unwrap(), 0).into())
            .collect();
        for answer in &expected {
            assert!(validate_addresses(vec![*answer]).is_ok(), "{answer}");
        }
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
