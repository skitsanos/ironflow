use super::{
    assets::Picture,
    config::Config,
    model::{Block, MAX_OUTPUT, Run, Style},
};
use crate::util::execution::ExecutionControl;
use anyhow::{Result, ensure};
use shape::{Line, Shaper, Text};
use std::collections::BTreeMap;

mod actual_text;
mod fonts;
mod paint;
mod shape;
mod table;

enum Draw {
    BeginText(String),
    EndText,
    Text(Text),
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Image {
        key: String,
        y: f32,
    },
    Rule(f32),
}

struct Planner<'a> {
    config: &'a Config,
    execution: &'a ExecutionControl,
    shaper: Shaper,
    pages: Vec<Vec<Draw>>,
    y: f32,
}

impl Planner<'_> {
    fn bottom(&self) -> f32 {
        self.config.height - self.config.margin
    }
    fn width(&self) -> f32 {
        self.config.width - self.config.margin * 2.0
    }
    fn new_page(&mut self) -> Result<()> {
        self.execution.checkpoint()?;
        ensure!(self.pages.len() < 250, "PDF exceeds 250 pages");
        self.pages.push(Vec::new());
        self.y = self.config.margin;
        Ok(())
    }
    fn ensure(&mut self, height: f32) -> Result<()> {
        ensure!(
            height <= self.bottom() - self.config.margin + 0.01,
            "PDF element exceeds page height"
        );
        if self.y + height > self.bottom() + 0.01 {
            self.new_page()?;
        }
        Ok(())
    }
    #[allow(clippy::expect_used)] // Infallible: one initial page.
    fn push(&mut self, draw: Draw) {
        self.pages.last_mut().expect("one initial page").push(draw);
    }
    fn line(&mut self, line: Line, x: f32, y: f32) {
        self.push(Draw::BeginText(line.source[line.range].to_owned()));
        for mut text in line.runs {
            text.x += x;
            text.baseline += y;
            self.push(Draw::Text(text));
        }
        self.push(Draw::EndText);
    }
    fn paragraph(&mut self, runs: &[Run], indent: f32, size: f32, heading: bool) -> Result<()> {
        let lines = self.shaper.paragraph(
            runs,
            self.width() - indent,
            size,
            self.config,
            self.execution,
        )?;
        if heading {
            self.y += if self.y > self.config.margin {
                10.0
            } else {
                0.0
            };
            if let Some(line) = lines.first() {
                self.ensure(
                    (line.height + self.config.font_size * 1.5)
                        .min(self.bottom() - self.config.margin),
                )?;
            }
        }
        for line in lines {
            self.execution.checkpoint()?;
            self.ensure(line.height)?;
            let height = line.height;
            self.line(line, self.config.margin + indent, self.y);
            self.y += height;
        }
        self.y += 6.0;
        Ok(())
    }
}

pub(super) fn render(
    config: &Config,
    blocks: &[Block],
    pictures: &BTreeMap<String, Picture>,
    execution: &ExecutionControl,
) -> Result<(Vec<u8>, usize)> {
    let mut plan = Planner {
        config,
        execution,
        shaper: Shaper::new()?,
        pages: vec![Vec::new()],
        y: config.margin,
    };
    for block in blocks {
        execution.checkpoint()?;
        match block {
            Block::Heading(level, runs) => {
                let size = config.font_size
                    * [2.0, 1.5, 1.25, 1.15, 1.1, 1.1][(*level as usize - 1).min(5)];
                let runs: Vec<_> = runs
                    .iter()
                    .map(|run| {
                        let mut run = run.clone();
                        run.style.bold = true;
                        run
                    })
                    .collect();
                plan.paragraph(&runs, 0.0, size, true)?;
            }
            Block::Paragraph { runs, depth, label } => {
                let mut runs = runs.clone();
                if !label.is_empty() {
                    runs.insert(
                        0,
                        Run {
                            text: label.clone(),
                            style: Style::default(),
                        },
                    );
                }
                plan.paragraph(
                    &runs,
                    (*depth as f32 * 12.0).min(120.0),
                    config.font_size,
                    false,
                )?;
            }
            Block::Code(code) => plan.paragraph(
                &[Run {
                    text: code.trim_end_matches('\n').to_owned(),
                    style: Style {
                        code: true,
                        ..Default::default()
                    },
                }],
                0.0,
                config.font_size,
                false,
            )?,
            Block::Table(rows) => table::layout(&mut plan, rows)?,
            Block::Image { key, .. } => {
                plan.ensure(pictures[key].height)?;
                plan.push(Draw::Image {
                    key: key.clone(),
                    y: plan.y,
                });
                plan.y += pictures[key].height + 6.0;
            }
            Block::Rule => {
                plan.ensure(8.0)?;
                plan.push(Draw::Rule(plan.y + 3.0));
                plan.y += 8.0;
            }
        }
    }
    let bytes = paint::paint(config, &plan.pages, pictures, execution)?;
    ensure!(bytes.len() <= MAX_OUTPUT, "PDF exceeds 32 MiB output limit");
    let bytes = actual_text::normalize(bytes, execution)?;
    ensure!(bytes.len() <= MAX_OUTPUT, "PDF exceeds 32 MiB output limit");
    Ok((bytes, plan.pages.len()))
}
