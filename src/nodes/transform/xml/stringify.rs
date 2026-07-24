use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::node_config::config_bool;

pub struct XmlStringifyNode;

#[async_trait]
impl Node for XmlStringifyNode {
    fn node_type(&self) -> &str {
        "xml_stringify"
    }

    fn description(&self) -> &str {
        "Convert a JSON value to an XML string"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
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
        let pretty = config_bool(config, "pretty", ctx).unwrap_or(false);
        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::Value::String(json_to_xml(source, root_tag, pretty)),
        );
        Ok(output)
    }
}

fn json_to_xml(value: &serde_json::Value, root_tag: &str, pretty: bool) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    if pretty {
        xml.push('\n');
    }
    write_element(&mut xml, root_tag, value, pretty, 0);
    xml
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
        serde_json::Value::Object(map) => write_object(xml, tag, map, pretty, depth),
        serde_json::Value::Array(values) => {
            for value in values {
                write_element(xml, tag, value, pretty, depth);
            }
        }
        serde_json::Value::String(value) => {
            xml.push_str(&format!(
                "{}<{}>{}</{}>{}",
                indent,
                tag,
                escape_xml(value),
                tag,
                newline
            ));
        }
        serde_json::Value::Number(value) => {
            xml.push_str(&format!("{indent}<{tag}>{value}</{tag}>{newline}"));
        }
        serde_json::Value::Bool(value) => {
            xml.push_str(&format!("{indent}<{tag}>{value}</{tag}>{newline}"));
        }
        serde_json::Value::Null => {
            xml.push_str(&format!("{indent}<{tag}/>{newline}"));
        }
    }
}

fn write_object(
    xml: &mut String,
    tag: &str,
    map: &serde_json::Map<String, serde_json::Value>,
    pretty: bool,
    depth: usize,
) {
    let indent = if pretty {
        "  ".repeat(depth)
    } else {
        String::new()
    };
    let newline = if pretty { "\n" } else { "" };
    let mut attributes = String::new();
    let mut children = Vec::new();
    let mut text = None;

    for (key, value) in map {
        if let Some(name) = key.strip_prefix('@') {
            if let serde_json::Value::String(value) = value {
                attributes.push_str(&format!(" {}=\"{}\"", name, escape_xml(value)));
            }
        } else if key == "#text" {
            if let serde_json::Value::String(value) = value {
                text = Some(value.as_str());
            }
        } else {
            children.push((key.as_str(), value));
        }
    }

    if children.is_empty() && text.is_none() {
        xml.push_str(&format!("{indent}<{tag}{attributes}/>{newline}"));
        return;
    }
    if children.is_empty() {
        xml.push_str(&format!(
            "{indent}<{tag}{attributes}>{}</{tag}>{newline}",
            escape_xml(text.unwrap_or(""))
        ));
        return;
    }

    xml.push_str(&format!("{indent}<{tag}{attributes}>{newline}"));
    if let Some(text) = text {
        let text_indent = if pretty {
            "  ".repeat(depth + 1)
        } else {
            String::new()
        };
        xml.push_str(&format!("{text_indent}{}{newline}", escape_xml(text)));
    }
    for (child_tag, child_value) in children {
        write_element(xml, child_tag, child_value, pretty, depth + 1);
    }
    xml.push_str(&format!("{indent}</{tag}>{newline}"));
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
