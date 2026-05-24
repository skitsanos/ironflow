mod cache_node;
pub(crate) mod code_node;
mod conditional_node;
mod date_node;
mod delay_node;
mod encoding_node;
mod foreach_node;
mod hash_node;
mod html_sanitize_node;
mod log_node;
mod lua_sandbox;
mod markdown_node;
mod mcp_node;
pub(crate) mod parallel_subworkflows_node;
mod shell_node;
pub(crate) mod subworkflow_node;
mod template_node;
mod validate_node;

use crate::nodes::NodeRegistry;
use std::sync::Arc;

/// Register all built-in nodes into the registry.
pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(log_node::LogNode));
    registry.register(Arc::new(delay_node::DelayNode));
    registry.register(Arc::new(shell_node::ShellCommandNode));
    registry.register(Arc::new(conditional_node::IfNode));
    registry.register(Arc::new(conditional_node::SwitchNode));
    registry.register(Arc::new(conditional_node::IfHttpStatusNode));
    registry.register(Arc::new(conditional_node::IfBodyContainsNode));
    registry.register(Arc::new(validate_node::ValidateSchemaNode));
    registry.register(Arc::new(validate_node::JsonValidateNode));
    registry.register(Arc::new(template_node::TemplateRenderNode));
    registry.register(Arc::new(hash_node::HashNode));
    registry.register(Arc::new(code_node::CodeNode));
    registry.register(Arc::new(html_sanitize_node::HtmlSanitizeNode));
    registry.register(Arc::new(markdown_node::MarkdownToHtmlNode));
    registry.register(Arc::new(markdown_node::HtmlToMarkdownNode));
    registry.register(Arc::new(foreach_node::ForEachNode));
    registry.register(Arc::new(cache_node::CacheSetNode));
    registry.register(Arc::new(cache_node::CacheGetNode));

    registry.register(Arc::new(mcp_node::McpClientNode));

    registry.register(Arc::new(date_node::DateFormatNode));

    registry.register(Arc::new(encoding_node::Base64EncodeNode));
    registry.register(Arc::new(encoding_node::Base64DecodeNode));
}
