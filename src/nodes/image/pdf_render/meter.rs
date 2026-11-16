//! Work admission for the built-in renderer.
//!
//! The rasterizer exposes no cancellation or work limit, and a small PDF can
//! request unbounded drawing through nested forms or dense tiling patterns.
//! Each page is therefore interpreted once against a device that draws
//! nothing: it counts the operations the rasterizer would execute, observes
//! cancellation, and stops interpretation as soon as a limit is crossed.

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use anyhow::Result;
use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::pattern::Pattern;
use hayro::hayro_interpret::{
    BlendMode, CacheKey, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, Paint, SoftMask, interpret_page,
};
use hayro::hayro_syntax::page::Page;
use hayro::kurbo::{Affine, BezPath, Rect};

use crate::util::execution::ExecutionControl;

/// Operations between cancellation checks.
const CHECKPOINT_INTERVAL: u64 = 256;
/// Image pixels charged as one operation.
const PIXELS_PER_OPERATION: u64 = 1024;

pub(super) struct Limits {
    pub(super) operations: u64,
    pub(super) image_pixels: u64,
}

impl Limits {
    pub(super) fn current() -> Self {
        Self {
            operations: crate::util::limits::max_pdf_render_operations(),
            image_pixels: crate::util::limits::max_image_pixels(),
        }
    }
}

/// Why interpretation stopped early. Carried out of the interpreter by
/// unwinding, because its device callbacks cannot return an error.
enum Stop {
    Operations,
    Image { width: u32, height: u32 },
    Execution(anyhow::Error),
}

struct Meter<'e> {
    used: u64,
    next_checkpoint: u64,
    limits: &'e Limits,
    masks: HashSet<u128>,
    execution: &'e ExecutionControl,
}

impl Meter<'_> {
    fn charge(&mut self, amount: u64) {
        self.used = self.used.saturating_add(amount);
        if self.used > self.limits.operations {
            resume_unwind(Box::new(Stop::Operations));
        }
        if self.used >= self.next_checkpoint {
            self.next_checkpoint = self.used.saturating_add(CHECKPOINT_INTERVAL);
            if let Err(error) = self.execution.checkpoint() {
                resume_unwind(Box::new(Stop::Execution(error)));
            }
        }
    }

    /// The rasterizer draws each distinct soft mask once per page.
    fn mask<'a>(&mut self, mask: Option<&SoftMask<'a>>) {
        if let Some(mask) = mask
            && self.masks.insert(mask.cache_key())
        {
            mask.interpret(self);
        }
    }

    /// A tiling pattern is interpreted once per tile copy; charge every copy.
    fn paint<'a>(&mut self, props: &DrawProps<'a>, mode: &DrawMode) {
        self.mask(props.soft_mask.as_ref());
        let Paint::Pattern(pattern) = &props.paint else {
            return;
        };
        let Pattern::Tiling(tiling) = pattern.as_ref() else {
            return;
        };
        let before = self.used;
        tiling.interpret(self, Affine::IDENTITY, matches!(mode, DrawMode::Stroke(_)));
        let cell = (self.used - before).max(1);
        let copies = tile_copies(tiling.bbox.width(), tiling.x_step)
            .saturating_mul(tile_copies(tiling.bbox.height(), tiling.y_step));
        self.charge(cell.saturating_mul(copies.saturating_sub(1)));
    }
}

/// Upper bound on the copies the rasterizer draws along one axis.
fn tile_copies(extent: f64, step: f32) -> u64 {
    let copies = (extent.abs() / f64::from(step.abs())).ceil() + 2.0;
    if copies.is_finite() {
        copies.min(u64::MAX as f64) as u64
    } else {
        u64::MAX
    }
}

impl<'a> Device<'a> for Meter<'_> {
    fn draw_path(&mut self, _: &BezPath, props: DrawProps<'a>, mode: &DrawMode) {
        self.charge(1);
        self.paint(&props, mode);
    }

    fn push_clip_path(&mut self, _: &ClipPath) {
        self.charge(1);
    }

    fn push_transparency_group(&mut self, _: f32, mask: Option<SoftMask<'a>>, _: BlendMode) {
        self.charge(1);
        self.mask(mask.as_ref());
    }

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        self.paint(&props, mode);
        for positioned in run.glyphs() {
            self.charge(1);
            if let Glyph::Type3(glyph) = &**positioned {
                glyph.interpret(self, props.transform, positioned.transform(), &props.paint);
            }
        }
    }

    fn draw_image(&mut self, image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        let (width, height) = match &image {
            Image::Stencil(stencil) => (stencil.width(), stencil.height()),
            Image::Raster(raster) => (raster.width(), raster.height()),
        };
        let pixels = u64::from(width).saturating_mul(u64::from(height));
        if pixels > self.limits.image_pixels {
            resume_unwind(Box::new(Stop::Image { width, height }));
        }
        self.charge(1 + pixels / PIXELS_PER_OPERATION);
        self.mask(props.soft_mask.as_ref());
    }

    fn pop_clip(&mut self) {}

    fn pop_transparency_group(&mut self) {}
}

/// Admit one page for rasterization with the transform and target the
/// rasterizer will use.
pub(super) fn admit(
    page: &Page<'_>,
    transform: Affine,
    target: Rect,
    limits: &Limits,
    operation: &str,
    page_number: usize,
    execution: &ExecutionControl,
) -> Result<()> {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let cache = InterpreterCache::new();
        let mut context = Context::new(
            transform,
            target,
            &cache,
            page.xref(),
            InterpreterSettings::default(),
        );
        let mut meter = Meter {
            used: 0,
            next_checkpoint: CHECKPOINT_INTERVAL,
            limits,
            masks: HashSet::new(),
            execution,
        };
        interpret_page(page, &mut context, &mut meter);
    }));
    let Err(payload) = outcome else {
        return execution.checkpoint();
    };
    match payload.downcast::<Stop>() {
        Ok(stop) => Err(match *stop {
            Stop::Operations => anyhow::anyhow!(
                "{operation}: page {page_number} needs more than {} drawing operations \
                 (set IRONFLOW_MAX_PDF_RENDER_OPERATIONS to raise)",
                limits.operations
            ),
            Stop::Image { width, height } => anyhow::anyhow!(
                "{operation}: page {page_number} embeds a {width}x{height} image, exceeds \
                 IRONFLOW_MAX_IMAGE_PIXELS ({})",
                limits.image_pixels
            ),
            Stop::Execution(error) => error,
        }),
        Err(_) => Err(anyhow::anyhow!(
            "{operation}: the renderer could not interpret page {page_number}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::tile_copies;

    #[test]
    fn tile_copies_bound_the_cell_extent_over_the_step_and_never_wrap() {
        assert_eq!(tile_copies(10.0, 10.0), 3);
        assert_eq!(tile_copies(1_000_000.0, -0.5), 2_000_002);
        // The interpreter drops a zero step; a subnormal one must still saturate.
        assert_eq!(tile_copies(1.0, 0.0), u64::MAX);
        assert_eq!(tile_copies(f64::MAX, f32::MIN_POSITIVE), u64::MAX);
    }
}
