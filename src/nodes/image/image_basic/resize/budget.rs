use anyhow::{Result, anyhow};

use super::super::super::common::target_size;
use super::super::super::resource::{ImageDecodeLimits, validate_output_shape};

pub(super) fn admit(
    source: (u32, u32),
    requested: (Option<u32>, Option<u32>),
    color: image::ColorType,
    source_bytes: u64,
    limits: ImageDecodeLimits,
) -> Result<(u32, u32)> {
    let target = target_size(source.0, source.1, requested.0, requested.1)?;
    validate_output_shape(
        "image_resize",
        target.0,
        target.1,
        color,
        source_bytes,
        limits,
    )?;
    let peak = peak_bytes(source, target, color, source_bytes)?;
    if peak > limits.max_allocation_bytes {
        anyhow::bail!(
            "image_resize: source, output, resampling intermediate and filter scratch buffers require an estimated {peak} bytes, exceeds IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES ({})",
            limits.max_allocation_bytes
        );
    }
    Ok(target)
}

fn peak_bytes(
    source: (u32, u32),
    target: (u32, u32),
    color: image::ColorType,
    source_bytes: u64,
) -> Result<u64> {
    let output = buffer_bytes(target, u64::from(color.bytes_per_pixel()))?;
    if source == target {
        return source_bytes.checked_add(output).ok_or_else(overflow);
    }

    // image 0.25.10 resizes vertically into RGBA f32, then horizontally into
    // the original pixel type. Revisit this model when its sampler changes.
    let intermediate = buffer_bytes((source.0, target.1), 16)?;
    let vertical_scratch = filter_scratch_bytes(source.1)?;
    let horizontal = output
        .checked_add(filter_scratch_bytes(source.0)?)
        .ok_or_else(overflow)?;
    source_bytes
        .checked_add(intermediate)
        .and_then(|bytes| bytes.checked_add(vertical_scratch.max(horizontal)))
        .ok_or_else(overflow)
}

fn buffer_bytes(dimensions: (u32, u32), bytes_per_pixel: u64) -> Result<u64> {
    u64::from(dimensions.0)
        .checked_mul(u64::from(dimensions.1))
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .filter(|bytes| *bytes <= isize::MAX as u64)
        .ok_or_else(overflow)
}

fn filter_scratch_bytes(source_axis: u32) -> Result<u64> {
    // At most one axis of f32 weights is live per pass. Bound Vec growth and
    // simultaneous old/new storage during reallocation, even for wide filters.
    u64::from(source_axis)
        .max(4)
        .checked_next_power_of_two()
        .and_then(|capacity| capacity.checked_mul(2 * size_of::<f32>() as u64))
        .filter(|bytes| *bytes <= isize::MAX as u64)
        .ok_or_else(overflow)
}

fn overflow() -> anyhow::Error {
    anyhow!("image_resize: working allocation estimate overflow or unaddressable buffer")
}

#[cfg(test)]
mod tests;
