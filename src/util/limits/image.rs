//! Resource ceilings for image decoding and multi-image PDF construction.

use super::env_u64;

/// Maximum encoded bytes accepted for one image source (50 MiB).
const DEFAULT_MAX_IMAGE_ENCODED_BYTES: u64 = 50 * 1024 * 1024;

/// Maximum decoded pixels accepted for one image (25 megapixels).
const DEFAULT_MAX_IMAGE_PIXELS: u64 = 25_000_000;

/// Maximum decoder-managed allocation for one image (128 MiB).
const DEFAULT_MAX_IMAGE_DECODE_ALLOCATION_BYTES: u64 = 128 * 1024 * 1024;

/// Maximum number of images admitted to one `image_to_pdf` call.
const DEFAULT_MAX_IMAGE_TO_PDF_SOURCES: u64 = 100;

/// Maximum cumulative encoded input retained by one `image_to_pdf` call (100 MiB).
const DEFAULT_MAX_IMAGE_TO_PDF_ENCODED_BYTES: u64 = 100 * 1024 * 1024;

/// Maximum cumulative decoded pixels admitted to one `image_to_pdf` call.
const DEFAULT_MAX_IMAGE_TO_PDF_PIXELS: u64 = 50_000_000;

pub fn max_image_encoded_bytes() -> u64 {
    env_u64(
        "IRONFLOW_MAX_IMAGE_ENCODED_BYTES",
        DEFAULT_MAX_IMAGE_ENCODED_BYTES,
    )
}

pub fn max_image_pixels() -> u64 {
    env_u64("IRONFLOW_MAX_IMAGE_PIXELS", DEFAULT_MAX_IMAGE_PIXELS)
}

pub fn max_image_decode_allocation_bytes() -> u64 {
    env_u64(
        "IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES",
        DEFAULT_MAX_IMAGE_DECODE_ALLOCATION_BYTES,
    )
}

pub fn max_image_to_pdf_sources() -> u64 {
    env_u64(
        "IRONFLOW_MAX_IMAGE_TO_PDF_SOURCES",
        DEFAULT_MAX_IMAGE_TO_PDF_SOURCES,
    )
}

pub fn max_image_to_pdf_encoded_bytes() -> u64 {
    env_u64(
        "IRONFLOW_MAX_IMAGE_TO_PDF_ENCODED_BYTES",
        DEFAULT_MAX_IMAGE_TO_PDF_ENCODED_BYTES,
    )
}

pub fn max_image_to_pdf_pixels() -> u64 {
    env_u64(
        "IRONFLOW_MAX_IMAGE_TO_PDF_PIXELS",
        DEFAULT_MAX_IMAGE_TO_PDF_PIXELS,
    )
}
