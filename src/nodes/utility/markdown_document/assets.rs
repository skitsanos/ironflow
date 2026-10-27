use super::{
    config::Config,
    model::{Block, MAX_OUTPUT},
};
use crate::{
    nodes::image::{
        common::{decode_image_bytes, load_image_for_pdf},
        image_sources::ImageInput,
        resource::ImageDecodeLimits,
    },
    util::execution::ExecutionControl,
};
use anyhow::{Result, ensure};
use std::{collections::BTreeMap, io::Cursor};

pub(super) struct Picture {
    pub png: Vec<u8>,
    pub width: f32,
    pub height: f32,
}

pub(super) fn load(
    config: &Config,
    blocks: &[Block],
    execution: &ExecutionControl,
) -> Result<BTreeMap<String, Picture>> {
    let mut pictures = BTreeMap::new();
    let mut encoded = 0u64;
    let mut pixels = 0u64;
    let mut normalized = 0usize;
    let limits = ImageDecodeLimits::current();
    for block in blocks {
        let Block::Image { key, .. } = block else {
            continue;
        };
        if pictures.contains_key(key) {
            continue;
        }
        let source = config.images.get(key).ok_or_else(|| {
            anyhow::anyhow!(
                "image '{key}' requires an explicit images binding; remote fetching is disabled"
            )
        })?;
        let loaded = load_image_for_pdf(
            ImageInput::File(source.clone()),
            limits,
            (16 * 1024 * 1024u64).saturating_sub(encoded),
            execution,
        )?;
        encoded += loaded.bytes.len() as u64;
        pixels = pixels.saturating_add(loaded.pixels);
        ensure!(
            pixels <= 8_000_000,
            "document images exceed 8 million decoded pixels"
        );
        let image = decode_image_bytes(&loaded.bytes, &loaded.label, limits, execution)?;
        let mut output = Cursor::new(Vec::new());
        image.write_to(&mut output, image::ImageFormat::Png)?;
        let png = output.into_inner();
        normalized = normalized.saturating_add(png.len());
        ensure!(
            normalized <= MAX_OUTPUT / 2,
            "normalized document images exceed 16 MiB"
        );
        let scale = ((config.width - config.margin * 2.0) / loaded.width as f32)
            .min((config.height - config.margin * 2.0 - 24.0) / loaded.height as f32)
            .min(0.75);
        pictures.insert(
            key.clone(),
            Picture {
                png,
                width: loaded.width as f32 * scale,
                height: loaded.height as f32 * scale,
            },
        );
        execution.checkpoint()?;
    }
    Ok(pictures)
}
