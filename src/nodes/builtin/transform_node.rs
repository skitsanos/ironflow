use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use csv::{QuoteStyle, ReaderBuilder, Trim, WriterBuilder};

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct JsonParseNode;

#[async_trait]
impl Node for JsonParseNode {
    fn node_type(&self) -> &str {
        "json_parse"
    }

    fn description(&self) -> &str {
        "Parse a JSON string from context into a value"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_parse requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_parse requires 'output_key'"))?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let json_str = source
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not a string", source_key))?;

        let parsed: serde_json::Value = serde_json::from_str(json_str)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), parsed);
        Ok(output)
    }
}

pub struct JsonStringifyNode;

#[async_trait]
impl Node for JsonStringifyNode {
    fn node_type(&self) -> &str {
        "json_stringify"
    }

    fn description(&self) -> &str {
        "Serialize a context value to a JSON string"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_stringify requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_stringify requires 'output_key'"))?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let json_str = serde_json::to_string(source)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(json_str));
        Ok(output)
    }
}

pub struct JsonExtractPathNode;

#[async_trait]
impl Node for JsonExtractPathNode {
    fn node_type(&self) -> &str {
        "json_extract_path"
    }

    fn description(&self) -> &str {
        "Extract a value from JSON data using a dotted path with optional array indexes"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_extract_path requires 'source_key'"))?;

        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_extract_path requires 'path'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("json_extract_path requires 'output_key'"))?;

        let parse_json = config
            .get("parse_json")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let required = config
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let default_value = config.get("default").cloned();

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let source = if parse_json {
            if let Some(json_text) = source.as_str() {
                serde_json::from_str(json_text).map_err(|err| {
                    anyhow::anyhow!(
                        "json_extract_path failed to parse '{}' as JSON: {}",
                        source_key,
                        err
                    )
                })?
            } else {
                source.clone()
            }
        } else {
            source.clone()
        };

        let value = if path.trim().is_empty() {
            Some(source)
        } else {
            resolve_json_path(&source, path).cloned()
        };

        let output_value = match value {
            Some(v) => v,
            None if required => anyhow::bail!("Path '{}' was not found in '{}'", path, source_key),
            None => default_value.unwrap_or(serde_json::Value::Null),
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), output_value);
        Ok(output)
    }
}

fn resolve_json_path<'a>(
    value: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    let mut index = 0;
    let bytes = path.as_bytes();
    let len = bytes.len();

    while index < len {
        let segment_start = index;
        while index < len && bytes[index] != b'.' && bytes[index] != b'[' {
            index += 1;
        }

        if index > segment_start {
            let key = &path[segment_start..index];
            current = current.as_object()?.get(key)?;
        }

        if index < len && bytes[index] == b'[' {
            index += 1;

            let bracket_start = index;
            while index < len && bytes[index] != b']' {
                index += 1;
            }
            if index >= len {
                return None;
            }

            let index_text = path[bracket_start..index].trim();
            let array_index = index_text.parse::<usize>().ok()?;
            current = current.as_array()?.get(array_index)?;

            index += 1;
        }

        if index < len && bytes[index] == b'.' {
            index += 1;
        }
    }

    Some(current)
}

pub struct CsvParseNode;

#[async_trait]
impl Node for CsvParseNode {
    fn node_type(&self) -> &str {
        "csv_parse"
    }

    fn description(&self) -> &str {
        "Parse CSV text from context into structured JSON data"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
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

pub struct SelectFieldsNode;

#[async_trait]
impl Node for SelectFieldsNode {
    fn node_type(&self) -> &str {
        "select_fields"
    }

    fn description(&self) -> &str {
        "Select specific fields from a context object"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("select_fields requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("select_fields requires 'output_key'"))?;

        let fields = config
            .get("fields")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("select_fields requires 'fields' array"))?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let source_obj = source
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an object", source_key))?;

        let mut selected = serde_json::Map::new();
        for field in fields {
            if let Some(field_name) = field.as_str()
                && let Some(value) = source_obj.get(field_name)
            {
                selected.insert(field_name.to_string(), value.clone());
            }
        }

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Object(selected));
        Ok(output)
    }
}

pub struct RenameFieldsNode;

#[async_trait]
impl Node for RenameFieldsNode {
    fn node_type(&self) -> &str {
        "rename_fields"
    }

    fn description(&self) -> &str {
        "Rename fields in a context object"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("rename_fields requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("rename_fields requires 'output_key'"))?;

        let mapping = config
            .get("mapping")
            .and_then(|v| v.as_object())
            .ok_or_else(|| anyhow::anyhow!("rename_fields requires 'mapping' object"))?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let source_obj = source
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an object", source_key))?;

        let mut result = serde_json::Map::new();
        for (old_key, value) in source_obj {
            // Check if this key has a rename mapping
            if let Some(new_key_val) = mapping.get(old_key) {
                if let Some(new_key) = new_key_val.as_str() {
                    result.insert(new_key.to_string(), value.clone());
                } else {
                    result.insert(old_key.clone(), value.clone());
                }
            } else {
                result.insert(old_key.clone(), value.clone());
            }
        }

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Object(result));
        Ok(output)
    }
}

pub struct DataFilterNode;

#[async_trait]
impl Node for DataFilterNode {
    fn node_type(&self) -> &str {
        "data_filter"
    }

    fn description(&self) -> &str {
        "Filter array items by a condition"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_filter requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_filter requires 'output_key'"))?;

        let field = config
            .get("field")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_filter requires 'field'"))?;

        let op = config.get("op").and_then(|v| v.as_str()).ok_or_else(|| {
            anyhow::anyhow!(
                "data_filter requires 'op' (eq, neq, gt, lt, gte, lte, contains, exists)"
            )
        })?;

        let compare_value = config.get("value");

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let items = source
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))?;

        let filtered: Vec<serde_json::Value> = items
            .iter()
            .filter(|item| filter_match(item, field, op, compare_value))
            .cloned()
            .collect();

        let count = filtered.len();
        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(filtered));
        output.insert(format!("{}_count", output_key), serde_json::json!(count));
        Ok(output)
    }
}

/// Evaluate a filter condition on a single item.
fn filter_match(
    item: &serde_json::Value,
    field: &str,
    op: &str,
    compare_value: Option<&serde_json::Value>,
) -> bool {
    let field_val = item.get(field);

    match op {
        "exists" => field_val.is_some() && !field_val.unwrap().is_null(),
        "not_exists" => field_val.is_none() || field_val.unwrap().is_null(),
        _ => {
            let field_val = match field_val {
                Some(v) => v,
                None => return false,
            };
            let cmp = match compare_value {
                Some(v) => v,
                None => return false,
            };

            match op {
                "eq" => field_val == cmp,
                "neq" => field_val != cmp,
                "gt" => field_val
                    .as_f64()
                    .zip(cmp.as_f64())
                    .is_some_and(|(a, b)| a > b),
                "lt" => field_val
                    .as_f64()
                    .zip(cmp.as_f64())
                    .is_some_and(|(a, b)| a < b),
                "gte" => field_val
                    .as_f64()
                    .zip(cmp.as_f64())
                    .is_some_and(|(a, b)| a >= b),
                "lte" => field_val
                    .as_f64()
                    .zip(cmp.as_f64())
                    .is_some_and(|(a, b)| a <= b),
                "contains" => {
                    if let (Some(haystack), Some(needle)) = (field_val.as_str(), cmp.as_str()) {
                        haystack.contains(needle)
                    } else {
                        false
                    }
                }
                _ => false,
            }
        }
    }
}

pub struct DataTransformNode;

#[async_trait]
impl Node for DataTransformNode {
    fn node_type(&self) -> &str {
        "data_transform"
    }

    fn description(&self) -> &str {
        "Transform data by mapping and renaming fields"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_transform requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("data_transform requires 'output_key'"))?;

        let mapping = config
            .get("mapping")
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                anyhow::anyhow!("data_transform requires 'mapping' object (new_name -> old_name)")
            })?;

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let result = match source {
            serde_json::Value::Array(arr) => {
                // Apply mapping to each item in the array
                let transformed: Vec<serde_json::Value> = arr
                    .iter()
                    .map(|item| apply_mapping(item, mapping))
                    .collect();
                serde_json::Value::Array(transformed)
            }
            serde_json::Value::Object(_) => {
                // Apply mapping to a single object
                apply_mapping(source, mapping)
            }
            _ => anyhow::bail!("Value at '{}' must be an object or array", source_key),
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), result);
        Ok(output)
    }
}

/// Apply a field mapping to a single value. Mapping is { new_name: "old_name" }.
fn apply_mapping(
    item: &serde_json::Value,
    mapping: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    for (new_name, old_name_val) in mapping {
        if let Some(old_name) = old_name_val.as_str()
            && let Some(value) = item.get(old_name)
        {
            result.insert(new_name.clone(), value.clone());
        }
    }
    serde_json::Value::Object(result)
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

fn parse_csv_single_byte(config: &serde_json::Value, key: &str, default: u8) -> Result<u8> {
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

pub struct BatchNode;

#[async_trait]
impl Node for BatchNode {
    fn node_type(&self) -> &str {
        "batch"
    }

    fn description(&self) -> &str {
        "Split an array into chunks of a specified size"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("batch requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("batch requires 'output_key'"))?;

        let size = config
            .get("size")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("batch requires 'size' (positive integer)"))?
            as usize;

        if size == 0 {
            anyhow::bail!("batch 'size' must be greater than 0");
        }

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let items = source
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))?;

        let batches: Vec<serde_json::Value> = items
            .chunks(size)
            .map(|chunk| serde_json::Value::Array(chunk.to_vec()))
            .collect();

        let batch_count = batches.len();
        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(batches));
        output.insert(
            format!("{}_count", output_key),
            serde_json::json!(batch_count),
        );
        Ok(output)
    }
}

pub struct DeduplicateNode;

#[async_trait]
impl Node for DeduplicateNode {
    fn node_type(&self) -> &str {
        "deduplicate"
    }

    fn description(&self) -> &str {
        "Remove duplicate items from an array"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("deduplicate requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("deduplicate requires 'output_key'"))?;

        let key_field = config.get("key").and_then(|v| v.as_str());

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let items = source
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Value at '{}' is not an array", source_key))?;

        let mut seen = HashSet::new();
        let mut unique = Vec::new();

        for item in items {
            let dedup_key = match key_field {
                Some(field) => {
                    // Deduplicate by a specific field value
                    item.get(field).map(|v| v.to_string()).unwrap_or_default()
                }
                None => {
                    // Deduplicate by full JSON serialization
                    serde_json::to_string(item).unwrap_or_default()
                }
            };

            if seen.insert(dedup_key) {
                unique.push(item.clone());
            }
        }

        let original_count = items.len();
        let unique_count = unique.len();
        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(unique));
        output.insert(
            format!("{}_removed", output_key),
            serde_json::json!(original_count - unique_count),
        );
        Ok(output)
    }
}
