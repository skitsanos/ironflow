use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use csv::{QuoteStyle, WriterBuilder};

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::node_config::config_bool;

use super::value::{csv_value_to_string, parse_csv_single_byte};

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
        let include_headers = config_bool(config, "include_headers", ctx).unwrap_or(true);
        let quote_all = config_bool(config, "quote_all", ctx).unwrap_or(false);
        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let mut writer = WriterBuilder::new()
            .delimiter(parse_csv_single_byte(config, "delimiter", b',')?)
            .quote(parse_csv_single_byte(config, "quote_char", b'"')?)
            .quote_style(if quote_all {
                QuoteStyle::Always
            } else {
                QuoteStyle::Necessary
            })
            .from_writer(Vec::new());

        match source {
            serde_json::Value::Array(values) => {
                write_array(&mut writer, values, include_headers)?;
            }
            serde_json::Value::Object(object) => {
                write_object(&mut writer, object, include_headers)?;
            }
            _ => {
                anyhow::bail!("csv_stringify requires 'source_key' to contain an object or array");
            }
        }

        let csv_text = String::from_utf8(writer.into_inner().map_err(|error| {
            anyhow::anyhow!("csv_stringify failed to finalize buffer: {}", error)
        })?)?;
        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(csv_text));
        Ok(output)
    }
}

fn write_array(
    writer: &mut csv::Writer<Vec<u8>>,
    values: &[serde_json::Value],
    include_headers: bool,
) -> Result<()> {
    match detect_source_mode(values)? {
        CsvSourceMode::Objects => write_object_rows(writer, values, include_headers),
        CsvSourceMode::Arrays => write_array_rows(writer, values, include_headers),
        CsvSourceMode::Scalars => {
            if include_headers {
                writer.write_record(["value"])?;
            }
            for value in values {
                writer.write_record([csv_value_to_string(value)])?;
            }
            Ok(())
        }
    }
}

fn write_object_rows(
    writer: &mut csv::Writer<Vec<u8>>,
    values: &[serde_json::Value],
    include_headers: bool,
) -> Result<()> {
    let mut headers = Vec::new();
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for value in values {
        let object = value.as_object().ok_or_else(|| {
            anyhow::anyhow!(
                "csv_stringify expects array elements to be objects when source is an array of objects"
            )
        })?;
        for field in object.keys() {
            if seen.insert(field.clone()) {
                headers.push(field.clone());
            }
        }
        rows.push(object);
    }
    headers.sort_unstable();
    if include_headers {
        writer.write_record(&headers)?;
    }
    for row in rows {
        writer.write_record(headers.iter().map(|field| {
            csv_value_to_string(row.get(field).unwrap_or(&serde_json::Value::Null))
        }))?;
    }
    Ok(())
}

fn write_array_rows(
    writer: &mut csv::Writer<Vec<u8>>,
    values: &[serde_json::Value],
    include_headers: bool,
) -> Result<()> {
    let width = values
        .iter()
        .map(|value| value.as_array().map_or(0, Vec::len))
        .max()
        .unwrap_or(0);
    if include_headers {
        writer.write_record((1..=width).map(|index| format!("column_{index}")))?;
    }
    for value in values {
        let row = value.as_array().ok_or_else(|| {
            anyhow::anyhow!(
                "csv_stringify expects array elements to be arrays when source is an array mode"
            )
        })?;
        writer.write_record((0..width).map(|index| {
            csv_value_to_string(row.get(index).unwrap_or(&serde_json::Value::Null))
        }))?;
    }
    Ok(())
}

fn write_object(
    writer: &mut csv::Writer<Vec<u8>>,
    object: &serde_json::Map<String, serde_json::Value>,
    include_headers: bool,
) -> Result<()> {
    let mut headers: Vec<_> = object.keys().cloned().collect();
    headers.sort_unstable();
    if include_headers {
        writer.write_record(&headers)?;
    }
    writer.write_record(
        headers.iter().map(|field| {
            csv_value_to_string(object.get(field).unwrap_or(&serde_json::Value::Null))
        }),
    )?;
    Ok(())
}

enum CsvSourceMode {
    Objects,
    Arrays,
    Scalars,
}

fn detect_source_mode(values: &[serde_json::Value]) -> Result<CsvSourceMode> {
    let Some(first) = values.first() else {
        return Ok(CsvSourceMode::Scalars);
    };
    let mode = match first {
        serde_json::Value::Object(_) => CsvSourceMode::Objects,
        serde_json::Value::Array(_) => CsvSourceMode::Arrays,
        _ => CsvSourceMode::Scalars,
    };
    let homogeneous = values.iter().all(|value| match (&mode, value) {
        (CsvSourceMode::Objects, serde_json::Value::Object(_))
        | (CsvSourceMode::Arrays, serde_json::Value::Array(_)) => true,
        (CsvSourceMode::Scalars, value) => !value.is_object() && !value.is_array(),
        _ => false,
    });
    if !homogeneous {
        anyhow::bail!(
            "csv_stringify array must contain only objects, only arrays, or only scalar values"
        );
    }
    Ok(mode)
}
