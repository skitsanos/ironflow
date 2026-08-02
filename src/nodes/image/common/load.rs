use std::io::{BufRead, Seek};

use anyhow::Result;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use crate::util::execution::ExecutionControl;

use super::super::image_sources::{
    ImageInput, LoadedImage, LoadedImageBytes, preflight_base64_bytes,
};
use super::super::resource::{ImageDecodeLimits, validate_image_shape};
use super::capped_reader::CappedFile;

#[derive(Debug)]
pub(crate) struct ImageInfo {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: ImageFormat,
    pub(crate) color_type: image::ColorType,
    pub(crate) total_bytes: u64,
    pub(crate) pixels: u64,
}

pub(crate) fn load_image(
    input: ImageInput,
    limits: ImageDecodeLimits,
    execution: &ExecutionControl,
) -> Result<LoadedImage> {
    execution.checkpoint()?;
    let image = match input {
        ImageInput::File(source) => {
            let (reader, label) = open_image_reader(&source, limits, execution)?;
            decode_reader(reader, &label, limits, execution)?
        }
        ImageInput::Base64(data) => {
            let bytes = decode_image_base64(data, limits.max_encoded_bytes)?;
            decode_image_bytes(&bytes, "base64 image", limits, execution)?
        }
    };
    execution.checkpoint()?;
    Ok(LoadedImage { image })
}

pub(crate) fn load_image_for_pdf(
    input: ImageInput,
    limits: ImageDecodeLimits,
    max_encoded_bytes: u64,
    execution: &ExecutionControl,
) -> Result<LoadedImageBytes> {
    execution.checkpoint()?;
    let limit = limits.max_encoded_bytes.min(max_encoded_bytes);
    let (label, bytes) = match input {
        ImageInput::File(source) => {
            let opened = source.open("image_to_pdf", execution)?;
            let (file, label) = opened.into_parts();
            let reader = CappedFile::from_file(
                file,
                label.clone(),
                limit,
                "IRONFLOW_MAX_IMAGE_ENCODED_BYTES",
                execution,
            )?;
            let bytes = crate::util::bounded_read::read_capped(reader, limit, "image_to_pdf")?;
            (label, bytes)
        }
        ImageInput::Base64(data) => ("base64 image".to_owned(), decode_image_base64(data, limit)?),
    };
    execution.checkpoint()?;
    let info = inspect_reader(
        ImageReader::new(std::io::Cursor::new(bytes.as_slice())),
        &label,
        limits,
    )?;
    execution.checkpoint()?;
    Ok(LoadedImageBytes {
        label,
        bytes,
        width: info.width,
        height: info.height,
        format: info.format,
        color_type: info.color_type,
        total_bytes: info.total_bytes,
        pixels: info.pixels,
    })
}

pub(crate) fn inspect_image(
    input: ImageInput,
    limits: ImageDecodeLimits,
    execution: &ExecutionControl,
) -> Result<ImageInfo> {
    execution.checkpoint()?;
    let info = match input {
        ImageInput::File(source) => {
            let (reader, label) = open_image_reader(&source, limits, execution)?;
            inspect_reader(reader, &label, limits)?
        }
        ImageInput::Base64(data) => {
            let bytes = decode_image_base64(data, limits.max_encoded_bytes)?;
            inspect_reader(
                ImageReader::new(std::io::Cursor::new(bytes.as_slice())),
                "base64 image",
                limits,
            )?
        }
    };
    execution.checkpoint()?;
    Ok(info)
}

pub(crate) fn decode_image_bytes(
    bytes: &[u8],
    label: &str,
    limits: ImageDecodeLimits,
    execution: &ExecutionControl,
) -> Result<DynamicImage> {
    let reader = ImageReader::new(std::io::Cursor::new(bytes));
    decode_reader(reader, label, limits, execution)
}

fn decode_reader<R>(
    reader: ImageReader<R>,
    label: &str,
    limits: ImageDecodeLimits,
    execution: &ExecutionControl,
) -> Result<DynamicImage>
where
    R: BufRead + Seek,
{
    execution.checkpoint()?;
    let mut reader = reader
        .with_guessed_format()
        .map_err(|error| anyhow::anyhow!("failed to identify image '{label}': {error}"))?;
    reader.limits(limits.decoder_limits());
    let decoder = reader
        .into_decoder()
        .map_err(|error| anyhow::anyhow!("invalid image data for '{label}': {error}"))?;
    inspect_decoder(&decoder, label, limits)?;
    execution.checkpoint()?;
    let image = DynamicImage::from_decoder(decoder)
        .map_err(|error| anyhow::anyhow!("invalid image data for '{label}': {error}"))?;
    execution.checkpoint()?;
    Ok(image)
}

fn inspect_reader<R>(
    reader: ImageReader<R>,
    label: &str,
    limits: ImageDecodeLimits,
) -> Result<ImageInfo>
where
    R: BufRead + Seek,
{
    let mut reader = reader
        .with_guessed_format()
        .map_err(|error| anyhow::anyhow!("failed to identify image '{label}': {error}"))?;
    let format = reader
        .format()
        .ok_or_else(|| anyhow::anyhow!("unsupported image format for '{label}'"))?;
    reader.limits(limits.decoder_limits());
    let decoder = reader
        .into_decoder()
        .map_err(|error| anyhow::anyhow!("invalid image data for '{label}': {error}"))?;
    let (width, height, color_type, total_bytes, pixels) =
        inspect_decoder(&decoder, label, limits)?;
    Ok(ImageInfo {
        width,
        height,
        format,
        color_type,
        total_bytes,
        pixels,
    })
}

fn inspect_decoder(
    decoder: &impl ImageDecoder,
    label: &str,
    limits: ImageDecodeLimits,
) -> Result<(u32, u32, image::ColorType, u64, u64)> {
    let (width, height) = decoder.dimensions();
    let color_type = decoder.color_type();
    let total_bytes = decoder.total_bytes();
    let pixels = validate_image_shape("image decode", label, width, height, total_bytes, limits)?;
    Ok((width, height, color_type, total_bytes, pixels))
}

fn open_image_reader(
    source: &crate::artifacts::FileSource,
    limits: ImageDecodeLimits,
    execution: &ExecutionControl,
) -> Result<(ImageReader<std::io::BufReader<CappedFile>>, String)> {
    let opened = source.open("image input", execution)?;
    let (file, label) = opened.into_parts();
    let file = CappedFile::from_file(
        file,
        label.clone(),
        limits.max_encoded_bytes,
        "IRONFLOW_MAX_IMAGE_ENCODED_BYTES",
        execution,
    )?;
    Ok((ImageReader::new(std::io::BufReader::new(file)), label))
}

fn decode_image_base64(data: String, limit: u64) -> Result<Vec<u8>> {
    use base64::Engine;
    preflight_base64_bytes(&data, limit)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| anyhow::anyhow!("failed to decode base64 image data: {error}"))?;
    if bytes.len() as u64 > limit {
        anyhow::bail!(
            "base64 image exceeds IRONFLOW_MAX_IMAGE_ENCODED_BYTES ({limit} decoded bytes)"
        );
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(max_encoded_bytes: u64, max_pixels: u64) -> ImageDecodeLimits {
        ImageDecodeLimits {
            max_encoded_bytes,
            max_pixels,
            max_allocation_bytes: 1_024,
        }
    }

    #[tokio::test]
    async fn rejects_dimensions_before_decoding_pixels() {
        let mut png = Vec::new();
        image::DynamicImage::new_rgba8(3, 3)
            .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        let error = crate::util::execution::run_tracked_blocking_step(move |execution| {
            decode_image_bytes(&png, "test", limits(1_024, 8), &execution)
        })
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("IRONFLOW_MAX_IMAGE_PIXELS"));
    }

    #[test]
    fn rejects_base64_before_decoding_unbounded_output() {
        let error = decode_image_base64("A".repeat(100), 4)
            .unwrap_err()
            .to_string();
        assert!(error.contains("IRONFLOW_MAX_IMAGE_ENCODED_BYTES"));
    }
}
