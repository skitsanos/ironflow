use anyhow::Result;
use async_trait::async_trait;
use quick_xml::Reader;
use quick_xml::events::Event;

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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("xml_data");

        let input = get_input(config, &ctx, "xml_parse")?;

        let parsed = parse_xml_to_json(&input)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), parsed);
        Ok(output)
    }
}

pub struct XmlStringifyNode;

#[async_trait]
impl Node for XmlStringifyNode {
    fn node_type(&self) -> &str {
        "xml_stringify"
    }

    fn description(&self) -> &str {
        "Convert a JSON value to an XML string"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("xml_stringify requires 'source_key'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("xml");

        let root_tag = config
            .get("root_tag")
            .and_then(|v| v.as_str())
            .unwrap_or("root");

        let pretty = config
            .get("pretty")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let xml = json_to_xml(source, root_tag, pretty)?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(xml));
        Ok(output)
    }
}

/// Get input text from either `input` (literal string with interpolation)
/// or `source_key` (context key reference).
fn get_input(config: &serde_json::Value, ctx: &Context, node_name: &str) -> Result<String> {
    let has_input = config.get("input").and_then(|v| v.as_str()).is_some();
    let has_source_key = config.get("source_key").and_then(|v| v.as_str()).is_some();

    if has_input && has_source_key {
        anyhow::bail!(
            "{} accepts either 'input' or 'source_key', not both",
            node_name
        );
    }

    if let Some(input_str) = config.get("input").and_then(|v| v.as_str()) {
        Ok(interpolate_ctx(input_str, ctx))
    } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
        let val = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
        match val {
            serde_json::Value::String(s) => Ok(s.clone()),
            other => Ok(serde_json::to_string(other)?),
        }
    } else {
        anyhow::bail!(
            "{} requires either 'input' string or 'source_key'",
            node_name
        )
    }
}

/// Parse an XML string into a serde_json::Value tree.
/// Elements become objects, attributes are prefixed with `@`, text content uses `#text`.
fn parse_xml_to_json(xml: &str) -> Result<serde_json::Value> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<(String, serde_json::Map<String, serde_json::Value>)> = Vec::new();
    let mut root: Option<(String, serde_json::Value)> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let mut map = serde_json::Map::new();

                // Add attributes with @ prefix
                for attr in e.attributes().flatten() {
                    let attr_name = format!("@{}", String::from_utf8_lossy(attr.key.as_ref()));
                    let attr_value = String::from_utf8_lossy(&attr.value).to_string();
                    map.insert(attr_name, serde_json::Value::String(attr_value));
                }

                stack.push((name, map));
            }
            Ok(Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let mut map = serde_json::Map::new();

                for attr in e.attributes().flatten() {
                    let attr_name = format!("@{}", String::from_utf8_lossy(attr.key.as_ref()));
                    let attr_value = String::from_utf8_lossy(&attr.value).to_string();
                    map.insert(attr_name, serde_json::Value::String(attr_value));
                }

                let child_value = if map.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::Object(map)
                };

                if let Some((_, parent_map)) = stack.last_mut() {
                    insert_child(parent_map, &name, child_value);
                } else {
                    root = Some((name, child_value));
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e
                    .xml_content()
                    .map_err(|err| anyhow::anyhow!("XML text decode error: {}", err))?
                    .to_string();
                if !text.is_empty()
                    && let Some((_, map)) = stack.last_mut()
                {
                    map.insert("#text".to_string(), serde_json::Value::String(text));
                }
            }
            Ok(Event::End(_)) => {
                if let Some((name, map)) = stack.pop() {
                    // If the map has only #text, simplify to just the string value
                    let value = simplify_element(map);

                    if let Some((_, parent_map)) = stack.last_mut() {
                        insert_child(parent_map, &name, value);
                    } else {
                        root = Some((name, value));
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => anyhow::bail!("XML parse error: {}", e),
            _ => {}
        }
    }

    match root {
        Some((name, value)) => {
            let mut obj = serde_json::Map::new();
            obj.insert(name, value);
            Ok(serde_json::Value::Object(obj))
        }
        None => anyhow::bail!("Empty or invalid XML document"),
    }
}

/// If an element map contains only `#text`, return just the string value.
fn simplify_element(map: serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    if map.len() == 1
        && let Some(text) = map.get("#text")
    {
        return text.clone();
    }
    if map.is_empty() {
        return serde_json::Value::Null;
    }
    serde_json::Value::Object(map)
}

/// Insert a child element into a parent map, converting to array if duplicate keys exist.
fn insert_child(
    parent: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) {
    if let Some(existing) = parent.get_mut(key) {
        // Convert to array if not already
        if let serde_json::Value::Array(arr) = existing {
            arr.push(value);
        } else {
            let prev = existing.clone();
            *existing = serde_json::Value::Array(vec![prev, value]);
        }
    } else {
        parent.insert(key.to_string(), value);
    }
}

/// Convert a JSON value to an XML string.
fn json_to_xml(value: &serde_json::Value, root_tag: &str, pretty: bool) -> Result<String> {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    if pretty {
        xml.push('\n');
    }
    write_element(&mut xml, root_tag, value, pretty, 0);
    Ok(xml)
}

fn write_element(
    xml: &mut String,
    tag: &str,
    value: &serde_json::Value,
    pretty: bool,
    depth: usize,
) {
    let indent = if pretty {
        "  ".repeat(depth)
    } else {
        String::new()
    };
    let newline = if pretty { "\n" } else { "" };

    match value {
        serde_json::Value::Object(map) => {
            // Separate attributes from child elements
            let mut attrs = String::new();
            let mut children: Vec<(&str, &serde_json::Value)> = Vec::new();
            let mut text_content: Option<&str> = None;

            for (k, v) in map {
                if let Some(attr_name) = k.strip_prefix('@') {
                    if let serde_json::Value::String(s) = v {
                        attrs.push_str(&format!(" {}=\"{}\"", attr_name, escape_xml(s)));
                    }
                } else if k == "#text" {
                    if let serde_json::Value::String(s) = v {
                        text_content = Some(s);
                    }
                } else {
                    children.push((k, v));
                }
            }

            if children.is_empty() && text_content.is_none() {
                xml.push_str(&format!("{}<{}{}/>", indent, tag, attrs));
                xml.push_str(newline);
            } else if children.is_empty() {
                // Text-only element
                xml.push_str(&format!(
                    "{}<{}{}>{}</{}>",
                    indent,
                    tag,
                    attrs,
                    escape_xml(text_content.unwrap_or("")),
                    tag
                ));
                xml.push_str(newline);
            } else {
                xml.push_str(&format!("{}<{}{}>", indent, tag, attrs));
                xml.push_str(newline);
                if let Some(text) = text_content {
                    xml.push_str(&format!(
                        "{}{}",
                        if pretty {
                            "  ".repeat(depth + 1)
                        } else {
                            String::new()
                        },
                        escape_xml(text)
                    ));
                    xml.push_str(newline);
                }
                for (child_tag, child_value) in &children {
                    write_element(xml, child_tag, child_value, pretty, depth + 1);
                }
                xml.push_str(&format!("{}</{}>", indent, tag));
                xml.push_str(newline);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                write_element(xml, tag, item, pretty, depth);
            }
        }
        serde_json::Value::String(s) => {
            xml.push_str(&format!("{}<{}>{}</{}>", indent, tag, escape_xml(s), tag));
            xml.push_str(newline);
        }
        serde_json::Value::Number(n) => {
            xml.push_str(&format!("{}<{}>{}</{}>", indent, tag, n, tag));
            xml.push_str(newline);
        }
        serde_json::Value::Bool(b) => {
            xml.push_str(&format!("{}<{}>{}</{}>", indent, tag, b, tag));
            xml.push_str(newline);
        }
        serde_json::Value::Null => {
            xml.push_str(&format!("{}<{}/>", indent, tag));
            xml.push_str(newline);
        }
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
