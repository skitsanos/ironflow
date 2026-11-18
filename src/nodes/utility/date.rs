use anyhow::Result;
use async_trait::async_trait;
use jiff::tz::Offset;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::date_input::{auto_parse, format_output, parse_timezone, parse_with_format};
use super::strftime::Moment;

pub struct DateFormatNode;

/// The current time, read at UTC.
fn now() -> Moment {
    Moment {
        local: Offset::UTC.to_datetime(jiff::Timestamp::now()),
        offset: 0,
        leap: false,
    }
}

#[async_trait]
impl Node for DateFormatNode {
    fn node_type(&self) -> &str {
        "date_format"
    }

    fn description(&self) -> &str {
        "Parse and format dates/timestamps"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_format = config
            .get("output_format")
            .and_then(|v| v.as_str())
            .unwrap_or("%Y-%m-%d %H:%M:%S");

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("formatted_date");

        let timezone = config.get("timezone").and_then(|v| v.as_str());

        // Resolve input value
        let input_has = config.get("input").and_then(|v| v.as_str()).is_some();
        let source_has = config.get("source_key").and_then(|v| v.as_str()).is_some();

        if input_has && source_has {
            anyhow::bail!("date_format: provide either 'input' or 'source_key', not both");
        }

        let raw_input = if let Some(input_str) = config.get("input").and_then(|v| v.as_str()) {
            interpolate_ctx(input_str, ctx)
        } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
            let val = ctx
                .get(source_key)
                .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
            match val {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string(other)?,
            }
        } else {
            anyhow::bail!("date_format requires either 'input' or 'source_key'");
        };

        // Parse the date
        let dt = if raw_input.eq_ignore_ascii_case("now") {
            now()
        } else if let Some(input_fmt) = config.get("input_format").and_then(|v| v.as_str()) {
            parse_with_format(&raw_input, input_fmt)?
        } else {
            auto_parse(&raw_input).ok_or_else(|| {
                anyhow::anyhow!(
                    "Could not parse '{}' as a date. Supported formats: RFC3339, RFC2822, \
                     YYYY-MM-DD HH:MM:SS, YYYY-MM-DDTHH:MM:SS, YYYY-MM-DD",
                    raw_input
                )
            })?
        };

        // Apply timezone conversion if requested
        let dt = if let Some(tz_str) = timezone {
            let offset = parse_timezone(tz_str)?;
            dt.at_offset(offset).ok_or_else(|| {
                anyhow::anyhow!(
                    "date_format: the date at timezone '{tz_str}' is outside the years -9999 to 9999"
                )
            })?
        } else {
            dt
        };

        let formatted = format_output(&dt, output_format)?;
        let unix_ts = dt.unix_seconds();

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::json!(formatted));
        output.insert(format!("{}_unix", output_key), serde_json::json!(unix_ts));
        Ok(output)
    }
}
