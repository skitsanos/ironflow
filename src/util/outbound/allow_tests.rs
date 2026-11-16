use std::net::IpAddr;

use super::{AllowList, MAX_ENTRIES, NameMatch};

fn list(entries: &[&str]) -> AllowList {
    AllowList::parse(entries.iter().copied()).unwrap()
}

fn ip(address: &str) -> IpAddr {
    address.parse().unwrap()
}

#[test]
fn malformed_entries_fail_and_name_only_the_position() {
    for entry in [
        "",
        " ",
        "http://ollama.internal:11434",
        "*.internal",
        "ollama.internal:0",
        "ollama.internal:+80",
        "ollama.internal:65536",
        "ollama.internal:",
        "ollama..internal",
        "-bad.internal",
        "bad host",
        "10.0.0.5/8",
        "10.0.0.0/33",
        "10.0.0.0/x",
        "fd00::/129",
        "[fd00::1",
        "[fd00::1]x",
        "[10.0.0.1]",
        "10.0.0.256",
        "1.2.3",
        "fd00::1:99999",
    ] {
        let error = AllowList::parse(["db.internal", entry])
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("allowlist entry 2:"),
            "{entry:?}: {error}"
        );
        if !entry.trim().is_empty() {
            assert!(!error.contains(entry), "{entry:?} echoed in: {error}");
        }
    }
}

#[test]
fn the_entry_count_is_bounded() {
    let entries = vec!["db.internal"; MAX_ENTRIES + 1];
    let error = AllowList::parse(entries).unwrap_err().to_string();
    assert!(error.contains("more than"), "{error}");
    assert_eq!(list(&vec!["db.internal"; MAX_ENTRIES]).len(), MAX_ENTRIES);
}

#[test]
fn a_name_entry_matches_only_the_exact_requested_host() {
    let allow = list(&["Ollama.Internal.", "db.internal:5432", "db.internal:5433"]);
    assert_eq!(
        allow.match_name("ollama.internal", 11434),
        NameMatch::Allowed
    );
    assert_eq!(allow.match_name("OLLAMA.internal.", 80), NameMatch::Allowed);
    for other in [
        "x.ollama.internal",
        "ollama.internal.example.com",
        "ollama",
        "internal",
        "ollama-internal",
    ] {
        assert_eq!(allow.match_name(other, 80), NameMatch::Unlisted, "{other}");
        assert!(!allow.names_host(other), "{other}");
    }
    assert_eq!(allow.match_name("db.internal", 5432), NameMatch::Allowed);
    assert_eq!(allow.match_name("db.internal", 5433), NameMatch::Allowed);
    assert_eq!(
        allow.match_name("db.internal", 80),
        NameMatch::OtherPortsOnly
    );
    assert!(allow.names_host("db.internal"));
    // A name entry never matches an address.
    assert!(!allow.allows_literal(ip("127.0.0.1"), 5432));
    assert!(!allow.allows_resolved(ip("127.0.0.1")));
}

#[test]
fn address_and_range_entries_match_by_address() {
    let allow = list(&[
        "10.1.0.0/16",
        "192.168.7.9",
        "127.0.0.1:11434",
        "fd00:1::/32",
        "[::1]:8080",
        "fe80::5",
        "0.0.0.0/0",
    ]);
    let allow_narrow = list(&["10.1.0.0/16", "192.168.7.9", "fd00:1::/32", "fe80::5"]);
    assert_eq!(allow.len(), 7);
    for (address, expected) in [
        ("10.1.0.0", true),
        ("10.1.255.255", true),
        ("10.2.0.0", false),
        ("10.0.255.255", false),
        ("192.168.7.9", true),
        ("192.168.7.8", false),
        ("::ffff:10.1.2.3", true),
        ("::ffff:10.2.2.3", false),
        ("fd00:1::9", true),
        ("fd00:2::9", false),
        ("fe80::5", true),
        ("fe80::6", false),
    ] {
        assert_eq!(
            allow_narrow.allows_resolved(ip(address)),
            expected,
            "{address}"
        );
        assert_eq!(
            allow_narrow.allows_literal(ip(address), 443),
            expected,
            "{address}"
        );
    }
    // The zero-length prefix admits everything of its family.
    assert!(allow.allows_resolved(ip("8.8.8.8")));
    assert!(!list(&["0.0.0.0/0"]).allows_resolved(ip("fd00::1")));
    assert!(list(&["::/0"]).allows_resolved(ip("fd00::1")));
}

#[test]
fn a_port_on_an_address_entry_applies_to_literal_destinations_only() {
    let allow = list(&["127.0.0.1:11434", "[::1]:8080"]);
    assert!(allow.allows_literal(ip("127.0.0.1"), 11434));
    assert!(!allow.allows_literal(ip("127.0.0.1"), 11435));
    assert!(!allow.allows_literal(ip("127.0.0.2"), 11434));
    assert!(allow.allows_literal(ip("::1"), 8080));
    assert!(!allow.allows_literal(ip("::1"), 8081));
    // A DNS answer carries no port, so a port-bound entry never admits one.
    assert!(!allow.allows_resolved(ip("127.0.0.1")));
    assert!(!allow.allows_resolved(ip("::1")));
}
