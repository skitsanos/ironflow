mod cache;
pub(crate) mod code;
mod date;
mod delay;
mod encoding;
mod hash;
mod html_sanitize;
mod log;
mod markdown;
mod shell;
mod template;
mod validate;

use crate::nodes::NodeRegistry;
use std::sync::Arc;

/// Register all utility nodes into the registry.
pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(log::LogNode));
    registry.register(Arc::new(delay::DelayNode));
    registry.register(Arc::new(shell::ShellCommandNode));
    registry.register(Arc::new(hash::HashNode));
    registry.register(Arc::new(date::DateFormatNode));
    registry.register(Arc::new(template::TemplateRenderNode));
    registry.register(Arc::new(markdown::MarkdownToHtmlNode));
    registry.register(Arc::new(markdown::HtmlToMarkdownNode));
    registry.register(Arc::new(html_sanitize::HtmlSanitizeNode));
    registry.register(Arc::new(encoding::Base64EncodeNode));
    registry.register(Arc::new(encoding::Base64DecodeNode));
    registry.register(Arc::new(validate::ValidateSchemaNode));
    registry.register(Arc::new(validate::JsonValidateNode));
    registry.register(Arc::new(cache::CacheSetNode));
    registry.register(Arc::new(cache::CacheGetNode));
    registry.register(Arc::new(code::CodeNode));
}
