//! Tests for the date_format node.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(key: &str, val: serde_json::Value) -> Context {
    let mut ctx = HashMap::new();
    ctx.insert(key.to_string(), val);
    ctx
}

#[tokio::test]
async fn date_format_rfc3339_input() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").expect("date_format node exists");

    let config = serde_json::json!({
        "input": "2024-06-15T10:30:00Z",
        "output_format": "%Y-%m-%d",
        "output_key": "result"
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("result").unwrap(),
        &serde_json::json!("2024-06-15")
    );
    assert!(output.contains_key("result_unix"));
}

#[tokio::test]
async fn date_format_date_only_input() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "2024-06-15",
        "output_format": "%Y-%m-%d",
        "output_key": "result"
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("result").unwrap(),
        &serde_json::json!("2024-06-15")
    );
}

#[tokio::test]
async fn date_format_custom_input_format() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "15/06/2024 10:30:00",
        "input_format": "%d/%m/%Y %H:%M:%S",
        "output_format": "%Y-%m-%d",
        "output_key": "result"
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("result").unwrap(),
        &serde_json::json!("2024-06-15")
    );
}

#[tokio::test]
async fn date_format_now() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "now",
        "output_format": "%Y",
        "output_key": "year"
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    let year = output.get("year").unwrap().as_str().unwrap();
    let year_num: i32 = year.parse().unwrap();
    assert!(year_num >= 2024);
    assert!(output.contains_key("year_unix"));
}

#[tokio::test]
async fn date_format_custom_output_format() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "2024-06-15T10:30:00Z",
        "output_format": "%B %d, %Y",
        "output_key": "result"
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("result").unwrap(),
        &serde_json::json!("June 15, 2024")
    );
}

#[tokio::test]
async fn date_format_unix_output() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "2024-06-15T10:30:00Z",
        "output_key": "ts"
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    let unix = output.get("ts_unix").unwrap().as_i64().unwrap();
    assert_eq!(unix, 1718447400);
}

#[tokio::test]
async fn date_format_via_source_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let ctx = ctx_with("my_date", serde_json::json!("2024-06-15T10:30:00Z"));

    let config = serde_json::json!({
        "source_key": "my_date",
        "output_format": "%Y-%m-%d",
        "output_key": "result"
    });

    let output = node.execute(&config, &ctx).await.unwrap();
    assert_eq!(
        output.get("result").unwrap(),
        &serde_json::json!("2024-06-15")
    );
}

#[tokio::test]
async fn date_format_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "2024-06-15T10:30:00Z",
        "output_format": "%Y-%m-%d",
        "output_key": "my_custom_date"
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    assert!(output.contains_key("my_custom_date"));
    assert!(output.contains_key("my_custom_date_unix"));
}

#[tokio::test]
async fn date_format_invalid_date_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "not-a-date",
        "output_key": "result"
    });

    let result = node.execute(&config, &empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not-a-date"));
}

#[tokio::test]
async fn date_format_missing_input_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "output_format": "%Y-%m-%d"
    });

    let result = node.execute(&config, &empty_ctx()).await;
    assert!(result.is_err());
}

// --- IF-195: input offsets, output_format validation, on_error recovery ---

const INSTANT_0830Z: i64 = 1_718_440_200; // 2024-06-15T08:30:00Z
const INSTANT_1030Z: i64 = 1_718_447_400; // 2024-06-15T10:30:00Z

async fn run_date(config: serde_json::Value) -> anyhow::Result<(String, i64)> {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();
    let output = node.execute(&config, &empty_ctx()).await?;
    Ok((
        output["formatted_date"].as_str().unwrap().to_string(),
        output["formatted_date_unix"].as_i64().unwrap(),
    ))
}

#[tokio::test]
async fn date_format_input_format_offsets_yield_the_real_instant() {
    let cases = [
        ("2024-06-15 10:30:00 +0200", "%Y-%m-%d %H:%M:%S %z"),
        ("2024-06-15 10:30:00 +02:00", "%Y-%m-%d %H:%M:%S %:z"),
        ("2024-06-15 10:30:00+02", "%Y-%m-%d %H:%M:%S%#z"),
        ("2024-06-15T10:30:00+02:00", "%+"),
        ("2024-06-15 04:30:00 -0400", "%Y-%m-%d %H:%M:%S %z"),
        ("15/06/2024 10:30 +0200 CEST", "%d/%m/%Y %H:%M %z %Z"),
        ("2024-06-15 08:30:00 +0000", "%Y-%m-%d %H:%M:%S %z"),
    ];
    for (input, input_format) in cases {
        let (formatted, unix) = run_date(serde_json::json!({
            "input": input,
            "input_format": input_format,
            "output_format": "%H:%M:%SZ",
            "timezone": "UTC"
        }))
        .await
        .unwrap_or_else(|e| panic!("{input} / {input_format}: {e}"));
        assert_eq!(unix, INSTANT_0830Z, "{input} / {input_format}");
        assert_eq!(formatted, "08:30:00Z", "{input} / {input_format}");
    }
}

#[tokio::test]
async fn date_format_input_offset_is_kept_without_timezone() {
    // Same rule as an RFC 3339 input: the value keeps its own offset.
    for (input, input_format) in [
        ("2024-06-15 10:30:00 +0200", Some("%Y-%m-%d %H:%M:%S %z")),
        ("2024-06-15T10:30:00+02:00", None),
    ] {
        let mut config = serde_json::json!({
            "input": input,
            "output_format": "%Y-%m-%dT%H:%M:%S%:z"
        });
        if let Some(input_format) = input_format {
            config["input_format"] = serde_json::json!(input_format);
        }
        let (formatted, unix) = run_date(config).await.unwrap();
        assert_eq!(formatted, "2024-06-15T10:30:00+02:00", "{input}");
        assert_eq!(unix, INSTANT_0830Z, "{input}");
    }
}

#[tokio::test]
async fn date_format_date_with_offset_is_midnight_at_that_offset() {
    let (formatted, unix) = run_date(serde_json::json!({
        "input": "2024-06-15 +0200",
        "input_format": "%Y-%m-%d %z",
        "output_format": "%Y-%m-%d %H:%M",
        "timezone": "UTC"
    }))
    .await
    .unwrap();
    assert_eq!(formatted, "2024-06-14 22:00");
    assert_eq!(unix, 1_718_402_400);
}

#[tokio::test]
async fn date_format_zone_name_is_utc_or_rejected() {
    for name in ["UTC", "GMT", "utc", "Z", "UT"] {
        let (_, unix) = run_date(serde_json::json!({
            "input": format!("2024-06-15 10:30:00 {name}"),
            "input_format": "%Y-%m-%d %H:%M:%S %Z"
        }))
        .await
        .unwrap();
        assert_eq!(unix, INSTANT_1030Z, "{name}");
    }
    // An absent name stays UTC, as before.
    let (_, unix) = run_date(serde_json::json!({
        "input": "2024-06-15 10:30:00",
        "input_format": "%Y-%m-%d %H:%M:%S%Z"
    }))
    .await
    .unwrap();
    assert_eq!(unix, INSTANT_1030Z);

    for name in ["CEST", "PST", "Europe/Berlin", "+02:00"] {
        let error = run_date(serde_json::json!({
            "input": format!("2024-06-15 10:30:00 {name}"),
            "input_format": "%Y-%m-%d %H:%M:%S %Z"
        }))
        .await
        .expect_err(name)
        .to_string();
        assert!(error.contains("%Z") && error.contains(name), "{error}");
    }
}

#[tokio::test]
async fn date_format_naive_input_formats_are_unchanged() {
    let cases = [
        ("15/06/2024 10:30", "%d/%m/%Y %H:%M", INSTANT_1030Z),
        ("2024-06-15 10:30:00", "%Y-%m-%d %H:%M:%S", INSTANT_1030Z),
        ("15/06/2024", "%d/%m/%Y", 1_718_409_600),
        ("Jun 15 2024", "%b %d %Y", 1_718_409_600),
        ("1718447400", "%s", INSTANT_1030Z),
        (
            "2024-06-15 10:30:00.250",
            "%Y-%m-%d %H:%M:%S%.f",
            INSTANT_1030Z,
        ),
    ];
    for (input, input_format, expected) in cases {
        let (formatted, unix) = run_date(serde_json::json!({
            "input": input,
            "input_format": input_format,
            "output_format": "%Y-%m-%dT%H:%M:%S%:z"
        }))
        .await
        .unwrap_or_else(|e| panic!("{input}: {e}"));
        assert_eq!(unix, expected, "{input}");
        assert!(formatted.ends_with("+00:00"), "{input}: {formatted}");
    }
    for (input, input_format) in [
        ("10:30", "%H:%M"),
        ("2024-06-15", "%Y-%m-%d %H:%i"),
        ("2024-06-15", "%Y-%m-%d %"),
        ("2024-13-45", "%Y-%m-%d"),
    ] {
        let error = run_date(serde_json::json!({ "input": input, "input_format": input_format }))
            .await
            .expect_err(input_format)
            .to_string();
        assert!(error.starts_with("Failed to parse '"), "{error}");
    }
}

#[tokio::test]
async fn date_format_invalid_output_format_is_a_node_error() {
    for output_format in [
        "%Y-%m-%d %H:%i",
        "%Y-%m-%d %",
        "%",
        "%%%",
        "%😽",
        "%#m",
        "%.j",
        "%:j",
        "%.e",
        "%-Z",
        "%:Z",
        "%0Z",
        "%_Z",
        "%42",
        "% +",
        "%E",
        "%O",
        "%N",
        "%Q",
        "%4f",
        "%03d",
        // Parses as a valid item, but chrono can only parse with it.
        "%#z",
        "%.2f",
    ] {
        let error = run_date(serde_json::json!({
            "input": "2024-06-15T10:30:00Z",
            "output_format": output_format
        }))
        .await
        .expect_err(output_format)
        .to_string();
        assert!(error.contains("invalid output_format"), "{error}");
    }
}

#[tokio::test]
async fn date_format_valid_output_specifiers_are_unchanged() {
    let cases = [
        ("%-d/%-m/%y", "5/6/24"),
        ("%_d|%_m|%_H", " 5| 6| 9"),
        ("%0e", "05"),
        ("%c", "Wed Jun  5 09:07:03 2024"),
        ("%x %X %r", "06/05/24 09:07:03 09:07:03 AM"),
        (
            "%D %F %v %T %R",
            "06/05/24 2024-06-05  5-Jun-2024 09:07:03 09:07",
        ),
        ("%z %:z %::z %:::z %Z", "+0000 +00:00 +00:00:00 +00 +00:00"),
        ("%+", "2024-06-05T09:07:03.250+00:00"),
        ("%s %f %.f %.3f %3f", "1717578423 250000000 .250 .250 250"),
        (
            "%a %A %b %B %h %j %U %W %G %g %V %u %w %q %C",
            "Wed Wednesday Jun June Jun 157 22 23 2024 24 23 3 3 2 20",
        ),
        ("%I %l %p %P %k %e %n%t%%", "09  9 AM am  9  5 \n\t%"),
        ("plain text", "plain text"),
        ("", ""),
    ];
    for (output_format, expected) in cases {
        let (formatted, _) = run_date(serde_json::json!({
            "input": "2024-06-05T09:07:03.250Z",
            "output_format": output_format
        }))
        .await
        .unwrap_or_else(|e| panic!("{output_format}: {e}"));
        assert_eq!(formatted, expected, "{output_format}");
    }
}

#[tokio::test]
async fn date_format_timezone_out_of_range_is_a_node_error() {
    for timezone in ["+999999999:00", "-999999999:00", "+00:999999999", "+24:00"] {
        let error = run_date(serde_json::json!({
            "input": "2024-06-15T10:30:00Z",
            "timezone": timezone
        }))
        .await
        .expect_err(timezone)
        .to_string();
        assert!(error.contains("Invalid timezone"), "{error}");
    }
    let (formatted, unix) = run_date(serde_json::json!({
        "input": "2024-06-15T10:30:00Z",
        "timezone": "-05:30",
        "output_format": "%H:%M %:z"
    }))
    .await
    .unwrap();
    assert_eq!((formatted.as_str(), unix), ("05:00 -05:30", INSTANT_1030Z));
}

#[tokio::test]
async fn date_format_failures_reach_the_on_error_handler() {
    use ironflow::engine::executor::WorkflowEngine;
    use ironflow::engine::types::{RunStatus, TaskStatus};
    use ironflow::lua::runtime::LuaRuntime;
    use ironflow::storage::StateStore;
    use ironflow::storage::null_store::NullStateStore;
    use std::sync::Arc;

    let steps = [
        r#"input = "2024-06-15T10:30:00Z", output_format = "%Y-%m-%d %H:%i""#,
        r#"input = "2024-06-15T10:30:00Z", output_format = "%Y %""#,
        r#"input = "2024-06-15 10:30:00 CEST", input_format = "%Y-%m-%d %H:%M:%S %Z""#,
    ];
    for step in steps {
        let source = format!(
            r#"
            local flow = Flow.new("date_on_error")
            flow:step("format", nodes.date_format({{ {step} }})):on_error("recover")
            flow:step("recover", nodes.code({{
                source = "return {{ failed_step = ctx._error_step }}"
            }}))
            return flow
            "#
        );
        let reg = Arc::new(NodeRegistry::with_builtins());
        let flow = LuaRuntime::load_flow_from_string(&source, &reg).unwrap();
        let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
        let engine = WorkflowEngine::new(reg, store.clone(), None);
        let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
        let info = store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Success, "{step}");
        assert_eq!(info.tasks["format"].status, TaskStatus::Failed, "{step}");
        assert_eq!(info.ctx["failed_step"], "format", "{step}");
    }
}
