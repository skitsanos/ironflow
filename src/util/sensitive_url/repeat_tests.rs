//! A diagnostic is scrubbed more than once on its way out: by the node that
//! raised it, then by the engine. A second pass must change nothing, and one
//! pass must not take ordinary words after a credential (IF-219 a).

use super::*;

#[track_caller]
fn assert_stable(raw: &str, expected: &str) {
    let once = redact_sensitive_text(raw);
    assert_eq!(once, expected, "input: {raw:?}");
    assert_eq!(redact_sensitive_text(&once), once, "second pass: {raw:?}");
}

#[test]
fn a_second_pass_keeps_the_lines_after_a_credential() {
    assert_stable(
        "login failed\npassword: hunter2\nretrying connection to db\nattempt 2, giving up",
        "login failed password: [REDACTED] retrying connection to db attempt 2, giving up",
    );
    assert_stable(
        "X-API-Key: hunter6\r\nContent-Type: text/plain\r\nbody follows, 3 lines",
        "X-API-Key: [REDACTED]  Content-Type: text/plain  body follows, 3 lines",
    );
    assert_stable(
        "Authorization: Bearer hunter7\nupstream said no, twice",
        "Authorization: [REDACTED] upstream said no, twice",
    );
}

#[test]
fn a_second_pass_keeps_the_lines_after_an_unclosed_quote() {
    assert_stable(
        "password=\"hunter2\nnext line stays, \"quoted\" too",
        "password=\"[REDACTED] next line stays, \"quoted\" too",
    );
    assert_stable(
        "{\\\"token\\\":\\\"hunter3\nnext line stays",
        "{\\\"token\\\":\\\"[REDACTED] next line stays",
    );
}

#[test]
fn a_header_credential_on_one_line_ends_at_the_credential() {
    assert_stable(
        "X-API-Key: hunter6 while calling the billing service, attempt 2",
        "X-API-Key: [REDACTED] while calling the billing service, attempt 2",
    );
    assert_stable(
        "request failed with Authorization: Bearer hunter7 against the gateway, attempt 2",
        "request failed with Authorization: [REDACTED] against the gateway, attempt 2",
    );
    assert_stable(
        "Proxy-Authorization: basic   hunter8 was refused; retrying",
        "Proxy-Authorization: [REDACTED] was refused; retrying",
    );
    assert_stable(
        "token: Bearer hunter9 expired yesterday",
        "token: [REDACTED] expired yesterday",
    );
}

#[test]
fn an_authorization_value_is_its_scheme_and_credential_whatever_the_scheme() {
    assert_stable(
        "Authorization: Token hunter10 rejected by upstream",
        "Authorization: [REDACTED] rejected by upstream",
    );
    assert_stable(
        "authorization=ApiKey hunter11 retry=2",
        "authorization=[REDACTED] retry=2",
    );
    // A scheme alone, then a delimiter: nothing after it is taken.
    assert_stable(
        "Authorization: Negotiate, retry",
        "Authorization: [REDACTED], retry",
    );
}

#[test]
fn an_already_cut_diagnostic_is_not_marked_twice() {
    // Redaction shortens the kept prefix by six bytes, so the first marker
    // straddles the limit on the second pass.
    let filler = "x".repeat(MAX_DIAGNOSTIC_BYTES);
    let text = format!("password=hunter2-0123456789 {filler}");

    let once = redact_sensitive_text(&text);
    let twice = redact_sensitive_text(&once);

    assert!(once.ends_with(TRUNCATED));
    assert_eq!(once.matches(TRUNCATED).count(), 1);
    assert_eq!(twice, once);
}

/// Every shape the scrubber guarantees, scrubbed twice.
#[test]
fn a_second_pass_is_a_no_op_for_every_guaranteed_shape() {
    for raw in [
        "password=hunter1 credentials=hunter2 db_password=hunter3",
        "Authorization=Bearer hunter7 Proxy-Authorization: Basic hunter8 tail words, more",
        r#"{"client_secret": "hunter21", "grant": "x"}"#,
        r#"body: {\"password\":\"hunter15\",\"user\":\"alice\"}"#,
        "password = 'dsn-secret''tail-secret', authorization: Bearer another, next",
        "connect postgres://alice:hunter2@db.test/app?token=hunter3 failed, retry",
        "POST https://hooks.example.test/services/T0/B0/hunter4?sig=hunter5 returned 500",
        "Invalid Redis URL: redis://:hunter6@cache.test/0",
        "secret:\n  hunter7\nnext: value, other",
        "api_key=\nnext line, more",
        // A URL that does not parse becomes a marker, which is neither part
        // of the URL before it nor of a credential's value.
        "mirror=postgres://db.test/a,http://[hunter8 next, more",
        "Authorization: Bearer http://[hunter9 next, more",
    ] {
        let once = redact_sensitive_text(raw);
        assert!(!once.contains("hunter"), "{once}");
        assert_eq!(redact_sensitive_text(&once), once, "input: {raw:?}");
    }
}

/// Generated text of keys, separators, quotes, line breaks and URLs. Whatever
/// the first pass produced, the second must return it unchanged. A URL is
/// followed by whitespace or a quote, as in a diagnostic a tool prints: text
/// glued to the end of a URL is part of that URL and parses differently once
/// the URL has been rewritten.
#[test]
fn a_second_pass_is_a_no_op_for_generated_text() {
    const PIECES: &[&str] = &[
        "token",
        "password",
        "Authorization",
        "X-API-Key",
        "--secret",
        "=",
        ":",
        "=>",
        ":=",
        " ",
        "\"",
        "'",
        "\\",
        "[",
        "]",
        "<",
        ">",
        "(",
        ")",
        "Some(",
        "\n",
        "\t",
        ",",
        ";",
        "&",
        "Bearer",
        "basic",
        "word",
        "hunter2",
        "\u{e9}",
        "http://h.test/p?q=1 ",
        "postgres://u:p@h/d\n",
        "http://[bad \"",
        "[REDACTED]",
        "...[truncated]",
    ];
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move |bound: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize % bound
    };

    for _ in 0..20_000 {
        let text: String = (0..1 + next(16))
            .map(|_| PIECES[next(PIECES.len())])
            .collect();

        let once = redact_sensitive_text(&text);

        assert_eq!(redact_sensitive_text(&once), once, "input: {text:?}");
    }
}
