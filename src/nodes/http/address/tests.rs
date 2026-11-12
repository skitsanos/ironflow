use std::net::IpAddr;

use super::ip_is_internal;
use crate::nodes::http::helpers::url_targets_internal_network;
use crate::nodes::http::test_support::{ALLOWED, REFUSED, url_host};

fn url_is_internal(address: &str) -> bool {
    let url = format!("http://{}:8080/path", url_host(address));
    url_targets_internal_network(&url::Url::parse(&url).unwrap())
}

#[test]
fn classifier_refuses_every_special_purpose_range() {
    let passed: Vec<_> = REFUSED
        .iter()
        .filter(|(_, address)| !ip_is_internal(address.parse::<IpAddr>().unwrap()))
        .collect();
    assert!(passed.is_empty(), "classified as public: {passed:#?}");
}

#[test]
fn classifier_allows_adjacent_public_addresses() {
    let refused: Vec<_> = ALLOWED
        .iter()
        .filter(|address| ip_is_internal(address.parse::<IpAddr>().unwrap()))
        .collect();
    assert!(refused.is_empty(), "classified as internal: {refused:#?}");
}

#[test]
fn url_literals_in_refused_ranges_are_internal() {
    let passed: Vec<_> = REFUSED
        .iter()
        .filter(|(_, address)| !url_is_internal(address))
        .collect();
    assert!(passed.is_empty(), "URL literal passed: {passed:#?}");
}

#[test]
fn url_literals_for_public_addresses_pass() {
    let refused: Vec<_> = ALLOWED
        .iter()
        .filter(|address| url_is_internal(address))
        .collect();
    assert!(refused.is_empty(), "URL literal refused: {refused:#?}");
}

#[test]
fn numeric_ipv4_host_forms_are_normalized_before_classification() {
    for url in [
        "http://1681915905/",      // 100.64.0.1 as one decimal number
        "http://0x64.0x40.0.1/",   // hex octets
        "http://0144.0100.0.1/",   // octal octets
        "http://100.64.1/",        // short form
        "http://[::ffff:6440:1]/", // IPv4-mapped, hex groups
    ] {
        assert!(
            url_targets_internal_network(&url::Url::parse(url).unwrap()),
            "{url}"
        );
    }
}
