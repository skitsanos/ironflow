use anyhow::Result;
use aws_sdk_s3vectors::types::{DataType, DistanceMetric};
use aws_smithy_types::Document;
use aws_smithy_types::Number;

pub(super) fn parse_json_to_document(value: &serde_json::Value) -> Result<Document> {
    match value {
        serde_json::Value::Null => Ok(Document::Null),
        serde_json::Value::Bool(value) => Ok(Document::from(*value)),
        serde_json::Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Ok(Document::from(value))
            } else if let Some(value) = number.as_u64() {
                Ok(Document::from(value))
            } else if let Some(value) = number.as_f64() {
                if !value.is_finite() {
                    anyhow::bail!("JSON to Document conversion requires finite numbers only");
                }
                Ok(Document::from(value))
            } else {
                Err(anyhow::anyhow!("unsupported JSON number format"))
            }
        }
        serde_json::Value::String(value) => Ok(Document::from(value.clone())),
        serde_json::Value::Array(values) => {
            let mut items = Vec::with_capacity(values.len());
            for value in values {
                items.push(parse_json_to_document(value)?);
            }
            Ok(Document::from(items))
        }
        serde_json::Value::Object(entries) => {
            let mut object = std::collections::HashMap::new();
            for (key, value) in entries {
                object.insert(key.clone(), parse_json_to_document(value)?);
            }
            Ok(Document::from(object))
        }
    }
}

pub(super) fn parse_metadata(value: &serde_json::Value, _node: &str) -> Result<Document> {
    let doc = parse_json_to_document(value)?;
    match &doc {
        Document::Object(_)
        | Document::Array(_)
        | Document::Number(_)
        | Document::String(_)
        | Document::Bool(_)
        | Document::Null => Ok(doc),
    }
}

pub(super) fn document_to_json(value: &Document) -> serde_json::Value {
    match value {
        Document::Null => serde_json::Value::Null,
        Document::Bool(v) => serde_json::Value::Bool(*v),
        Document::String(v) => serde_json::Value::String(v.clone()),
        Document::Number(number) => match number {
            Number::PosInt(number) => serde_json::Number::from(*number).into(),
            Number::NegInt(number) => serde_json::Number::from(*number).into(),
            Number::Float(number) => serde_json::Number::from_f64(*number)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
        },
        Document::Object(entries) => {
            let mut map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            for (key, value) in entries {
                map.insert(key.clone(), document_to_json(value));
            }
            serde_json::Value::Object(map)
        }
        Document::Array(values) => {
            let items = values.iter().map(document_to_json).collect();
            serde_json::Value::Array(items)
        }
    }
}

pub(super) fn parse_data_type(value: &serde_json::Value, node: &str) -> Result<DataType> {
    let value = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("{} requires 'data_type' as string", node))?;
    DataType::try_parse(value)
        .map_err(|_| anyhow::anyhow!("{} received unsupported data_type '{}'", node, value))
}

pub(super) fn parse_distance_metric(
    value: &serde_json::Value,
    node: &str,
) -> Result<DistanceMetric> {
    let value = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("{} requires 'distance_metric' as string", node))?;
    DistanceMetric::try_parse(value)
        .map_err(|_| anyhow::anyhow!("{} received unsupported distance_metric '{}'", node, value))
}
