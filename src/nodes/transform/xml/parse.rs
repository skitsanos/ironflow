use anyhow::Result;
use async_trait::async_trait;
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;
use crate::util::xml::{Decoder, attribute_value};

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
        let parsed =
            run_tracked_blocking_step(move |execution| parse_xml_to_json(&input, &execution))
                .await?;

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

/// Maximum element-nesting depth accepted by `xml_parse`. Beyond this the
/// resulting `serde_json::Value` is deep enough that recursively dropping or
/// serializing it can overflow the worker thread's stack (an uncatchable
/// process abort), so pathological input is rejected as an ordinary node error.
/// Matches the YAML parser's own nesting limit for a uniform contract.
const MAX_XML_NESTING_DEPTH: usize = 128;

fn parse_xml_to_json(
    xml: &str,
    execution: &crate::util::execution::ExecutionControl,
) -> Result<serde_json::Value> {
    let mut reader = Reader::from_str(xml);
    let mut decoder = Decoder::default();
    let mut remaining = xml.len() as u64;
    let mut charge = |bytes: u64| -> Result<()> {
        execution.checkpoint()?;
        remaining = remaining
            .checked_sub(bytes)
            .ok_or_else(|| anyhow::anyhow!("XML decoded content exceeds input byte budget"))?;
        Ok(())
    };
    let mut stack: Vec<(String, serde_json::Map<String, serde_json::Value>)> = Vec::new();
    let mut root = None;

    loop {
        execution.checkpoint()?;
        let event = decoder.decode(
            reader
                .read_event()
                .map_err(|error| anyhow::anyhow!("XML parse error: {error}"))?,
            &mut charge,
        )?;
        match event {
            Event::Start(element) => {
                if stack.len() >= MAX_XML_NESTING_DEPTH {
                    anyhow::bail!(
                        "XML nesting depth exceeds the limit of {}",
                        MAX_XML_NESTING_DEPTH
                    );
                }
                stack.push((
                    element_name(&element),
                    attributes(&element, decoder.version, &mut charge)?,
                ));
            }
            Event::Empty(element) => {
                anyhow::ensure!(
                    stack.len() < MAX_XML_NESTING_DEPTH,
                    "XML nesting depth exceeds the limit of {MAX_XML_NESTING_DEPTH}"
                );
                let name = element_name(&element);
                let map = attributes(&element, decoder.version, &mut charge)?;
                let value = if map.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::Object(map)
                };
                add_element(&mut stack, &mut root, name, value);
            }
            Event::Text(text) => {
                if let Some((_, map)) = stack.last_mut() {
                    let value = map
                        .entry("#text")
                        .or_insert_with(|| serde_json::Value::String(String::new()));
                    if let serde_json::Value::String(value) = value {
                        value.push_str(text.as_ref());
                    }
                } else if !text.trim().is_empty() {
                    anyhow::bail!("XML text outside the root element");
                }
            }
            Event::End(_) => {
                if let Some((name, map)) = stack.pop() {
                    add_element(&mut stack, &mut root, name, simplify_element(map));
                }
            }
            Event::Eof => {
                anyhow::ensure!(
                    stack.is_empty(),
                    "XML document ended with unclosed elements"
                );
                break;
            }
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
    element.name().as_ref().to_string()
}

fn attributes(
    element: &quick_xml::events::BytesStart<'_>,
    version: quick_xml::XmlVersion,
    mut charge: impl FnMut(u64) -> Result<()>,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    element
        .attributes()
        .map(|attribute| {
            let attribute = attribute?;
            Ok((
                format!("@{}", attribute.key.as_ref()),
                serde_json::Value::String(
                    attribute_value(&attribute, version, &mut charge)?.into_owned(),
                ),
            ))
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

fn simplify_element(mut map: serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    if let Some(serde_json::Value::String(text)) = map.get_mut("#text") {
        *text = text.trim().to_owned();
        if text.is_empty() {
            map.remove("#text");
        }
    }
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
