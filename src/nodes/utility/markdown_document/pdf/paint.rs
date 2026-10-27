use super::{
    super::{assets::Picture, config::Config},
    Draw,
};
use crate::util::execution::ExecutionControl;
use anyhow::Result;
use krilla::{
    Document,
    color::rgb::Color,
    geom::{PathBuilder, Point, Rect, Size, Transform},
    image::Image,
    metadata::Metadata,
    num::NormalizedF32,
    page::PageSettings,
    paint::{Fill, Stroke},
    surface::Surface,
    tagging::{ContentTag, SpanTag, Tag, TagGroup, TagTree},
};
use std::collections::BTreeMap;

pub(super) fn paint(
    config: &Config,
    pages: &[Vec<Draw>],
    pictures: &BTreeMap<String, Picture>,
    execution: &ExecutionControl,
) -> Result<Vec<u8>> {
    let mut pdf = Document::new();
    let mut tree = TagTree::new().with_lang(Some(config.language.clone()));
    pdf.set_metadata(
        Metadata::new()
            .title(config.title.clone())
            .authors(vec![config.author.clone()])
            .language(config.language.clone())
            .creator("IronFlow".to_owned()),
    );
    let images: BTreeMap<_, _> = pictures
        .iter()
        .map(|(key, picture)| {
            Image::from_png(picture.png.clone().into(), true)
                .map(|image| (key, image))
                .map_err(anyhow::Error::msg)
        })
        .collect::<Result<_>>()?;
    for draws in pages {
        execution.checkpoint()?;
        let mut page = pdf.start_page_with(
            PageSettings::from_wh(config.width, config.height)
                .ok_or_else(|| anyhow::anyhow!("invalid page size"))?,
        );
        let mut surface = page.surface();
        for draw in draws {
            execution.checkpoint()?;
            match draw {
                Draw::BeginText(text) => {
                    let id = surface.start_tagged(ContentTag::Span(
                        SpanTag::empty().with_actual_text(Some(text)),
                    ));
                    tree.push(TagGroup::with_children(Tag::P, vec![id.into()]));
                }
                Draw::EndText => surface.end_tagged(),
                Draw::Text(text) => {
                    surface.set_stroke(None);
                    surface.set_fill(Some(Fill {
                        paint: Color::new(0, 0, 0).into(),
                        opacity: NormalizedF32::ONE,
                        ..Default::default()
                    }));
                    surface.push_transform(&Transform::from_row(
                        1.0,
                        0.0,
                        -text.skew.to_radians().tan(),
                        1.0,
                        text.x,
                        text.baseline,
                    ));
                    surface.draw_glyphs(
                        Point::from_xy(0.0, 0.0),
                        &text.glyphs,
                        text.font.clone(),
                        &text.source,
                        text.size,
                        false,
                    );
                    surface.pop();
                    if text.strike {
                        let mut path = PathBuilder::new();
                        path.move_to(text.x, text.baseline - text.size * 0.3);
                        path.line_to(text.x + text.advance, text.baseline - text.size * 0.3);
                        if let Some(path) = path.finish() {
                            line(&mut surface, &path);
                        }
                    }
                }
                Draw::Rect {
                    x,
                    y,
                    width,
                    height,
                } => {
                    let mut path = PathBuilder::new();
                    if let Some(rect) = Rect::from_xywh(*x, *y, *width, *height) {
                        path.push_rect(rect);
                    }
                    if let Some(path) = path.finish() {
                        line(&mut surface, &path);
                    }
                }
                Draw::Image { key, y } => {
                    let picture = &pictures[key];
                    surface.push_transform(&Transform::from_translate(config.margin, *y));
                    surface.draw_image(
                        images[key].clone(),
                        Size::from_wh(picture.width, picture.height)
                            .ok_or_else(|| anyhow::anyhow!("invalid image size"))?,
                    );
                    surface.pop();
                }
                Draw::Rule(y) => {
                    let mut path = PathBuilder::new();
                    path.move_to(config.margin, *y);
                    path.line_to(config.width - config.margin, *y);
                    if let Some(path) = path.finish() {
                        line(&mut surface, &path);
                    }
                }
            }
        }
        surface.finish();
        page.finish();
    }
    execution.checkpoint()?;
    pdf.set_tag_tree(tree);
    pdf.finish().map_err(anyhow::Error::from)
}

fn line(surface: &mut Surface<'_>, path: &krilla::geom::Path) {
    surface.set_fill(None);
    surface.set_stroke(Some(Stroke {
        paint: Color::new(150, 150, 150).into(),
        width: 0.6,
        ..Default::default()
    }));
    surface.draw_path(path);
}
