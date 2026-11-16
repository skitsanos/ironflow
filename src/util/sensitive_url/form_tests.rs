//! Key-value forms that tools print besides `key=value` and `key: value`
//! (IF-219 b): Lua bracket keys, `=>`, `:=`, command-line flags, wrapped
//! values and XML elements.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::*;

#[track_caller]
fn assert_scrubbed(raw: &str, expected: &str) {
    let once = redact_sensitive_text(raw);
    assert_eq!(once, expected, "input: {raw}");
    assert_eq!(redact_sensitive_text(&once), once, "second pass: {raw}");
}

#[test]
fn a_lua_bracket_key_is_matched() {
    assert_scrubbed(
        r#"config = { ["password"] = "hunter1", ['api_key'] = 'hunter2', n = 1 }"#,
        r#"config = { ["password"] = "[REDACTED]", ['api_key'] = '[REDACTED]', n = 1 }"#,
    );
    assert_scrubbed(
        "headers[authorization] = Bearer hunter3 sent",
        "headers[authorization] = [REDACTED] sent",
    );
    assert_scrubbed(
        r#"{[\"token\"] = \"hunter4\"}"#,
        r#"{[\"token\"] = \"[REDACTED]\"}"#,
    );
}

#[test]
fn a_hash_rocket_is_a_separator() {
    assert_scrubbed(
        r#"{"password" => "hunter5", :secret => 'hunter6', token => hunter7, n => 1}"#,
        r#"{"password" => "[REDACTED]", :secret => '[REDACTED]', token => [REDACTED], n => 1}"#,
    );
    assert_scrubbed("api_key=>hunter8;", "api_key=>[REDACTED];");
}

#[test]
fn a_walrus_assignment_is_a_separator() {
    assert_scrubbed(
        r#"password := "hunter9"; token := hunter10 done"#,
        r#"password := "[REDACTED]"; token := [REDACTED] done"#,
    );
    assert_scrubbed("DB_PASSWORD:=hunter11", "DB_PASSWORD:=[REDACTED]");
}

#[test]
fn a_quoted_json_key_may_have_spaces_around_its_colon() {
    assert_scrubbed(
        r#"{ "password" : "hunter12" , "api_key"  :  hunter13 , "n" : 1 }"#,
        r#"{ "password" : "[REDACTED]" , "api_key"  :  [REDACTED] , "n" : 1 }"#,
    );
}

#[test]
fn a_command_line_flag_takes_the_next_argument() {
    assert_scrubbed(
        "command failed: mysql --password hunter14 -u root --api-key 'hunter 15' db",
        "command failed: mysql --password [REDACTED] -u root --api-key '[REDACTED]' db",
    );
    // The next argument is another flag, not a value.
    assert_scrubbed("tool --token --verbose", "tool --token --verbose");
    assert_scrubbed("tool --secret", "tool --secret");
}

#[test]
fn a_wrapped_value_is_redacted_inside_its_wrapper() {
    assert_scrubbed(
        r#"Config { password: Some("hunter16"), token: Some(hunter17), n: 1 }"#,
        r#"Config { password: Some("[REDACTED]"), token: [REDACTED], n: 1 }"#,
    );
    assert_scrubbed(
        "api_key=SecretString('hunter18') host=db",
        "api_key=SecretString('[REDACTED]') host=db",
    );
}

#[test]
fn an_xml_element_is_matched() {
    assert_scrubbed(
        "<login><user>alice</user><password>hunter 19</password></login>",
        "<login><user>alice</user><password>[REDACTED]</password></login>",
    );
    assert_scrubbed(
        "<ApiKey>hunter20</ApiKey>\nnext line",
        "<ApiKey>[REDACTED]</ApiKey> next line",
    );
    // Pretty-printed: the text is on the line after its tag.
    assert_scrubbed(
        "<secret>\n  hunter21\n</secret>",
        "<secret>   [REDACTED] </secret>",
    );
}

#[test]
fn text_that_only_resembles_those_forms_is_left_alone() {
    for raw in [
        "[password] section is missing",
        "rows[token] and rows[secret] are both nil",
        "password >= 8 characters, token <= 64",
        "<secret/> and </password> without a value",
        "token -> next, secret | filter",
        "tokens => 3, passwords := 2",
    ] {
        assert_eq!(redact_sensitive_text(raw), raw);
    }
}

/// The new forms must not cost more than one more read of the text: a
/// mebibyte of each, without the length cap, finishes well inside a budget
/// that is generous for an unoptimized build under load.
#[test]
fn a_mebibyte_of_each_new_form_is_scanned_in_linear_time() {
    const MEBIBYTE: usize = 1024 * 1024;
    const BUDGET: Duration = Duration::from_secs(120);

    for unit in [
        "[\"password\"] ",
        "[\"password\"]",
        "token => ",
        "token=>",
        "secret:=",
        "--secret ",
        "--secret \"",
        "password: Some(",
        "password=Some(\"",
        "<pwd>",
        "<token>x",
        "token\t\t\t\t",
        "password=[REDACTED] ",
        "Authorization: [REDACTED]\n",
        "api_key='[REDACTED] ",
    ] {
        let text = unit.repeat(MEBIBYTE / unit.len() + 1);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(scrub(&text, false));
        });

        let redacted = receiver
            .recv_timeout(BUDGET)
            .unwrap_or_else(|_| panic!("unit {unit:?} did not finish within {BUDGET:?}"));

        assert!(!redacted.is_empty(), "unit: {unit}");
    }
}
