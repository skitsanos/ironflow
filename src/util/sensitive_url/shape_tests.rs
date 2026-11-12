//! Credential shapes the diagnostic scrubber guarantees, and ordinary text it
//! must leave alone (IF-177).

use super::*;

#[track_caller]
fn assert_scrubbed(raw: &str, expected: &str) {
    assert_eq!(redact_sensitive_text(raw), expected, "input: {raw}");
}

#[track_caller]
fn assert_untouched(raw: &str) {
    assert_eq!(redact_sensitive_text(raw), raw);
}

#[test]
fn reported_task_error_loses_every_credential() {
    let raw = concat!(
        "password=hunter1 credentials=hunter2 db_password=hunter3 ",
        "PGPASSWORD=hunter4 aws_secret_access_key=hunter5 X-API-Key: hunter6 ",
        "Authorization=Bearer hunter7 Proxy-Authorization: Basic hunter8"
    );

    let redacted = redact_sensitive_text(raw);

    assert!(!redacted.contains("hunter"), "{redacted}");
    assert!(redacted.starts_with("password=[REDACTED] credentials=[REDACTED] "));
}

#[test]
fn plural_and_compound_keys_are_matched_by_their_sensitive_suffix() {
    assert_scrubbed(
        "login failed: credentials=hunter2 for tenant acme",
        "login failed: credentials=[REDACTED] for tenant acme",
    );
    assert_scrubbed(
        "db_password=hunter3 host=db.internal",
        "db_password=[REDACTED] host=db.internal",
    );
    assert_scrubbed(
        "PGPASSWORD=hunter4 psql exited with 2",
        "PGPASSWORD=[REDACTED] psql exited with 2",
    );
    assert_scrubbed(
        "aws_secret_access_key=hunter5 region=eu-west-1",
        "aws_secret_access_key=[REDACTED] region=eu-west-1",
    );
    assert_scrubbed(
        "--password=hunter9 --verbose",
        "--password=[REDACTED] --verbose",
    );
    assert_scrubbed(
        "sessionToken=hunter10 clientSecret=hunter11 apiKey=hunter12 next",
        "sessionToken=[REDACTED] clientSecret=[REDACTED] apiKey=[REDACTED] next",
    );
}

#[test]
fn header_names_with_a_prefix_are_matched() {
    assert_scrubbed("X-API-Key: hunter6", "X-API-Key: [REDACTED]");
    assert_scrubbed(
        "Proxy-Authorization: Basic hunter8",
        "Proxy-Authorization: [REDACTED]",
    );
    assert_scrubbed(
        "x-hub-signature: sha256=hunter13, retry",
        "x-hub-signature: [REDACTED], retry",
    );
}

#[test]
fn scheme_and_token_are_one_value_after_an_equals_sign() {
    assert_scrubbed(
        "Authorization=Bearer hunter7 retry=2",
        "Authorization=[REDACTED] retry=2",
    );
    assert_scrubbed(
        "authorization=basic   hunter14 retry=2",
        "authorization=[REDACTED] retry=2",
    );
    // A scheme with nothing after it is still a value of its own.
    assert_scrubbed("authorization=Bearer", "authorization=[REDACTED]");
    // Another word is not a scheme, so the value ends at the whitespace.
    assert_scrubbed(
        "token=opaque trailing words",
        "token=[REDACTED] trailing words",
    );
}

#[test]
fn escaped_json_quotes_delimit_keys_and_values() {
    assert_scrubbed(
        r#"body: {\"password\":\"hunter15\",\"user\":\"alice\"}"#,
        r#"body: {\"password\":\"[REDACTED]\",\"user\":\"alice\"}"#,
    );
    assert_scrubbed(
        r#"{\"api_key\" : \"hunter16\", \"n\": 1}"#,
        r#"{\"api_key\" : \"[REDACTED]\", \"n\": 1}"#,
    );
    // An escaped quote inside the escaped value does not end it.
    assert_scrubbed(
        r#"{\"token\":\"hun\\\"ter17\",\"n\":1}"#,
        r#"{\"token\":\"[REDACTED]\",\"n\":1}"#,
    );
    // Two levels of escaping.
    assert_scrubbed(
        r#"{\\\"secret\\\":\\\"hunter18\\\",\\\"n\\\":1}"#,
        r#"{\\\"secret\\\":\\\"[REDACTED]\\\",\\\"n\\\":1}"#,
    );
    // An escaped value that never closes is redacted to the end.
    assert_scrubbed(
        r#"{\"password\":\"hunter19 and more"#,
        r#"{\"password\":\"[REDACTED]"#,
    );
}

#[test]
fn ordinary_words_and_non_secret_keys_are_left_alone() {
    for raw in [
        "token_type=bearer expires_in=3600",
        "usage: prompt_tokens=12 completion_tokens=40 max_tokens: 4096",
        "password_policy: strict; tokenizer=gpt2",
        "secretary: Alice, keyboard=us, monkey: banana",
        "author: Jane, authorized=true, authorization_url: none",
        "the password is wrong and the token expired",
        "passwords: 3 rejected, secrets: 2 rotated",
        "Basic usage: run Bearer checks",
        "primary_key: id, public_key: abc, sort_key=name",
        "temp_wd=/tmp/work cwd=/srv mode=basic auth",
        "token expired: retry later",
        "attempt to index a nil value (field 'token')",
        "password-reset: sent, secret_name=db-main",
    ] {
        assert_untouched(raw);
    }
}

#[test]
fn shapes_that_were_already_covered_keep_their_output() {
    assert_scrubbed(
        "password=hunter1 next=value",
        "password=[REDACTED] next=value",
    );
    assert_scrubbed(
        "authorization: Bearer hunter20, attempt 2",
        "authorization: [REDACTED], attempt 2",
    );
    assert_scrubbed(
        r#"{"client_secret": "hunter21", "grant": "x"}"#,
        r#"{"client_secret": "[REDACTED]", "grant": "x"}"#,
    );
    assert_scrubbed(
        "access_token=hunter22&refresh_token=hunter23&state=ok",
        "access_token=[REDACTED]&refresh_token=[REDACTED]&state=ok",
    );
}

#[test]
fn scrubbing_is_idempotent_for_every_shape() {
    for raw in [
        "Authorization=Bearer hunter7 retry=2",
        "X-API-Key: hunter6",
        r#"body: {\"password\":\"hunter15\",\"user\":\"alice\"}"#,
        "db_password=hunter3 host=db.internal",
    ] {
        let once = redact_sensitive_text(raw);
        assert_eq!(redact_sensitive_text(&once), once);
    }
}

/// Generated text mixes keys, separators, quotes, escapes, URLs and
/// multi-byte characters. Byte offsets drive the scan, so this pins that no
/// combination slices inside a character or leaves a control character.
#[test]
fn generated_text_never_panics_or_keeps_control_characters() {
    const PIECES: &[&str] = &[
        "token",
        "password",
        "api_key",
        "X-API-Key",
        "db_pwd",
        "=",
        ":",
        " ",
        "\"",
        "'",
        "\\",
        "\u{e9}",
        "\u{65e5}\u{672c}",
        "\u{1f642}",
        "\n",
        "\t",
        "\u{85}",
        ",",
        "&",
        ";",
        "<",
        ">",
        "Bearer",
        "basic",
        "http://",
        "postgres://u:p@h/d",
        "sqlite:",
        "Invalid Redis URL:",
        "a",
        "_",
        "-",
        "@",
        "/",
        "?",
        "#",
        "%zz",
        "[",
        "]",
    ];
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move |bound: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize % bound
    };

    for _ in 0..20_000 {
        let text: String = (0..1 + next(24))
            .map(|_| PIECES[next(PIECES.len())])
            .collect();

        let redacted = redact_sensitive_text(&text);

        assert!(!redacted.chars().any(char::is_control), "input: {text:?}");
    }
}
