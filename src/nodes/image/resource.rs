use anyhow::Result;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ImageDecodeLimits {
    pub(crate) max_encoded_bytes: u64,
    pub(crate) max_pixels: u64,
    pub(crate) max_allocation_bytes: u64,
}

impl ImageDecodeLimits {
    pub(crate) fn current() -> Self {
        Self {
            max_encoded_bytes: crate::util::limits::max_image_encoded_bytes(),
            max_pixels: crate::util::limits::max_image_pixels(),
            max_allocation_bytes: crate::util::limits::max_image_decode_allocation_bytes(),
        }
    }

    pub(crate) fn decoder_limits(self) -> image::Limits {
        let maximum_dimension = u32::try_from(self.max_pixels).unwrap_or(u32::MAX);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(maximum_dimension);
        limits.max_image_height = Some(maximum_dimension);
        limits.max_alloc = Some(self.max_allocation_bytes);
        limits
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ImageToPdfLimits {
    pub(crate) decode: ImageDecodeLimits,
    pub(crate) max_sources: u64,
    pub(crate) max_encoded_bytes: u64,
    pub(crate) max_pixels: u64,
}

impl ImageToPdfLimits {
    pub(crate) fn current() -> Self {
        Self {
            decode: ImageDecodeLimits::current(),
            max_sources: crate::util::limits::max_image_to_pdf_sources(),
            max_encoded_bytes: crate::util::limits::max_image_to_pdf_encoded_bytes(),
            max_pixels: crate::util::limits::max_image_to_pdf_pixels(),
        }
    }

    pub(crate) fn validate_source_count(self, count: usize) -> Result<()> {
        let count = u64::try_from(count).unwrap_or(u64::MAX);
        if count > self.max_sources {
            anyhow::bail!(
                "image_to_pdf: {count} sources exceed IRONFLOW_MAX_IMAGE_TO_PDF_SOURCES ({})",
                self.max_sources
            );
        }
        Ok(())
    }
}

pub(crate) struct ImageToPdfBudget {
    limits: ImageToPdfLimits,
    encoded_bytes: u64,
    pixels: u64,
}

impl ImageToPdfBudget {
    pub(crate) fn new(limits: ImageToPdfLimits) -> Self {
        Self {
            limits,
            encoded_bytes: 0,
            pixels: 0,
        }
    }

    pub(crate) fn remaining_encoded_bytes(&self) -> u64 {
        self.limits
            .max_encoded_bytes
            .saturating_sub(self.encoded_bytes)
            .min(self.limits.decode.max_encoded_bytes)
    }

    pub(crate) fn admit(&mut self, encoded_bytes: u64, pixels: u64) -> Result<()> {
        let next_encoded = self
            .encoded_bytes
            .checked_add(encoded_bytes)
            .ok_or_else(|| {
                anyhow::anyhow!("image_to_pdf: cumulative encoded byte count overflow")
            })?;
        if next_encoded > self.limits.max_encoded_bytes {
            anyhow::bail!(
                "image_to_pdf: cumulative encoded input exceeds IRONFLOW_MAX_IMAGE_TO_PDF_ENCODED_BYTES ({})",
                self.limits.max_encoded_bytes
            );
        }
        let next_pixels = self
            .pixels
            .checked_add(pixels)
            .ok_or_else(|| anyhow::anyhow!("image_to_pdf: cumulative pixel count overflow"))?;
        if next_pixels > self.limits.max_pixels {
            anyhow::bail!(
                "image_to_pdf: cumulative decoded pixels exceed IRONFLOW_MAX_IMAGE_TO_PDF_PIXELS ({})",
                self.limits.max_pixels
            );
        }
        self.encoded_bytes = next_encoded;
        self.pixels = next_pixels;
        Ok(())
    }
}

pub(crate) fn validate_image_shape(
    operation: &str,
    label: &str,
    width: u32,
    height: u32,
    total_bytes: u64,
    limits: ImageDecodeLimits,
) -> Result<u64> {
    if width == 0 || height == 0 {
        anyhow::bail!("{operation}: image '{label}' dimensions must be greater than zero");
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| anyhow::anyhow!("{operation}: image '{label}' pixel count overflow"))?;
    if pixels > limits.max_pixels {
        anyhow::bail!(
            "{operation}: image '{label}' is {width}x{height} ({pixels} pixels), exceeds IRONFLOW_MAX_IMAGE_PIXELS ({})",
            limits.max_pixels
        );
    }
    if total_bytes > limits.max_allocation_bytes {
        anyhow::bail!(
            "{operation}: image '{label}' requires at least {total_bytes} decoded bytes, exceeds IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES ({})",
            limits.max_allocation_bytes
        );
    }
    Ok(pixels)
}

pub(crate) fn validate_output_shape(
    operation: &str,
    width: u32,
    height: u32,
    color_type: image::ColorType,
    source_bytes: u64,
    limits: ImageDecodeLimits,
) -> Result<u64> {
    let total_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(u64::from(color_type.bytes_per_pixel())))
        .unwrap_or(u64::MAX);
    let pixels = validate_image_shape(operation, "output", width, height, total_bytes, limits)?;
    validate_combined_allocation(operation, source_bytes, total_bytes, limits)?;
    Ok(pixels)
}

pub(crate) fn validate_combined_allocation(
    operation: &str,
    retained_bytes: u64,
    additional_bytes: u64,
    limits: ImageDecodeLimits,
) -> Result<()> {
    let peak = retained_bytes
        .checked_add(additional_bytes)
        .ok_or_else(|| anyhow::anyhow!("{operation}: working allocation estimate overflow"))?;
    if peak > limits.max_allocation_bytes {
        anyhow::bail!(
            "{operation}: source and output buffers require at least {peak} bytes, exceeds IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES ({})",
            limits.max_allocation_bytes
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> ImageToPdfLimits {
        ImageToPdfLimits {
            decode: ImageDecodeLimits {
                max_encoded_bytes: 10,
                max_pixels: 10,
                max_allocation_bytes: 40,
            },
            max_sources: 2,
            max_encoded_bytes: 12,
            max_pixels: 8,
        }
    }

    #[test]
    fn cumulative_pdf_budget_fails_before_growth() {
        let limits = limits();
        assert!(limits.validate_source_count(3).is_err());
        let mut budget = ImageToPdfBudget::new(limits);
        budget.admit(7, 4).unwrap();
        assert_eq!(budget.remaining_encoded_bytes(), 5);
        assert!(
            budget
                .admit(6, 1)
                .unwrap_err()
                .to_string()
                .contains("encoded")
        );
        assert!(
            budget
                .admit(5, 5)
                .unwrap_err()
                .to_string()
                .contains("pixels")
        );
    }

    #[test]
    fn shape_validation_checks_pixels_and_decoded_bytes() {
        let limits = limits().decode;
        assert!(validate_image_shape("test", "wide", 11, 1, 11, limits).is_err());
        assert!(validate_image_shape("test", "deep", 2, 2, 41, limits).is_err());
        assert_eq!(
            validate_image_shape("test", "ok", 2, 2, 16, limits).unwrap(),
            4
        );
        assert!(validate_combined_allocation("test", 24, 17, limits).is_err());
        validate_combined_allocation("test", 24, 16, limits).unwrap();
    }
}
