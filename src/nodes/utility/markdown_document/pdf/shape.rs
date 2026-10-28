use super::super::{
    config::Config,
    language::{Direction, Face},
    model::Run,
};
use crate::util::execution::ExecutionControl;
use anyhow::{Result, ensure};
use krilla::text::{Font, GlyphId, KrillaGlyph, Tag};
use parley::{
    Alignment, BaseDirection, FontContext, FontStyle, FontWeight, LayoutContext, LineHeight,
    OverflowWrap, StyleProperty,
};
use std::{collections::HashMap, ops::Range, sync::Arc};

pub(super) struct Text {
    pub font: Font,
    pub source: Arc<str>,
    pub glyphs: Vec<KrillaGlyph>,
    pub size: f32,
    pub x: f32,
    pub baseline: f32,
    pub advance: f32,
    pub strike: bool,
    pub skew: f32,
}

#[cfg(test)]
mod tests;

pub(super) struct Line {
    pub source: Arc<str>,
    pub range: Range<usize>,
    pub runs: Vec<Text>,
    pub height: f32,
}

pub(super) struct Shaper {
    fonts: FontContext,
    cjk_available: bool,
    layouts: LayoutContext<u8>,
    cache: HashMap<(u64, u32, u32), Font>,
    remaining_text: usize,
}

impl Shaper {
    pub fn new() -> Result<Self> {
        let fonts = super::fonts::context()?;
        Ok(Self {
            fonts: fonts.context,
            cjk_available: fonts.cjk_available,
            layouts: LayoutContext::new(),
            cache: HashMap::new(),
            remaining_text: 1024 * 1024,
        })
    }

    #[allow(clippy::expect_used)] // Infallible: ligature start recorded; batch exists.
    pub fn paragraph(
        &mut self,
        runs: &[Run],
        width: f32,
        size: f32,
        config: &Config,
        execution: &ExecutionControl,
    ) -> Result<Vec<Line>> {
        execution.checkpoint()?;
        let mut text = String::new();
        let mut spans = Vec::new();
        for (index, run) in runs.iter().enumerate() {
            let start = text.len();
            let expanded = run.text.len().saturating_add(
                run.text
                    .bytes()
                    .filter(|&b| b == b'\t')
                    .count()
                    .saturating_mul(3),
            );
            ensure!(
                start.saturating_add(expanded) <= self.remaining_text,
                "PDF expanded layout text exceeds 1 MiB"
            );
            text.push_str(&run.text.replace('\t', "    "));
            spans.push((start..text.len(), &run.style));
            if let Some(target) = &run.style.link
                && runs.get(index + 1).and_then(|r| r.style.link.as_ref()) != Some(target)
            {
                ensure!(
                    text.len().saturating_add(target.len()).saturating_add(3)
                        <= self.remaining_text,
                    "PDF expanded layout text exceeds 1 MiB"
                );
                text.push_str(&format!(" ({target})"));
            }
        }
        self.remaining_text -= text.len();
        // Fail before layout with a pointer to the remedy instead of a generic
        // missing-glyph error when no registered font covers CJK text.
        if !self.cjk_available && text.chars().any(|c| Face::of(c) == Some(Face::Cjk)) {
            anyhow::bail!(
                "markdown_to_pdf: the document contains CJK text but no CJK font is available; \
                 build IronFlow with the `cjk-fonts` feature or set {} to a directory \
                 containing a CJK font",
                super::fonts::FONT_DIR_ENV
            );
        }
        let source: Arc<str> = text.into();
        let mut builder = self
            .layouts
            .ranged_builder(&mut self.fonts, &source, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(
            "'Noto Serif', 'Noto Sans Arabic', 'Noto Sans Hebrew', 'Noto Sans CJK SC'".into(),
        ));
        builder.push_default(StyleProperty::FontSize(size));
        builder.push_default(StyleProperty::Brush(0));
        builder.push_default(StyleProperty::LineHeight(LineHeight::MetricsRelative(1.1)));
        builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        builder.push_default(StyleProperty::Locale(
            parley::fontique::Language::parse(&config.language).ok(),
        ));
        let direction = if runs.iter().all(|r| r.style.code) {
            Direction::Ltr
        } else {
            config.direction
        };
        builder.set_base_direction(match direction {
            Direction::Auto => BaseDirection::Auto,
            Direction::Ltr => BaseDirection::Ltr,
            Direction::Rtl => BaseDirection::Rtl,
        });
        for (range, style) in spans {
            if style.bold {
                builder.push(
                    StyleProperty::FontWeight(FontWeight::new(700.0)),
                    range.clone(),
                );
            }
            if style.italic {
                builder.push(StyleProperty::FontStyle(FontStyle::Italic), range.clone());
            }
            if style.code {
                builder.push(StyleProperty::FontFamily("'Noto Sans Mono', 'Noto Sans Arabic', 'Noto Sans Hebrew', 'Noto Sans CJK SC'".into()), range.clone());
            }
            builder.push(StyleProperty::Brush(u8::from(style.strike)), range);
        }
        let cjk_language = Face::Cjk.locale(&source, &config.language);
        if source.chars().any(|c| Face::of(c) == Some(Face::Cjk)) {
            builder.push_default(StyleProperty::Locale(
                parley::fontique::Language::parse(cjk_language).ok(),
            ));
        }
        let mut layout = builder.build(&source);
        execution.checkpoint()?;
        layout.break_all_lines(Some(width));
        layout.align(Alignment::Start, Default::default());
        let mut lines = Vec::new();
        for line in layout.lines() {
            execution.checkpoint()?;
            let metrics = line.metrics();
            let mut output = Line {
                source: source.clone(),
                range: line.text_range(),
                height: metrics.line_height,
                runs: Vec::new(),
            };
            let mut x = metrics.offset;
            for run in line.runs() {
                let data = run.font();
                let weight = run.font_attrs().weight.value();
                let key = (data.data.id(), data.index, weight.to_bits());
                let font = if let Some(font) = self.cache.get(&key) {
                    font.clone()
                } else {
                    let (bytes, _) = data.data.clone().into_raw_parts();
                    let font = Font::new_variable(
                        bytes.into(),
                        data.index,
                        &[(Tag::new(b"wght"), weight)],
                    )
                    .ok_or_else(|| anyhow::anyhow!("cannot load shaped font"))?;
                    self.cache.insert(key, font.clone());
                    font
                };
                let mut ranges: HashMap<usize, Range<usize>> = HashMap::new();
                let mut previous = None;
                for cluster in run.clusters() {
                    let range = cluster.text_range();
                    if cluster.is_ligature_continuation() {
                        if let Some(start) = previous {
                            ranges.get_mut(&start).expect("ligature start recorded").end =
                                range.end;
                        }
                    } else {
                        previous = Some(range.start);
                        ranges.insert(range.start, range);
                    }
                }
                let mut batch: Option<Text> = None;
                for cluster in run.visual_clusters() {
                    if cluster.is_ligature_continuation() {
                        continue;
                    }
                    let start = cluster.text_range().start;
                    let range = ranges
                        .get(&start)
                        .cloned()
                        .unwrap_or_else(|| cluster.text_range());
                    let mut glyphs = Vec::new();
                    let mut advance = 0.0;
                    for glyph in cluster.glyphs() {
                        ensure!(
                            glyph.id != 0,
                            "PDF contains a character missing from the available fonts"
                        );
                        glyphs.push(KrillaGlyph::new(
                            GlyphId::new(glyph.id),
                            glyph.advance / run.font_size(),
                            glyph.x / run.font_size(),
                            glyph.y / run.font_size(),
                            0.0,
                            range.clone(),
                            None,
                        ));
                        advance += glyph.advance;
                    }
                    if !glyphs.is_empty() {
                        let strike = cluster.style().brush == 1;
                        if batch.as_ref().is_some_and(|text| text.strike != strike) {
                            output.runs.push(batch.take().expect("batch exists"));
                        }
                        let text = batch.get_or_insert_with(|| Text {
                            font: font.clone(),
                            source: source.clone(),
                            glyphs: Vec::new(),
                            size: run.font_size(),
                            x,
                            baseline: metrics.baseline - metrics.block_min_coord,
                            advance: 0.0,
                            strike,
                            skew: run.synthesis().skew().unwrap_or(0.0),
                        });
                        text.glyphs.extend(glyphs);
                        text.advance += advance;
                    }
                    x += advance;
                }
                if let Some(text) = batch {
                    output.runs.push(text);
                }
            }
            lines.push(output);
        }
        Ok(lines)
    }
}
