use super::{
    super::resource::Budget,
    fonts::{Decoder, Fonts},
    layout::{Join, Layout, Run},
    marked::Marked,
    text_output::Output,
};
use anyhow::{Context as _, Result, ensure};
use lopdf::{Document, Object, ObjectId, content::Content, content::Operation};

pub(super) fn extract_page(
    doc: &Document,
    page: ObjectId,
    limit: usize,
    fonts: &mut Fonts<'_>,
    budget: &mut Budget<'_>,
) -> Result<String> {
    let stream_limit = limit.min(
        crate::util::limits::max_pdf_decompressed_stream_bytes().min(isize::MAX as u64) as usize,
    );
    let names = fonts.page(page, stream_limit, budget)?;
    budget.checkpoint()?;
    let bytes = doc.get_page_content_with_limit(page, stream_limit)?;
    budget.checkpoint()?;
    let content = Content::decode(&bytes).context("cannot decode PDF page content")?;
    budget.checkpoint()?;
    let mut marked = Marked::default();
    let mut output = Output::new(limit);
    let mut layout = Layout::default();
    let mut decoder = None;
    for operation in &content.operations {
        budget.charge_item("PDF content operations")?;
        let before = output.len();
        if marked.process(doc, page, operation, &mut output, budget)? {
            // Replacement text has no position: a block that ended before it
            // still breaks the line.
            if output.len() > before && layout.take_block_end() {
                output.break_at(before, budget)?;
            }
            continue;
        }
        let hidden = marked.hidden();
        match operation.operator.as_str() {
            "Tf" => {
                let name = operation
                    .operands
                    .first()
                    .context("missing PDF font operand")?
                    .as_name()?;
                decoder = names.get(name).and_then(|index| fonts.get(*index));
                layout.font_size(&operation.operands);
            }
            "Tj" | "TJ" | "'" | "\"" => {
                show(operation, decoder, &mut layout, &mut output, hidden, budget)?;
            }
            "T*" => {
                layout.next_line();
                if !hidden {
                    output.newline(budget)?;
                }
            }
            "ET" if !hidden => layout.end_block(),
            operator => layout.operation(operator, &operation.operands),
        }
    }
    marked.finish()?;
    if layout.take_block_end() {
        output.newline(budget)?;
    }
    Ok(output.finish())
}

/// One show operator. Hidden text (inside an `ActualText` span) and text of
/// a font that cannot be decoded still moves the text position.
fn show(
    operation: &Operation,
    decoder: Option<&Decoder<'_>>,
    layout: &mut Layout,
    output: &mut Output,
    hidden: bool,
    budget: &mut Budget<'_>,
) -> Result<()> {
    let next_line = operation.operator != "Tj" && operation.operator != "TJ";
    let operands = if operation.operator == "\"" {
        layout.spacing(&operation.operands);
        operation
            .operands
            .get(2)
            .map(std::slice::from_ref)
            .unwrap_or_default()
    } else {
        &operation.operands
    };
    if next_line {
        layout.next_line();
    }
    let visible = !hidden && decoder.is_some();
    if visible {
        match layout.join() {
            _ if next_line => output.newline(budget)?,
            Join::Line => output.newline(budget)?,
            Join::Space => output.space(budget)?,
            Join::None => {}
        }
    }
    let mut run = Run::default();
    let target = visible.then_some(output);
    collect(operands, decoder, target, &mut run, budget, 0)?;
    layout.shown(&run, visible);
    Ok(())
}

fn collect(
    operands: &[Object],
    decoder: Option<&Decoder<'_>>,
    mut output: Option<&mut Output>,
    run: &mut Run,
    budget: &mut Budget<'_>,
    depth: usize,
) -> Result<()> {
    ensure!(depth <= 64, "PDF text operand nesting exceeds 64");
    for operand in operands {
        budget.charge_item("PDF text operands")?;
        match operand {
            Object::String(bytes, _) => {
                budget.charge_items(
                    (bytes.len() as u64).div_ceil(4096),
                    "PDF glyph input chunks",
                )?;
                run.glyphs(decoder.and_then(|decoder| decoder.measure(bytes)));
                if let (Some(decoder), Some(output)) = (decoder, output.as_deref_mut()) {
                    decoder.show(bytes, output, budget)?;
                }
            }
            Object::Array(array) => {
                collect(
                    array,
                    decoder,
                    output.as_deref_mut(),
                    run,
                    budget,
                    depth + 1,
                )?;
                if let Some(output) = output.as_deref_mut() {
                    output.push(" ", budget)?;
                }
            }
            Object::Integer(value) => {
                run.adjust(*value as f64);
                if *value < -100
                    && let Some(output) = output.as_deref_mut()
                {
                    output.push(" ", budget)?;
                }
            }
            Object::Real(value) if value.is_finite() => run.adjust(f64::from(*value)),
            _ => {}
        }
    }
    Ok(())
}
