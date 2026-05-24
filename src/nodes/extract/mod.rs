mod common;
mod docx_parser;
mod html;
mod pdf;
mod pptx;
mod pptx_format;
mod pptx_parser;
mod subtitles;
mod word;
mod word_format;

pub(crate) use html::ExtractHtmlNode;
pub(crate) use pdf::ExtractPdfNode;
pub(crate) use pptx::ExtractPptxNode;
pub(crate) use subtitles::{ExtractSrtNode, ExtractVttNode};
pub(crate) use word::ExtractWordNode;

use crate::nodes::NodeRegistry;
use std::sync::Arc;

pub fn register_all(registry: &mut NodeRegistry) {
    registry.register(Arc::new(ExtractWordNode));
    registry.register(Arc::new(ExtractPptxNode));
    registry.register(Arc::new(ExtractPdfNode));
    registry.register(Arc::new(ExtractHtmlNode));
    registry.register(Arc::new(ExtractVttNode));
    registry.register(Arc::new(ExtractSrtNode));
}
