use super::*;

fn assert_absent(text: &str, secrets: &[&str]) {
    for secret in secrets {
        assert!(
            !text.contains(secret),
            "redacted text still contains {secret:?}: {text}"
        );
    }
}

#[test]
fn connection_hides_userinfo_query_values_and_fragment() {
    let raw = "postgres://alice:s3cr3t@db.internal/app?sslmode=require&password=query-secret#fragment-secret";
    let redacted = redact_connection_url(raw);

    assert_absent(
        &redacted,
        &[
            "alice",
            "s3cr3t",
            "require",
            "query-secret",
            "fragment-secret",
        ],
    );
    assert!(redacted.contains("db.internal/app"));
    assert!(redacted.contains("sslmode="));
    assert!(redacted.contains("password="));

    assert_eq!(
        redact_connection_url("redis://:password-only@cache.internal/0"),
        "redis://cache.internal/0"
    );
}

#[test]
fn encoded_credentials_and_query_values_are_not_reemitted() {
    let raw = "redis://%61lice:p%40ss%3Aword@[::1]:6379/0?access%5Ftoken=t%2Bok%40en";
    let redacted = redact_connection_url(raw);

    assert_absent(
        &redacted,
        &[
            "%61lice",
            "alice",
            "p%40ss%3Aword",
            "p@ss:word",
            "t%2Bok%40en",
            "t+ok@en",
        ],
    );
    assert!(redacted.contains("[::1]:6379"));
    assert!(redacted.contains("access_token="));
}

#[test]
fn secret_endpoint_hides_path_userinfo_query_and_fragment() {
    let raw = "https://bot:pass@hooks.slack.com/services/T/B/path-secret?token=query-secret#fragment-secret";
    let redacted = redact_secret_endpoint(raw);

    assert_absent(
        &redacted,
        &[
            "bot",
            "pass",
            "/services/T/B/path-secret",
            "query-secret",
            "fragment-secret",
        ],
    );
    assert!(redacted.starts_with("https://hooks.slack.com/"));
}

#[test]
fn malformed_schemeless_and_control_urls_fail_closed() {
    for raw in [
        "postgres://user:secret@@host/%zz",
        "user:secret@host/database?token=query-secret",
        "redis://user:secret@host\r\nforged=true",
    ] {
        assert_eq!(redact_connection_url(raw), REDACTED_URL);
    }
}

#[test]
fn redaction_is_idempotent_for_connection_and_secret_endpoint() {
    let connection = redact_connection_url(
        "postgres://user:pass@[2001:db8::1]/db?password=secret&sslmode=require#fragment",
    );
    assert_eq!(redact_connection_url(&connection), connection);

    let endpoint = redact_secret_endpoint(
        "https://user:pass@example.test/hooks/secret?signature=value#fragment",
    );
    assert_eq!(redact_secret_endpoint(&endpoint), endpoint);
}

#[test]
fn display_and_debug_never_expose_raw_values() {
    let raw = "postgres://user:password@db.test/app?token=query-secret";
    let display = Connection::new(raw).to_string();
    let debug = format!("{:?}", Connection::new(raw));

    assert_eq!(display, debug);
    assert_absent(&display, &["user", "password", "query-secret"]);

    let endpoint = "https://user:password@example.test/hooks/path-secret?token=query-secret";
    let display = SecretEndpoint::new(endpoint).to_string();
    let debug = format!("{:?}", SecretEndpoint::new(endpoint));

    assert_eq!(display, debug);
    assert_absent(
        &display,
        &["user", "password", "path-secret", "query-secret"],
    );
}

#[test]
fn text_fallback_redacts_urls_assignments_quotes_whitespace_and_controls() {
    let raw = concat!(
        "connect postgres://alice:p%40ss@db.test/app?token=query-secret failed; ",
        "password = 'dsn-secret''tail-secret', authorization: Bearer another-secret, ",
        "\"client_secret\": \"json-\\\"quoted-secret\"\r\nnext"
    );
    let redacted = redact_sensitive_text(raw);

    assert_absent(
        &redacted,
        &[
            "alice",
            "p%40ss",
            "query-secret",
            "dsn-secret",
            "tail-secret",
            "another-secret",
            "quoted-secret",
            "\r",
            "\n",
        ],
    );
    assert!(redacted.contains("password = '[REDACTED]'"));
    assert!(redacted.contains("authorization: [REDACTED]"));
}

#[test]
fn known_constructor_messages_hide_malformed_raw_urls() {
    let redacted = redact_sensitive_text(
        "Invalid Redis URL: user:secret@host without a scheme: parser details",
    );

    assert_eq!(redacted, "Invalid Redis URL: [REDACTED URL]");
    assert_absent(&redacted, &["user", "secret", "host", "parser details"]);
}
