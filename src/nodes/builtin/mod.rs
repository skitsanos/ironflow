mod cache_node;
pub(crate) mod code_node;
mod conditional_node;
mod arangodb_node;
mod db_node;
mod delay_node;
mod extract_node;
mod file_node;
mod foreach_node;
mod hash_node;
mod http_node;
mod log_node;
mod lua_sandbox;
mod markdown_node;
#[cfg(feature = "pdf-render")]
mod pdf_image_node;
mod shell_node;
pub(crate) mod subworkflow_node;
mod template_node;
mod transform_node;
mod validate_node;

use crate::nodes::NodeRegistry;
use std::sync::Arc;

/// Register all built-in nodes into the registry.
pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(log_node::LogNode));
    registry.register(Arc::new(delay_node::DelayNode));
    registry.register(Arc::new(shell_node::ShellCommandNode));
    registry.register(Arc::new(http_node::HttpRequestNode));
    registry.register(Arc::new(http_node::HttpGetNode));
    registry.register(Arc::new(http_node::HttpPostNode));
    registry.register(Arc::new(http_node::HttpPutNode));
    registry.register(Arc::new(http_node::HttpDeleteNode));
    registry.register(Arc::new(file_node::ReadFileNode));
    registry.register(Arc::new(file_node::WriteFileNode));
    registry.register(Arc::new(file_node::CopyFileNode));
    registry.register(Arc::new(file_node::MoveFileNode));
    registry.register(Arc::new(file_node::DeleteFileNode));
    registry.register(Arc::new(file_node::ListDirectoryNode));
    registry.register(Arc::new(transform_node::JsonParseNode));
    registry.register(Arc::new(transform_node::JsonStringifyNode));
    registry.register(Arc::new(transform_node::SelectFieldsNode));
    registry.register(Arc::new(transform_node::RenameFieldsNode));
    registry.register(Arc::new(transform_node::DataFilterNode));
    registry.register(Arc::new(transform_node::DataTransformNode));
    registry.register(Arc::new(transform_node::BatchNode));
    registry.register(Arc::new(transform_node::DeduplicateNode));
    registry.register(Arc::new(conditional_node::IfNode));
    registry.register(Arc::new(conditional_node::SwitchNode));
    registry.register(Arc::new(validate_node::ValidateSchemaNode));
    registry.register(Arc::new(template_node::TemplateRenderNode));
    registry.register(Arc::new(hash_node::HashNode));
    registry.register(Arc::new(code_node::CodeNode));
    registry.register(Arc::new(markdown_node::MarkdownToHtmlNode));
    registry.register(Arc::new(markdown_node::HtmlToMarkdownNode));
    registry.register(Arc::new(extract_node::ExtractWordNode));
    registry.register(Arc::new(extract_node::ExtractPdfNode));
    registry.register(Arc::new(extract_node::ExtractHtmlNode));
    registry.register(Arc::new(foreach_node::ForEachNode));
    registry.register(Arc::new(cache_node::CacheSetNode));
    registry.register(Arc::new(cache_node::CacheGetNode));
    registry.register(Arc::new(db_node::DbQueryNode));
    registry.register(Arc::new(db_node::DbExecNode));
    registry.register(Arc::new(arangodb_node::ArangoDbAqlNode));

    #[cfg(feature = "pdf-render")]
    registry.register(Arc::new(pdf_image_node::PdfToImageNode));
}
