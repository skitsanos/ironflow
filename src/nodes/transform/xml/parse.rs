use anyhow::Result;
use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct XmlParseNode;

#[async_trait]
impl Node for XmlParseNode {
    fn node_type(&self) -> &str {
        "xml_parse"
    }

    fn description(&self) -> &str {
        "Parse XML string into a JSON object"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("xml_data");
        let input = get_input(config, ctx)?;
        let parsed = parse_xml_to_json(&input)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), parsed);
        Ok(output)
    }
}

fn get_input(config: &serde_json::Value, ctx: &Context) -> Result<String> {
    let input = config.get("input").and_then(|v| v.as_str());
    let source_key = config.get("source_key").and_then(|v| v.as_str());
    match (input, source_key) {
        (Some(_), Some(_)) => {
            anyhow::bail!("xml_parse accepts either 'input' or 'source_key', not both")
        }
        (Some(input), None) => Ok(interpolate_ctx(input, ctx)),
        (None, Some(source_key)) => {
            let value = ctx
                .get(source_key)
                .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
            match value {
                serde_json::Value::String(value) => Ok(value.clone()),
                value => Ok(serde_json::to_string(value)?),
            }
        }
        (None, None) => {
            anyhow::bail!("xml_parse requires either 'input' string or 'source_key'")
        }
    }
}

fn parse_xml_to_json(xml: &str) -> Result<serde_json::Value> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<(String, serde_json::Map<String, serde_json::Value>)> = Vec::new();
    let mut root = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                stack.push((element_name(&element), attributes(&element)));
            }
            Ok(Event::Empty(element)) => {
                let name = element_name(&element);
                let map = attributes(&element);
                let value = if map.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::Object(map)
                };
                add_element(&mut stack, &mut root, name, value);
            }
            Ok(Event::Text(text)) => {
                let text = text
                    .xml_content(XmlVersion::Implicit1_0)
                    .map_err(|error| anyhow::anyhow!("XML text decode error: {}", error))?
                    .to_string();
                if !text.is_empty()
                    && let Some((_, map)) = stack.last_mut()
                {
                    map.insert("#text".to_string(), serde_json::Value::String(text));
                }
            }
            Ok(Event::End(_)) => {
                if let Some((name, map)) = stack.pop() {
                    add_element(&mut stack, &mut root, name, simplify_element(map));
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => anyhow::bail!("XML parse error: {}", error),
            _ => {}
        }
    }

    let Some((name, value)) = root else {
        anyhow::bail!("Empty or invalid XML document");
    };
    Ok(serde_json::Value::Object(
        [(name, value)].into_iter().collect(),
    ))
}

fn element_name(element: &quick_xml::events::BytesStart<'_>) -> String {
    String::from_utf8_lossy(element.name().as_ref()).to_string()
}

fn attributes(
    element: &quick_xml::events::BytesStart<'_>,
) -> serde_json::Map<String, serde_json::Value> {
    element
        .attributes()
        .flatten()
        .map(|attribute| {
            (
                format!("@{}", String::from_utf8_lossy(attribute.key.as_ref())),
                serde_json::Value::String(String::from_utf8_lossy(&attribute.value).to_string()),
            )
        })
        .collect()
}

fn add_element(
    stack: &mut [(String, serde_json::Map<String, serde_json::Value>)],
    root: &mut Option<(String, serde_json::Value)>,
    name: String,
    value: serde_json::Value,
) {
    if let Some((_, parent)) = stack.last_mut() {
        insert_child(parent, &name, value);
    } else {
        *root = Some((name, value));
    }
}

fn simplify_element(map: serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    if map.len() == 1
        && let Some(text) = map.get("#text")
    {
        return text.clone();
    }
    if map.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::Object(map)
    }
}

fn insert_child(
    parent: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) {
    if let Some(existing) = parent.get_mut(key) {
        if let serde_json::Value::Array(values) = existing {
            values.push(value);
        } else {
            *existing = serde_json::Value::Array(vec![existing.clone(), value]);
        }
    } else {
        parent.insert(key.to_string(), value);
    }
}
