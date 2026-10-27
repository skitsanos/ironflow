use super::super::model::Run;
use super::{Draw, Line, Planner};
use anyhow::{Result, ensure};

type Row = Vec<std::collections::VecDeque<Line>>;

pub(super) fn layout(plan: &mut Planner<'_>, rows: &[Vec<Vec<Run>>]) -> Result<()> {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Ok(());
    }
    let width = plan.width() / columns as f32;
    ensure!(
        width >= plan.config.font_size * 2.0 + 12.0,
        "PDF table is too wide for this page/font/margin; use fewer columns, smaller font/margins or DOCX"
    );
    let header = shape_row(plan, &rows[0], width, true)?;
    let header_height = height(&header);
    ensure!(
        header_height + plan.config.font_size * 2.0 < plan.bottom() - plan.config.margin,
        "PDF table header leaves no room for a data row"
    );
    plan.ensure(header_height + plan.config.font_size * 2.0)?;
    let rtl = plan.config.direction.rtl(
        &rows[0]
            .iter()
            .flatten()
            .map(|r| r.text.as_str())
            .collect::<String>(),
    );
    draw_row(plan, header, width, header_height, rtl);
    for row in rows.iter().skip(1) {
        let mut shaped = shape_row(plan, row, width, false)?;
        while shaped.iter().any(|cell| !cell.is_empty()) {
            plan.execution.checkpoint()?;
            let needed = shaped
                .iter()
                .filter_map(|c| c.front())
                .map(|l| l.height)
                .fold(0.0, f32::max)
                + 8.0;
            if plan.y + needed > plan.bottom() {
                plan.new_page()?;
                let header = shape_row(plan, &rows[0], width, true)?;
                draw_row(plan, header, width, header_height, rtl);
            }
            let available = plan.bottom() - plan.y - 8.0;
            let mut fragment = Vec::new();
            for cell in &mut shaped {
                let mut lines = std::collections::VecDeque::new();
                let mut used = 0.0;
                while cell
                    .front()
                    .is_some_and(|line| used + line.height <= available + 0.01)
                {
                    let line = cell.pop_front().expect("front exists");
                    used += line.height;
                    lines.push_back(line);
                }
                fragment.push(lines);
            }
            ensure!(
                fragment.iter().any(|c| !c.is_empty()),
                "table line cannot fit on a PDF page"
            );
            let height = height(&fragment);
            draw_row(plan, fragment, width, height, rtl);
        }
    }
    plan.y += 6.0;
    Ok(())
}

fn shape_row(plan: &mut Planner<'_>, row: &[Vec<Run>], width: f32, bold: bool) -> Result<Row> {
    row.iter()
        .map(|cell| {
            let runs: Vec<_> = cell
                .iter()
                .map(|run| {
                    let mut run = run.clone();
                    run.style.bold |= bold;
                    run
                })
                .collect();
            Ok(plan
                .shaper
                .paragraph(
                    &runs,
                    width - 12.0,
                    plan.config.font_size,
                    plan.config,
                    plan.execution,
                )?
                .into())
        })
        .collect()
}

fn height(row: &Row) -> f32 {
    row.iter()
        .map(|cell| cell.iter().map(|line| line.height).sum::<f32>())
        .fold(0.0, f32::max)
        + 8.0
}

fn draw_row(plan: &mut Planner<'_>, row: Row, width: f32, height: f32, rtl: bool) {
    let count = row.len();
    for (index, cell) in row.into_iter().enumerate() {
        let index = if rtl { count - 1 - index } else { index };
        let x = plan.config.margin + index as f32 * width;
        plan.push(Draw::Rect {
            x,
            y: plan.y,
            width,
            height,
        });
        let mut y = plan.y + 4.0;
        for line in cell {
            let height = line.height;
            plan.line(line, x + 6.0, y);
            y += height;
        }
    }
    plan.y += height;
}
