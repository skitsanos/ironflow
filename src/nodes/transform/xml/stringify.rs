use std::fmt::Write as _;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::node_config::config_bool;

use super::names::{NameKind, checked_name, checked_text, quoted};

pub struct XmlStringifyNode;

#[async_trait]
impl Node for XmlStringifyNode {
    fn node_type(&self) -> &str {
        "xml_stringify"
    }

    fn description(&self) -> &str {
        "Convert a JSON value to an XML string"
    }

    async fn execute(&self, config: &Value, ctx: &Context) -> Result<NodeOutput> {
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
        let item_tag = config.get("item_tag").and_then(|v| v.as_str());
        let pretty = config_bool(config, "pretty", ctx).unwrap_or(false);
        let source = ctx
            .get(source_key)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            Value::String(json_to_xml(source, root_tag, item_tag, pretty)?),
        );
        Ok(output)
    }
}

fn json_to_xml(
    value: &Value,
    root_tag: &str,
    item_tag: Option<&str>,
    pretty: bool,
) -> Result<String> {
    let root_tag = checked_name(root_tag, NameKind::Element, "'root_tag'")?;
    let item_tag = item_tag
        .map(|tag| checked_name(tag, NameKind::Element, "'item_tag'"))
        .transpose()?;
    let mut writer = Writer {
        xml: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"),
        pretty,
    };
    if pretty {
        writer.xml.push('\n');
    }
    match (value, item_tag) {
        // One document has one root element, so the items of a top-level
        // array need a named wrapper element each.
        (Value::Array(items), Some(item_tag)) => {
            writer.container(
                root_tag,
                "",
                None,
                items.iter().map(|item| (item_tag, item)),
                0,
            )?;
        }
        (Value::Array(_), None) => anyhow::bail!(
            "xml_stringify: the value at 'source_key' is an array, which has no single root \
             element; set 'item_tag' to write each item as a child of the root element"
        ),
        _ => writer.element(root_tag, value, 0)?,
    }
    Ok(writer.xml)
}

struct Writer {
    xml: String,
    pretty: bool,
}

impl Writer {
    fn indent(&self, depth: usize) -> String {
        if self.pretty {
            "  ".repeat(depth)
        } else {
            String::new()
        }
    }

    fn newline(&self) -> &'static str {
        if self.pretty { "\n" } else { "" }
    }

    /// Writes `value` under an already validated `tag`.
    fn element(&mut self, tag: &str, value: &Value, depth: usize) -> Result<()> {
        match value {
            Value::Object(map) => self.object(tag, map, depth),
            Value::Array(values) => values
                .iter()
                .try_for_each(|value| self.element(tag, value, depth)),
            Value::Null => {
                self.leaf(tag, "", None, depth);
                Ok(())
            }
            scalar => {
                let text = scalar_text(scalar).unwrap_or_default();
                let origin = format!("element {}", quoted(tag));
                let text = escape_text(checked_text(&text, &origin)?);
                self.leaf(tag, "", Some(&text), depth);
                Ok(())
            }
        }
    }

    fn object(
        &mut self,
        tag: &str,
        map: &serde_json::Map<String, Value>,
        depth: usize,
    ) -> Result<()> {
        let mut attributes = String::new();
        let mut children = Vec::new();
        let mut text = None;

        for (key, value) in map {
            if let Some(name) = key.strip_prefix('@') {
                let name = checked_name(name, NameKind::Attribute, "attribute key")?;
                let origin = format!("attribute {}", quoted(key));
                if let Some(value) = scalar_content(value, &origin)? {
                    let value = escape_attribute(checked_text(&value, &origin)?);
                    let _ = write!(attributes, " {name}=\"{value}\"");
                }
            } else if key == "#text" {
                let origin = format!("'#text' of element {}", quoted(tag));
                text = scalar_content(value, &origin)?
                    .map(|text| checked_text(&text, &origin).map(escape_text))
                    .transpose()?;
            } else {
                let name = checked_name(key, NameKind::Element, "key")?;
                children.push((name, value));
            }
        }

        if children.is_empty() {
            self.leaf(tag, &attributes, text.as_deref(), depth);
            return Ok(());
        }
        self.container(
            tag,
            &attributes,
            text.as_deref(),
            children.into_iter(),
            depth,
        )
    }

    /// Writes an element that has no child elements.
    fn leaf(&mut self, tag: &str, attributes: &str, text: Option<&str>, depth: usize) {
        let (indent, newline) = (self.indent(depth), self.newline());
        let _ = match text {
            Some(text) => write!(
                self.xml,
                "{indent}<{tag}{attributes}>{text}</{tag}>{newline}"
            ),
            None => write!(self.xml, "{indent}<{tag}{attributes}/>{newline}"),
        };
    }

    /// Writes an element around child elements; with no children it is empty.
    fn container<'a>(
        &mut self,
        tag: &str,
        attributes: &str,
        text: Option<&str>,
        children: impl ExactSizeIterator<Item = (&'a str, &'a Value)>,
        depth: usize,
    ) -> Result<()> {
        if children.len() == 0 {
            self.leaf(tag, attributes, text, depth);
            return Ok(());
        }
        let (indent, newline) = (self.indent(depth), self.newline());
        let _ = write!(self.xml, "{indent}<{tag}{attributes}>{newline}");
        if let Some(text) = text {
            let text_indent = self.indent(depth + 1);
            let _ = write!(self.xml, "{text_indent}{text}{newline}");
        }
        for (child_tag, child_value) in children {
            self.element(child_tag, child_value, depth + 1)?;
        }
        let _ = write!(self.xml, "{indent}</{tag}>{newline}");
        Ok(())
    }
}

/// The text of a string, number or boolean; `None` for every other value.
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

/// Attribute and `#text` content: scalars are written as text, `null` means
/// absent, and an object or array has no text form, so it fails the step.
fn scalar_content(value: &Value, origin: &str) -> Result<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::Object(_) | Value::Array(_) => anyhow::bail!(
            "xml_stringify: {origin} must be a string, number, boolean or null, not an {}",
            if value.is_object() { "object" } else { "array" }
        ),
        scalar => Ok(scalar_text(scalar)),
    }
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('\r', "&#13;")
}

/// Attribute values also escape tab and newline, which a parser would
/// otherwise normalize to spaces.
fn escape_attribute(value: &str) -> String {
    escape_text(value)
        .replace('\t', "&#9;")
        .replace('\n', "&#10;")
}
