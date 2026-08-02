use super::env_u64;

const DEFAULT_MAX_PDF_BYTES: u64 = 100 * 1024 * 1024;
const DEFAULT_MAX_PDF_EXTRACT_PAGES: u64 = 1_000;
const DEFAULT_MAX_PDF_RENDER_PAGES: u64 = 25;
const DEFAULT_MAX_PDF_SPLIT_PAGES: u64 = 1_000;
const DEFAULT_MAX_PDF_RENDER_PIXELS: u64 = 25_000_000;
const DEFAULT_MAX_PDF_DPI: u64 = 300;
const DEFAULT_MAX_PDF_MERGE_FILES: u64 = 100;
const DEFAULT_MAX_PDF_MERGE_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_MAX_PDF_MERGE_PAGES: u64 = 2_000;
const DEFAULT_MAX_PDF_MERGE_OBJECTS: u64 = 250_000;

pub fn max_pdf_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_PDF_BYTES", DEFAULT_MAX_PDF_BYTES)
}

pub fn max_pdf_extract_pages() -> u64 {
    env_u64(
        "IRONFLOW_MAX_PDF_EXTRACT_PAGES",
        DEFAULT_MAX_PDF_EXTRACT_PAGES,
    )
}

pub fn max_pdf_render_pages() -> u64 {
    env_u64(
        "IRONFLOW_MAX_PDF_RENDER_PAGES",
        DEFAULT_MAX_PDF_RENDER_PAGES,
    )
}

pub fn max_pdf_split_pages() -> u64 {
    env_u64("IRONFLOW_MAX_PDF_SPLIT_PAGES", DEFAULT_MAX_PDF_SPLIT_PAGES)
}

pub fn max_pdf_render_pixels() -> u64 {
    env_u64(
        "IRONFLOW_MAX_PDF_RENDER_PIXELS",
        DEFAULT_MAX_PDF_RENDER_PIXELS,
    )
}

pub fn max_pdf_dpi() -> u64 {
    env_u64("IRONFLOW_MAX_PDF_DPI", DEFAULT_MAX_PDF_DPI)
}

pub fn max_pdf_merge_files() -> u64 {
    env_u64("IRONFLOW_MAX_PDF_MERGE_FILES", DEFAULT_MAX_PDF_MERGE_FILES)
}

pub fn max_pdf_merge_bytes() -> u64 {
    env_u64("IRONFLOW_MAX_PDF_MERGE_BYTES", DEFAULT_MAX_PDF_MERGE_BYTES)
}

pub fn max_pdf_merge_pages() -> u64 {
    env_u64("IRONFLOW_MAX_PDF_MERGE_PAGES", DEFAULT_MAX_PDF_MERGE_PAGES)
}

pub fn max_pdf_merge_objects() -> u64 {
    env_u64(
        "IRONFLOW_MAX_PDF_MERGE_OBJECTS",
        DEFAULT_MAX_PDF_MERGE_OBJECTS,
    )
}
