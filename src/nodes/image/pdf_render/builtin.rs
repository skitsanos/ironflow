//! Built-in pure-Rust renderer: no native library, bounded before it draws.

use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};

use anyhow::Result;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_syntax::{LoadPdfError, Pdf};
use hayro::kurbo::{Affine, Rect};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::peniko::ImageAlphaType;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{RenderCache, RenderSettings, render_into};

use super::meter;
use super::preflight::StreamBudget;
use crate::artifacts::FileSource;
use crate::engine::types::NodeOutput;
use crate::nodes::image::common::open_pdf_file_capped;
use crate::util::execution::ExecutionControl;

/// One opened document that renders pages into straight-alpha RGBA.
pub(super) struct Document<'a> {
    operation: &'static str,
    pdf: &'a Pdf,
    cache: &'a RenderCache<'a>,
    document: lopdf::Document,
    pages: Vec<lopdf::ObjectId>,
    streams: StreamBudget,
    limits: meter::Limits,
}

pub(super) fn with_document(
    source: &FileSource,
    operation: &'static str,
    execution: &ExecutionControl,
    work: impl FnOnce(&mut Document<'_>) -> Result<NodeOutput>,
) -> Result<NodeOutput> {
    let bytes = read_capped(source, operation, execution)?;
    // The shared loading policy rejects encrypted input, oversized object
    // streams and excessive object counts with the errors every PDF node uses.
    let document = crate::util::pdf_load::from_bytes(&bytes, operation, execution)?;
    let pages: Vec<_> = document.get_pages().into_values().collect();
    let pdf = Pdf::new(bytes).map_err(|error| match error {
        LoadPdfError::Decryption(_) => anyhow::anyhow!(
            "{operation}: PDF requires a password; password-protected input is unsupported"
        ),
        LoadPdfError::Invalid => anyhow::anyhow!("{operation}: failed to parse PDF"),
    })?;
    if pdf.pages().len() != pages.len() {
        anyhow::bail!("{operation}: PDF page tree is inconsistent");
    }
    execution.checkpoint()?;
    let cache = RenderCache::new();
    work(&mut Document {
        operation,
        pdf: &pdf,
        cache: &cache,
        document,
        pages,
        streams: StreamBudget::current(),
        limits: meter::Limits::current(),
    })
}

fn read_capped(
    source: &FileSource,
    operation: &str,
    execution: &ExecutionControl,
) -> Result<Vec<u8>> {
    let mut file = open_pdf_file_capped(source, operation, execution)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| anyhow::anyhow!("{operation}: failed to read PDF: {error}"))?;
    Ok(bytes)
}

impl Document<'_> {
    pub(super) fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Page size in PDF points, after page rotation.
    pub(super) fn page_points(&self, index: usize) -> Result<(f32, f32)> {
        Ok(self.pdf.pages()[index].render_dimensions())
    }

    /// Render one page at exactly `width` by `height` pixels. The caller has
    /// already admitted the pixel count.
    pub(super) fn render(
        &mut self,
        index: usize,
        width: u32,
        height: u32,
        execution: &ExecutionControl,
    ) -> Result<image::RgbaImage> {
        let (operation, number) = (self.operation, index + 1);
        let (Ok(target_width), Ok(target_height)) = (u16::try_from(width), u16::try_from(height))
        else {
            anyhow::bail!(
                "{operation}: render target {width}x{height} exceeds the built-in renderer's \
                 maximum side of {} pixels",
                u16::MAX
            );
        };
        self.streams
            .admit_page(&self.document, self.pages[index], operation, execution)?;
        let page = &self.pdf.pages()[index];
        let (points_width, points_height) = page.render_dimensions();
        let transform = Affine::scale_non_uniform(
            f64::from(width) / f64::from(points_width),
            f64::from(height) / f64::from(points_height),
        ) * page.initial_transform(true).to_kurbo();
        let target = Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
        meter::admit(
            page,
            transform,
            target,
            &self.limits,
            operation,
            number,
            execution,
        )?;
        let cache = self.cache;
        // A renderer defect on hostile input must fail this step, not unwind
        // through the worker.
        let rgba = catch_unwind(AssertUnwindSafe(|| {
            let mut context = RenderContext::new(target_width, target_height);
            render_into(
                page,
                cache,
                &InterpreterSettings::default(),
                &RenderSettings::default(),
                &mut context,
                transform,
            );
            context.flush();
            let mut pixmap = Pixmap::new(target_width, target_height);
            context.render_with(
                &mut pixmap,
                &mut Resources::default(),
                RasterizerSettings {
                    target_init: TargetInit::Clear(WHITE),
                    ..Default::default()
                },
            );
            pixmap.take_rgba8(ImageAlphaType::Alpha)
        }))
        .map_err(|_| anyhow::anyhow!("{operation}: the renderer failed on page {number}"))?;
        execution.checkpoint()?;
        image::RgbaImage::from_raw(width, height, rgba).ok_or_else(|| {
            anyhow::anyhow!("{operation}: the renderer returned a malformed page {number}")
        })
    }
}
