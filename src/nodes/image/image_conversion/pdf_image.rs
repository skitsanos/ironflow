use anyhow::Result;
use lopdf::{Object, Stream, dictionary};

use crate::util::execution::ExecutionControl;

use super::super::common::decode_image_bytes;
use super::super::image_sources::LoadedImageBytes;
use super::super::resource::ImageDecodeLimits;

pub(super) fn image_stream(
    loaded: LoadedImageBytes,
    limits: ImageDecodeLimits,
    execution: &ExecutionControl,
) -> Result<Stream> {
    let LoadedImageBytes {
        label,
        bytes,
        width,
        height,
        format,
        color_type,
        total_bytes,
        ..
    } = loaded;
    let (bits, color_space) = pdf_color(color_type)?;
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => width,
        "Height" => height,
        "ColorSpace" => Object::Name(color_space.to_vec()),
        "BitsPerComponent" => bits,
    };

    if format == image::ImageFormat::Jpeg {
        dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
        return Ok(Stream::new(dict, bytes));
    }

    validate_conversion_allocation(bytes.len(), total_bytes, color_type, limits)?;
    let image = decode_image_bytes(&bytes, &label, limits, execution)?;
    drop(bytes);
    execution.checkpoint()?;
    let content = pdf_pixels(image, color_type)?;
    execution.checkpoint()?;
    let mut stream = Stream::new(dict, content);
    stream.compress().map_err(|error| {
        anyhow::anyhow!("image_to_pdf: failed to compress image '{label}': {error}")
    })?;
    execution.checkpoint()?;
    Ok(stream)
}

fn validate_conversion_allocation(
    encoded_bytes: usize,
    decoded_bytes: u64,
    color_type: image::ColorType,
    limits: ImageDecodeLimits,
) -> Result<()> {
    let output_bytes = pdf_bytes(decoded_bytes, color_type)?;
    let conversion_bytes = match color_type {
        image::ColorType::L8 | image::ColorType::Rgb8 => 0,
        _ => output_bytes,
    };
    let peak = u64::try_from(encoded_bytes)
        .unwrap_or(u64::MAX)
        .checked_add(decoded_bytes)
        .and_then(|value| value.checked_add(conversion_bytes))
        .and_then(|value| value.checked_add(output_bytes))
        .ok_or_else(|| anyhow::anyhow!("image_to_pdf: decode allocation estimate overflow"))?;
    let limit = limits.max_allocation_bytes;
    if peak > limit {
        anyhow::bail!(
            "image_to_pdf: encoded, decoded, and conversion buffers require at least {peak} bytes, exceeds IRONFLOW_MAX_IMAGE_DECODE_ALLOCATION_BYTES ({limit})"
        );
    }
    Ok(())
}

fn pdf_bytes(decoded_bytes: u64, color_type: image::ColorType) -> Result<u64> {
    let input_channels = u64::from(color_type.channel_count());
    let output_channels = if color_type.has_color() { 3 } else { 1 };
    decoded_bytes
        .checked_div(input_channels)
        .and_then(|samples| samples.checked_mul(output_channels))
        .ok_or_else(|| anyhow::anyhow!("image_to_pdf: image conversion byte count overflow"))
}

fn pdf_color(color_type: image::ColorType) -> Result<(i64, &'static [u8])> {
    match color_type {
        image::ColorType::L8 | image::ColorType::La8 => Ok((8, b"DeviceGray")),
        image::ColorType::Rgb8 | image::ColorType::Rgba8 => Ok((8, b"DeviceRGB")),
        image::ColorType::L16 | image::ColorType::La16 => Ok((16, b"DeviceGray")),
        image::ColorType::Rgb16 | image::ColorType::Rgba16 => Ok((16, b"DeviceRGB")),
        image::ColorType::Rgb32F | image::ColorType::Rgba32F => {
            anyhow::bail!("image_to_pdf: floating-point images are not supported")
        }
        _ => anyhow::bail!("image_to_pdf: unsupported image color type"),
    }
}

fn pdf_pixels(image: image::DynamicImage, color_type: image::ColorType) -> Result<Vec<u8>> {
    match color_type {
        image::ColorType::L8 => Ok(image.into_bytes()),
        image::ColorType::La8 => Ok(image.into_luma8().into_raw()),
        image::ColorType::Rgb8 => Ok(image.into_bytes()),
        image::ColorType::Rgba8 => Ok(image.into_rgb8().into_raw()),
        image::ColorType::L16 | image::ColorType::La16 => {
            Ok(be_bytes(image.into_luma16().into_raw()))
        }
        image::ColorType::Rgb16 | image::ColorType::Rgba16 => {
            Ok(be_bytes(image.into_rgb16().into_raw()))
        }
        image::ColorType::Rgb32F | image::ColorType::Rgba32F => {
            anyhow::bail!("image_to_pdf: floating-point images are not supported")
        }
        _ => anyhow::bail!("image_to_pdf: unsupported image color type"),
    }
}

fn be_bytes(samples: Vec<u16>) -> Vec<u8> {
    samples.into_iter().flat_map(u16::to_be_bytes).collect()
}
