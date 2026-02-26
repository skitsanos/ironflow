use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct TemplateRenderNode;

#[async_trait]
impl Node for TemplateRenderNode {
    fn node_type(&self) -> &str {
        "template_render"
    }

    fn description(&self) -> &str {
        "Render a string template with context variable interpolation"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let template = config
            .get("template")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("template_render requires 'template'"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("template_render requires 'output_key'"))?;

        let rendered = interpolate_ctx(template, &ctx);

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(rendered));
        Ok(output)
    }
}
