pub(crate) mod common;
pub(crate) mod image_advanced;
pub(crate) mod image_basic;
pub(crate) mod image_conversion;
pub(crate) mod image_metadata;
pub(crate) mod image_sources;
pub(crate) mod pdf_merge_split;
pub(crate) mod pdf_metadata;
pub(crate) mod pdf_render;

pub(crate) use image_advanced::{ImageConvertNode, ImageGrayscaleNode, ImageWatermarkNode};
pub(crate) use image_basic::{ImageCropNode, ImageFlipNode, ImageResizeNode, ImageRotateNode};
pub(crate) use image_conversion::ImageToPdfNode;
pub(crate) use image_metadata::ImageMetadataNode;
pub(crate) use pdf_merge_split::{PdfMergeNode, PdfSplitNode};
pub(crate) use pdf_metadata::PdfMetadataNode;
pub(crate) use pdf_render::{PdfThumbnailNode, PdfToImageNode};

use std::sync::Arc;

pub fn register_all(registry: &mut crate::nodes::NodeRegistry) {
    registry.register(Arc::new(PdfToImageNode));
    registry.register(Arc::new(PdfThumbnailNode));
    registry.register(Arc::new(ImageToPdfNode));
    registry.register(Arc::new(PdfMetadataNode));
    registry.register(Arc::new(ImageResizeNode));
    registry.register(Arc::new(ImageCropNode));
    registry.register(Arc::new(ImageRotateNode));
    registry.register(Arc::new(ImageFlipNode));
    registry.register(Arc::new(ImageGrayscaleNode));
    registry.register(Arc::new(ImageMetadataNode));
    registry.register(Arc::new(ImageConvertNode));
    registry.register(Arc::new(ImageWatermarkNode));
    registry.register(Arc::new(PdfMergeNode));
    registry.register(Arc::new(PdfSplitNode));
}
