//! Resource ceilings for PDF loading, rendering, splitting and merging.

use super::{Limit, MIB};

/// Size of each PDF accepted by rendering, metadata, splitting, merging and
/// extraction (100 MiB).
pub(crate) const MAX_PDF_BYTES: Limit = Limit::positive("IRONFLOW_MAX_PDF_BYTES", 100 * MIB);
/// Decoded bytes per object or cross-reference stream (64 MiB).
pub(crate) const MAX_PDF_DECOMPRESSED_STREAM_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES", 64 * MIB);
/// Loaded document objects per source.
pub(crate) const MAX_PDF_OBJECTS: Limit = Limit::positive("IRONFLOW_MAX_PDF_OBJECTS", 250_000);
/// Pages extracted from one document.
pub(crate) const MAX_PDF_EXTRACT_PAGES: Limit =
    Limit::positive("IRONFLOW_MAX_PDF_EXTRACT_PAGES", 1_000);
/// Pages rendered from one document.
pub(crate) const MAX_PDF_RENDER_PAGES: Limit = Limit::positive("IRONFLOW_MAX_PDF_RENDER_PAGES", 25);
/// Pages split from one document.
pub(crate) const MAX_PDF_SPLIT_PAGES: Limit =
    Limit::positive("IRONFLOW_MAX_PDF_SPLIT_PAGES", 1_000);
/// Pixels rendered for one page (25 megapixels).
pub(crate) const MAX_PDF_RENDER_PIXELS: Limit =
    Limit::positive("IRONFLOW_MAX_PDF_RENDER_PIXELS", 25_000_000);
/// Rendering resolution in dots per inch.
pub(crate) const MAX_PDF_DPI: Limit = Limit::positive("IRONFLOW_MAX_PDF_DPI", 300);
/// Sources admitted by one `pdf_merge` call.
pub(crate) const MAX_PDF_MERGE_FILES: Limit = Limit::positive("IRONFLOW_MAX_PDF_MERGE_FILES", 100);
/// Cumulative input and staged output bytes for one `pdf_merge` call (512 MiB).
pub(crate) const MAX_PDF_MERGE_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_PDF_MERGE_BYTES", 512 * MIB);
/// Cumulative pages admitted by one `pdf_merge` call.
pub(crate) const MAX_PDF_MERGE_PAGES: Limit =
    Limit::positive("IRONFLOW_MAX_PDF_MERGE_PAGES", 2_000);
/// Retained merged-graph objects for one `pdf_merge` call.
pub(crate) const MAX_PDF_MERGE_OBJECTS: Limit =
    Limit::positive("IRONFLOW_MAX_PDF_MERGE_OBJECTS", 250_000);

pub fn max_pdf_bytes() -> u64 {
    MAX_PDF_BYTES.read()
}

pub fn max_pdf_decompressed_stream_bytes() -> u64 {
    MAX_PDF_DECOMPRESSED_STREAM_BYTES.read()
}

pub fn max_pdf_objects() -> u64 {
    MAX_PDF_OBJECTS.read()
}

pub fn max_pdf_extract_pages() -> u64 {
    MAX_PDF_EXTRACT_PAGES.read()
}

pub fn max_pdf_render_pages() -> u64 {
    MAX_PDF_RENDER_PAGES.read()
}

pub fn max_pdf_split_pages() -> u64 {
    MAX_PDF_SPLIT_PAGES.read()
}

pub fn max_pdf_render_pixels() -> u64 {
    MAX_PDF_RENDER_PIXELS.read()
}

pub fn max_pdf_dpi() -> u64 {
    MAX_PDF_DPI.read()
}

pub fn max_pdf_merge_files() -> u64 {
    MAX_PDF_MERGE_FILES.read()
}

pub fn max_pdf_merge_bytes() -> u64 {
    MAX_PDF_MERGE_BYTES.read()
}

pub fn max_pdf_merge_pages() -> u64 {
    MAX_PDF_MERGE_PAGES.read()
}

pub fn max_pdf_merge_objects() -> u64 {
    MAX_PDF_MERGE_OBJECTS.read()
}
