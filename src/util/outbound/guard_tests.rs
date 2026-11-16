use std::net::SocketAddr;

use super::test_scope::with_policy;
use super::test_support::{ALLOWED, REFUSED};
use super::{Guard, Refusal, Target};

fn address(ip: &str) -> SocketAddr {
    SocketAddr::new(ip.parse().unwrap(), 0)
}

fn url(text: &str) -> url::Url {
    url::Url::parse(text).unwrap()
}

async fn process_guard(allowlist: &[&str]) -> Guard {
    with_policy(true, allowlist, &[], async {
        Guard::resolve(false).unwrap().unwrap()
    })
    .await
}

#[test]
fn flags_local_and_private_hosts() {
    let guard = Guard::node_only();
    for target in [
        "http://localhost/x",
        "http://LOCALHOST./x",
        "http://app.localhost/x",
        "http://127.0.0.1/x",
        "http://10.0.0.5/x",
        "http://192.168.1.1/x",
        "http://172.16.9.9/x",
        "http://169.254.169.254/latest/meta-data",
        "http://[::1]/x",
        "http://[fd00::1]/x",
        "http://[::ffff:127.0.0.1]/x",
        "http://[::ffff:10.0.0.5]/x",
        "http://[::ffff:169.254.169.254]/x",
    ] {
        assert_eq!(
            guard.check_url(&url(target)),
            Err(Refusal::PrivateAddress),
            "{target}"
        );
    }
}

#[test]
fn allows_public_hosts() {
    let guard = Guard::node_only();
    assert_eq!(
        guard.check_url(&url("http://example.com/x")),
        Ok(Target::NeedsResolution)
    );
    for target in [
        "https://8.8.8.8/x",
        "http://93.184.216.34/x",
        "http://[::ffff:8.8.8.8]/x",
    ] {
        assert_eq!(guard.check_url(&url(target)), Ok(Target::Permitted));
    }
}

#[test]
fn rejects_private_mixed_and_empty_dns_answers() {
    let guard = Guard::node_only();
    for addresses in [
        vec![address("127.0.0.1")],
        vec![address("93.184.216.34"), address("10.0.0.1")],
        vec![address("::ffff:169.254.169.254")],
    ] {
        assert_eq!(
            guard.validate_answer("host.example", addresses),
            Err(Refusal::PrivateDnsAnswer)
        );
    }
    assert_eq!(
        guard.validate_answer("host.example", Vec::new()),
        Err(Refusal::EmptyDnsAnswer)
    );
}

// IF-191: a hostname whose answer contains any address in a refused range is
// rejected as a whole, alone or mixed with a public address.
#[test]
fn rejects_dns_answers_in_every_refused_range() {
    let guard = Guard::node_only();
    let mut accepted = Vec::new();
    for (range, ip) in REFUSED {
        for addresses in [
            vec![address(ip)],
            vec![address("93.184.216.34"), address(ip)],
        ] {
            if guard.validate_answer("host.example", addresses).is_ok() {
                accepted.push((range, ip));
            }
        }
    }
    assert!(accepted.is_empty(), "DNS answer accepted: {accepted:#?}");
}

#[test]
fn preserves_public_dns_answers_adjacent_to_refused_ranges() {
    let guard = Guard::node_only();
    let expected: Vec<SocketAddr> = ALLOWED.iter().map(|ip| address(ip)).collect();
    for answer in &expected {
        assert!(guard.validate_answer("h.example", vec![*answer]).is_ok());
    }
    assert_eq!(
        guard
            .validate_answer("h.example", expected.clone())
            .unwrap(),
        expected
    );
}

// IF-212: the guard exists when the operator forces it or the node opts in,
// and a node cannot opt out of the process policy.
#[tokio::test]
async fn the_process_policy_is_one_way() {
    assert!(Guard::resolve(true).unwrap().is_some());
    with_policy(false, &[], &[], async {
        assert!(Guard::resolve(false).unwrap().is_none());
        let guard = Guard::resolve(true).unwrap().unwrap();
        assert!(guard.setting().starts_with("block_private_network"));
    })
    .await;
    with_policy(true, &[], &[], async {
        for node_opt_in in [false, true] {
            let guard = Guard::resolve(node_opt_in).unwrap().unwrap();
            assert!(guard.setting().contains("IRONFLOW_BLOCK_PRIVATE_NETWORK"));
        }
    })
    .await;
}

#[tokio::test]
async fn allowlisted_literals_pass_the_first_stage() {
    let guard = process_guard(&["127.0.0.1:11434", "10.1.0.0/16", "[fd00::7]"]).await;
    for (target, expected) in [
        ("http://127.0.0.1:11434/v1", Ok(Target::Permitted)),
        ("http://127.0.0.1:11435/v1", Err(Refusal::PrivateAddress)),
        ("http://127.0.0.2:11434/v1", Err(Refusal::PrivateAddress)),
        ("https://10.1.2.3/", Ok(Target::Permitted)),
        ("https://[::ffff:10.1.2.3]/", Ok(Target::Permitted)),
        ("https://10.2.2.3/", Err(Refusal::PrivateAddress)),
        ("http://[fd00::7]:9000/", Ok(Target::Permitted)),
        ("http://[fd00::8]:9000/", Err(Refusal::PrivateAddress)),
        ("http://169.254.169.254/", Err(Refusal::PrivateAddress)),
    ] {
        assert_eq!(guard.check_url(&url(target)), expected, "{target}");
    }
}

// A host-name entry admits that requested name and nothing else: it is not a
// licence for the address behind it.
#[tokio::test]
async fn a_name_entry_does_not_unpin_other_hosts() {
    let guard = process_guard(&["ollama.internal:11434", "localhost:8080"]).await;
    assert_eq!(
        guard.check_url(&url("http://ollama.internal:11434/api")),
        Ok(Target::Permitted)
    );
    assert_eq!(
        guard.check_url(&url("http://localhost:8080/")),
        Ok(Target::Permitted)
    );
    assert_eq!(
        guard.check_url(&url("http://ollama.internal/api")),
        Err(Refusal::OtherPortsOnly)
    );
    assert_eq!(
        guard.check_url(&url("http://localhost:8081/")),
        Err(Refusal::OtherPortsOnly)
    );
    let internal = vec![address("10.0.0.5")];
    assert_eq!(
        guard.validate_answer("ollama.internal", internal.clone()),
        Ok(internal.clone())
    );
    for other in [
        "evil.example",
        "x.ollama.internal",
        "ollama.internal.evil.example",
    ] {
        assert_eq!(
            guard.check_url(&url(&format!("http://{other}:11434/"))),
            Ok(Target::NeedsResolution)
        );
        assert_eq!(
            guard.validate_answer(other, internal.clone()),
            Err(Refusal::PrivateDnsAnswer),
            "{other}"
        );
    }
}

#[tokio::test]
async fn range_entries_admit_dns_answers_but_port_bound_entries_do_not() {
    let guard = process_guard(&["10.1.0.0/16", "127.0.0.1:11434"]).await;
    let inside = vec![address("10.1.2.3"), address("93.184.216.34")];
    assert_eq!(
        guard.validate_answer("a.example", inside.clone()),
        Ok(inside)
    );
    for refused in ["10.2.0.1", "127.0.0.1"] {
        assert_eq!(
            guard.validate_answer("a.example", vec![address(refused)]),
            Err(Refusal::PrivateDnsAnswer)
        );
    }
}
