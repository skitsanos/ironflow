//! Resource ceilings for image decoding and multi-image PDF construction.

use super::{Limit, MIB};

/// Encoded bytes accepted for one image source (50 MiB).
pub(crate) const MAX_IMAGE_ENCODED_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_IMAGE_ENCODED_BYTES", 50 * MIB);
/// Decoded pixels accepted for one image (25 megapixels).
pub(crate) const MAX_IMAGE_PIXELS: Limit = Limit::positive("IRONFLOW_MAX_IMAGE_PIXELS", 25_000_000);
/// Decoder-managed allocation and admitted image working buffers (128 MiB).
pub(crate) const MAX_IMAGE_DECODE_ALLOCATION_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES", 128 * MIB);
/// Images admitted to one `image_to_pdf` call.
pub(crate) const MAX_IMAGE_TO_PDF_SOURCES: Limit =
    Limit::positive("IRONFLOW_MAX_IMAGE_TO_PDF_SOURCES", 100);
/// Cumulative encoded input retained by one `image_to_pdf` call (100 MiB).
pub(crate) const MAX_IMAGE_TO_PDF_ENCODED_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_IMAGE_TO_PDF_ENCODED_BYTES", 100 * MIB);
/// Cumulative decoded pixels admitted to one `image_to_pdf` call.
pub(crate) const MAX_IMAGE_TO_PDF_PIXELS: Limit =
    Limit::positive("IRONFLOW_MAX_IMAGE_TO_PDF_PIXELS", 50_000_000);

pub fn max_image_encoded_bytes() -> u64 {
    MAX_IMAGE_ENCODED_BYTES.read()
}

pub fn max_image_pixels() -> u64 {
    MAX_IMAGE_PIXELS.read()
}

pub fn max_image_decode_allocation_bytes() -> u64 {
    MAX_IMAGE_DECODE_ALLOCATION_BYTES.read()
}

pub fn max_image_to_pdf_sources() -> u64 {
    MAX_IMAGE_TO_PDF_SOURCES.read()
}

pub fn max_image_to_pdf_encoded_bytes() -> u64 {
    MAX_IMAGE_TO_PDF_ENCODED_BYTES.read()
}

pub fn max_image_to_pdf_pixels() -> u64 {
    MAX_IMAGE_TO_PDF_PIXELS.read()
}
