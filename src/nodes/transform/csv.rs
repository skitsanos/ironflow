use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use csv::{QuoteStyle, ReaderBuilder, Trim, WriterBuilder};

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct CsvParseNode;

#[async_trait]
impl Node for CsvParseNode {
    fn node_type(&self) -> &str {
        "csv_parse"
    }

    fn description(&self) -> &str {
        "Parse CSV text from context into structured JSON data"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("csv_parse requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("csv_parse requires 'output_key'"))?;

        let has_header = config
            .get("has_header")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let trim_fields = config
            .get("trim")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let skip_empty_lines = config
            .get("skip_empty_lines")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let infer_types = config
            .get("infer_types")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let delimiter = parse_csv_single_byte(config, "delimiter", b',')?;
        let quote = parse_csv_single_byte(config, "quote_char", b'"')?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let csv_text = source
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not a string", source_key))?;

        let mut reader = ReaderBuilder::new()
            .delimiter(delimiter)
            .quote(quote)
            .has_headers(has_header)
            .trim(if trim_fields { Trim::All } else { Trim::None })
            .from_reader(csv_text.as_bytes());

        let mut rows = Vec::new();

        if has_header {
            let headers: Vec<String> = reader
                .headers()
                .map(|headers| headers.iter().map(|h| h.to_string()).collect())?;

            for record in reader.records() {
                let record = record?;
                if skip_empty_lines && record.iter().all(|field| field.is_empty()) {
                    continue;
                }

                let mut row = serde_json::Map::new();
                for (idx, value) in record.iter().enumerate() {
                    let key = headers
                        .get(idx)
                        .cloned()
                        .unwrap_or_else(|| format!("column_{}", idx + 1));
                    row.insert(key, csv_value_from_str(value, infer_types));
                }
                for idx in headers.len()..record.len() {
                    let key = format!("column_{}", idx + 1);
                    row.insert(
                        key,
                        csv_value_from_str(record.get(idx).unwrap_or_default(), infer_types),
                    );
                }
                rows.push(serde_json::Value::Object(row));
            }
        } else {
            for record in reader.records() {
                let record = record?;
                if skip_empty_lines && record.iter().all(|field| field.is_empty()) {
                    continue;
                }

                let row: Vec<serde_json::Value> = record
                    .iter()
                    .map(|value| csv_value_from_str(value, infer_types))
                    .collect();
                rows.push(serde_json::Value::Array(row));
            }
        }

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(rows));
        Ok(output)
    }
}

pub struct CsvStringifyNode;

#[async_trait]
impl Node for CsvStringifyNode {
    fn node_type(&self) -> &str {
        "csv_stringify"
    }

    fn description(&self) -> &str {
        "Serialize JSON data to CSV text"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("csv_stringify requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("csv_stringify requires 'output_key'"))?;

        let delimiter = parse_csv_single_byte(config, "delimiter", b',')?;
        let quote = parse_csv_single_byte(config, "quote_char", b'"')?;
        let include_headers = config
            .get("include_headers")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let quote_all = config
            .get("quote_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let mut csv = WriterBuilder::new()
            .delimiter(delimiter)
            .quote(quote)
            .quote_style(if quote_all {
                QuoteStyle::Always
            } else {
                QuoteStyle::Necessary
            })
            .from_writer(Vec::new());

        match source {
            serde_json::Value::Array(values) => {
                let mode = detect_csv_source_mode(values)?;
                match mode {
                    CsvSourceMode::Objects => {
                        let mut headers: Vec<String> = Vec::new();
                        let mut seen = HashSet::new();
                        let mut rows = Vec::new();

                        for value in values {
                            let object = value
                                .as_object()
                                .ok_or_else(|| anyhow::anyhow!(
                                    "csv_stringify expects array elements to be objects when source is an array of objects"
                                ))?;
                            for field in object.keys() {
                                if seen.insert(field.clone()) {
                                    headers.push(field.clone());
                                }
                            }
                            rows.push(object.clone());
                        }
                        headers.sort_unstable();

                        if include_headers {
                            csv.write_record(&headers)?;
                        }
                        for row in rows {
                            let fields: Vec<String> = headers
                                .iter()
                                .map(|field| {
                                    csv_value_to_string(
                                        row.get(field).unwrap_or(&serde_json::Value::Null),
                                    )
                                })
                                .collect();
                            csv.write_record(fields)?;
                        }
                    }
                    CsvSourceMode::Arrays => {
                        let max_len = values
                            .iter()
                            .map(|v| v.as_array().map_or(0, |arr| arr.len()))
                            .max()
                            .unwrap_or(0);
                        if include_headers {
                            let header: Vec<String> =
                                (1..=max_len).map(|idx| format!("column_{idx}")).collect();
                            csv.write_record(header)?;
                        }
                        for row in values {
                            let arr = row.as_array().ok_or_else(|| {
                                anyhow::anyhow!("csv_stringify expects array elements to be arrays when source is an array mode")
                            })?;
                            let fields: Vec<String> = (0..max_len)
                                .map(|idx| {
                                    csv_value_to_string(
                                        arr.get(idx).unwrap_or(&serde_json::Value::Null),
                                    )
                                })
                                .collect();
                            csv.write_record(fields)?;
                        }
                    }
                    CsvSourceMode::Scalars => {
                        if include_headers {
                            csv.write_record(["value"])?;
                        }
                        for row in values {
                            csv.write_record([csv_value_to_string(row)])?;
                        }
                    }
                }
            }
            serde_json::Value::Object(object) => {
                let mut headers: Vec<String> = object.keys().cloned().collect();
                headers.sort_unstable();
                if include_headers {
                    csv.write_record(&headers)?;
                }
                let fields: Vec<String> = headers
                    .iter()
                    .map(|field| {
                        csv_value_to_string(object.get(field).unwrap_or(&serde_json::Value::Null))
                    })
                    .collect();
                csv.write_record(fields)?;
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "csv_stringify requires 'source_key' to contain an object or array"
                ));
            }
        }

        let csv_text =
            String::from_utf8(csv.into_inner().map_err(|err| {
                anyhow::anyhow!("csv_stringify failed to finalize buffer: {}", err)
            })?)?;
        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(csv_text));
        Ok(output)
    }
}

#[derive(Debug)]
enum CsvSourceMode {
    Objects,
    Arrays,
    Scalars,
}

fn detect_csv_source_mode(values: &[serde_json::Value]) -> Result<CsvSourceMode> {
    if values.is_empty() {
        return Ok(CsvSourceMode::Scalars);
    }

    let mode = match &values[0] {
        serde_json::Value::Object(_) => CsvSourceMode::Objects,
        serde_json::Value::Array(_) => CsvSourceMode::Arrays,
        _ => CsvSourceMode::Scalars,
    };

    for value in values {
        match (&mode, value) {
            (CsvSourceMode::Objects, serde_json::Value::Object(_)) => {}
            (CsvSourceMode::Arrays, serde_json::Value::Array(_)) => {}
            (CsvSourceMode::Scalars, v) if !v.is_object() && !v.is_array() => {}
            _ => {
                return Err(anyhow::anyhow!(
                    "csv_stringify array must contain only objects, only arrays, or only scalar values"
                ));
            }
        }
    }

    Ok(mode)
}

pub(super) fn parse_csv_single_byte(
    config: &serde_json::Value,
    key: &str,
    default: u8,
) -> Result<u8> {
    let Some(value) = config.get(key).and_then(|value| value.as_str()) else {
        return Ok(default);
    };

    if value == "\\t" {
        return Ok(b'\t');
    }
    if value == "\\n" {
        return Ok(b'\n');
    }
    if value == "\\r" {
        return Ok(b'\r');
    }

    let bytes = value.as_bytes();
    if bytes.len() != 1 {
        anyhow::bail!("{} must be a single-byte character", key);
    }
    Ok(bytes[0])
}

fn csv_value_from_str(value: &str, infer_types: bool) -> serde_json::Value {
    if !infer_types {
        return serde_json::Value::String(value.to_string());
    }

    let trimmed = value.trim();

    if trimmed.eq_ignore_ascii_case("true") {
        return serde_json::Value::Bool(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return serde_json::Value::Bool(false);
    }
    if trimmed.is_empty() {
        return serde_json::Value::String(String::new());
    }
    if let Ok(int_value) = trimmed.parse::<i64>() {
        return serde_json::json!(int_value);
    }
    if let Ok(float_value) = trimmed.parse::<f64>() {
        return serde_json::json!(float_value);
    }
    serde_json::Value::String(trimmed.to_string())
}

fn csv_value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(v) => v.to_string(),
        serde_json::Value::Number(v) => v.to_string(),
        serde_json::Value::String(v) => v.clone(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| String::new()),
    }
}
