//! IF-212: destinations given as a host and port (SMTP relays, databases).

use serde_json::json;

use super::node_checks::{LOOPBACK, run};
use super::test_listener::Listener;
use super::test_scope::with_policy;
use super::{Guard, Refusal, Target};

#[tokio::test]
async fn a_host_given_as_text_is_classified_like_a_url_host() {
    with_policy(true, &["10.1.0.0/16", "[fd00::7]"], &[], async {
        let guard = Guard::resolve(false).unwrap().unwrap();
        assert_eq!(guard.check_target("[fd00::7]", 25), Ok(Target::Permitted));
        assert_eq!(guard.check_target("fd00::7", 25), Ok(Target::Permitted));
        assert_eq!(guard.check_target("10.1.9.9", 5432), Ok(Target::Permitted));
        for refused in ["10.9.9.9", "127.0.0.1", "[::1]", "localhost"] {
            assert_eq!(
                guard.check_target(refused, 5432),
                Err(Refusal::PrivateAddress),
                "{refused}"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn resolve_target_uses_the_scoped_dns_answer() {
    let hosts: &[(&str, &[&str])] = &[
        ("internal.test", &["10.0.0.5"]),
        ("listed.test", &["10.0.0.6"]),
        ("public.test", &["93.184.216.34"]),
    ];
    with_policy(true, &["listed.test:25"], hosts, async {
        let guard = Guard::resolve(false).unwrap().unwrap();
        let error = guard.resolve_target("internal.test", 25).await.unwrap_err();
        assert!(error.to_string().contains("private network"), "{error}");
        assert_eq!(
            guard.resolve_target("listed.test", 25).await.unwrap(),
            vec!["10.0.0.6:25".parse().unwrap()]
        );
        assert!(guard.resolve_target("listed.test", 26).await.is_err());
        assert_eq!(
            guard.resolve_target("public.test", 443).await.unwrap(),
            vec!["93.184.216.34:443".parse().unwrap()]
        );
        assert!(guard.resolve_target("127.0.0.1", 25).await.is_err());
        assert!(guard.resolve_target("localhost", 25).await.is_err());
    })
    .await;
}

// SMTP connects to the address the guard validated, not to a second lookup:
// `listed.test` resolves only in the scoped table, so a connection proves it.
#[tokio::test]
async fn smtp_is_pinned_to_the_validated_relay_address() {
    let listener = Listener::ok();
    let smtp = |server: &str| {
        json!({"provider": "smtp", "smtp_server": server, "smtp_port": listener.port,
            "smtp_tls": "none", "from": "a@example.com", "to": "b@example.com",
            "subject": "s", "text": "t", "timeout": 3})
    };
    for server in ["internal.test", "mixed.test", "127.0.0.1", "localhost"] {
        let error = with_policy(true, &[], LOOPBACK, run("send_email", smtp(server)))
            .await
            .unwrap_err();
        assert!(error.contains("private network"), "{server}: {error}");
    }
    assert_eq!(listener.hits(), 0);

    let entry = format!("listed.test:{}", listener.port);
    let error = with_policy(
        true,
        &[&entry],
        LOOPBACK,
        run("send_email", smtp("listed.test")),
    )
    .await
    .unwrap_err();
    assert!(!error.contains("private network"), "{error}");
    assert_eq!(listener.hits(), 1);
}

// `db_query` and `db_exec` get a host check only; the driver resolves the
// name again when it connects.
#[tokio::test]
async fn database_hosts_are_checked_before_the_driver_connects() {
    let listener = Listener::ok();
    for node in ["db_query", "db_exec"] {
        for connection in [
            format!("postgres://u:made-up@internal.test:{}/db", listener.port),
            format!("postgres://u:made-up@127.0.0.1:{}/db", listener.port),
            format!(
                "postgres://u:made-up@public.example/db?host=127.0.0.1&port={}",
                listener.port
            ),
            "postgres://u:made-up@public.example/db?hostaddr=169.254.169.254".to_owned(),
        ] {
            let config = json!({"connection": connection, "query": "SELECT 1"});
            let hosts: &[(&str, &[&str])] = &[
                ("internal.test", &["127.0.0.1"]),
                ("public.example", &["93.184.216.34"]),
            ];
            let error = with_policy(true, &[], hosts, run(node, config))
                .await
                .unwrap_err();
            assert!(error.contains("private network"), "{node}: {error}");
            assert!(!error.contains("made-up"), "{error}");
        }
    }
    assert_eq!(listener.hits(), 0);

    // A local SQLite file names no network host and is unaffected.
    let config = json!({"connection": "sqlite::memory:", "query": "SELECT 1"});
    with_policy(true, &[], &[], run("db_query", config))
        .await
        .unwrap();
}
