use anyhow::Result;
use async_trait::async_trait;
use csv::{ReaderBuilder, Trim};

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::node_config::config_bool;

use super::value::{csv_value_from_str, parse_csv_single_byte};

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
        let has_header = config_bool(config, "has_header", ctx).unwrap_or(true);
        let trim_fields = config_bool(config, "trim", ctx).unwrap_or(false);
        let skip_empty_lines = config_bool(config, "skip_empty_lines", ctx).unwrap_or(true);
        let infer_types = config_bool(config, "infer_types", ctx).unwrap_or(false);

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        let csv_text = source
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not a string", source_key))?;
        let mut reader = ReaderBuilder::new()
            .delimiter(parse_csv_single_byte(config, "delimiter", b',')?)
            .quote(parse_csv_single_byte(config, "quote_char", b'"')?)
            .has_headers(has_header)
            .trim(if trim_fields { Trim::All } else { Trim::None })
            .from_reader(csv_text.as_bytes());

        let rows = if has_header {
            parse_rows_with_headers(&mut reader, skip_empty_lines, infer_types)?
        } else {
            parse_array_rows(&mut reader, skip_empty_lines, infer_types)?
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(rows));
        Ok(output)
    }
}

fn parse_rows_with_headers(
    reader: &mut csv::Reader<&[u8]>,
    skip_empty_lines: bool,
    infer_types: bool,
) -> Result<Vec<serde_json::Value>> {
    let headers: Vec<String> = reader
        .headers()
        .map(|headers| headers.iter().map(str::to_string).collect())?;
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        if skip_empty_lines && record.iter().all(str::is_empty) {
            continue;
        }
        let mut row = serde_json::Map::new();
        for (index, value) in record.iter().enumerate() {
            let key = headers
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("column_{}", index + 1));
            row.insert(key, csv_value_from_str(value, infer_types));
        }
        for index in headers.len()..record.len() {
            row.insert(
                format!("column_{}", index + 1),
                csv_value_from_str(record.get(index).unwrap_or_default(), infer_types),
            );
        }
        rows.push(serde_json::Value::Object(row));
    }
    Ok(rows)
}

fn parse_array_rows(
    reader: &mut csv::Reader<&[u8]>,
    skip_empty_lines: bool,
    infer_types: bool,
) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        if skip_empty_lines && record.iter().all(str::is_empty) {
            continue;
        }
        rows.push(serde_json::Value::Array(
            record
                .iter()
                .map(|value| csv_value_from_str(value, infer_types))
                .collect(),
        ));
    }
    Ok(rows)
}
