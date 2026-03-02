use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, Utc};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct DateFormatNode;

/// Try to parse a date string using common formats, returning a DateTime<FixedOffset>.
fn auto_parse(input: &str) -> Option<DateTime<FixedOffset>> {
    // RFC3339 (e.g. "2024-06-15T10:30:00Z" or "2024-06-15T10:30:00+02:00")
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Some(dt);
    }
    // RFC2822 (e.g. "Sat, 15 Jun 2024 10:30:00 +0000")
    if let Ok(dt) = DateTime::parse_from_rfc2822(input) {
        return Some(dt);
    }
    // Common datetime formats (naive — assume UTC)
    for fmt in &["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(input, fmt) {
            return Some(ndt.and_utc().fixed_offset());
        }
    }
    // Date only
    if let Ok(nd) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        let ndt = nd.and_hms_opt(0, 0, 0).unwrap();
        return Some(ndt.and_utc().fixed_offset());
    }
    None
}

/// Parse a timezone string like "+02:00", "-05:00", or "UTC" into a FixedOffset.
fn parse_timezone(tz: &str) -> Result<FixedOffset> {
    let trimmed = tz.trim();
    if trimmed.eq_ignore_ascii_case("utc") || trimmed == "Z" {
        return Ok(FixedOffset::east_opt(0).unwrap());
    }
    // Parse "+HH:MM" or "-HH:MM"
    if (trimmed.starts_with('+') || trimmed.starts_with('-')) && trimmed.len() >= 5 {
        let sign = if trimmed.starts_with('-') { -1 } else { 1 };
        let rest = &trimmed[1..];
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() == 2
            && let (Ok(h), Ok(m)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>())
        {
            let total_seconds = sign * (h * 3600 + m * 60);
            if let Some(offset) = FixedOffset::east_opt(total_seconds) {
                return Ok(offset);
            }
        }
    }
    anyhow::bail!(
        "Invalid timezone '{}'. Use 'UTC', '+HH:MM', or '-HH:MM'",
        tz
    )
}

#[async_trait]
impl Node for DateFormatNode {
    fn node_type(&self) -> &str {
        "date_format"
    }

    fn description(&self) -> &str {
        "Parse and format dates/timestamps"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
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
            interpolate_ctx(input_str, &ctx)
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
        let dt: DateTime<FixedOffset> = if raw_input.eq_ignore_ascii_case("now") {
            Utc::now().fixed_offset()
        } else if let Some(input_fmt) = config.get("input_format").and_then(|v| v.as_str()) {
            // Try with explicit format — first as naive, then assume UTC
            let ndt = NaiveDateTime::parse_from_str(&raw_input, input_fmt)
                .or_else(|_| {
                    NaiveDate::parse_from_str(&raw_input, input_fmt)
                        .map(|nd| nd.and_hms_opt(0, 0, 0).unwrap())
                })
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to parse '{}' with format '{}': {}",
                        raw_input,
                        input_fmt,
                        e
                    )
                })?;
            ndt.and_utc().fixed_offset()
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
            dt.with_timezone(&offset)
        } else {
            dt
        };

        let formatted = dt.format(output_format).to_string();
        let unix_ts = dt.timestamp();

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::json!(formatted));
        output.insert(format!("{}_unix", output_key), serde_json::json!(unix_ts));
        Ok(output)
    }
}
