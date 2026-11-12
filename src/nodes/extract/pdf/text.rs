use super::{
    super::resource::Budget,
    fonts::{Decoder, Fonts},
    marked::Marked,
    text_output::Output,
};
use anyhow::{Context as _, Result, ensure};
use lopdf::{Document, Object, ObjectId, content::Content};

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
    let mut decoder = None;
    for operation in &content.operations {
        budget.charge_item("PDF content operations")?;
        if marked.process(doc, page, operation, &mut output, budget)? {
            continue;
        }
        if operation.operator == "Tf" {
            let name = operation
                .operands
                .first()
                .context("missing PDF font operand")?
                .as_name()?;
            decoder = names.get(name).and_then(|index| fonts.get(*index));
            continue;
        }
        if marked.hidden() {
            continue;
        }
        match operation.operator.as_str() {
            "Tj" | "TJ" => {
                if let Some(decoder) = decoder {
                    collect(&operation.operands, decoder, &mut output, budget, 0)?;
                }
            }
            "'" | "\"" => {
                if let Some(decoder) = decoder {
                    output.newline(budget)?;
                    let operands = if operation.operator == "\"" {
                        operation
                            .operands
                            .get(2)
                            .map(std::slice::from_ref)
                            .unwrap_or_default()
                    } else {
                        &operation.operands
                    };
                    collect(operands, decoder, &mut output, budget, 0)?;
                }
            }
            "T*" | "ET" => output.newline(budget)?,
            _ => {}
        }
    }
    marked.finish()?;
    Ok(output.finish())
}

fn collect(
    operands: &[Object],
    decoder: &Decoder<'_>,
    output: &mut Output,
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
                decoder.show(bytes, output, budget)?;
            }
            Object::Array(array) => {
                collect(array, decoder, output, budget, depth + 1)?;
                output.push(" ", budget)?;
            }
            Object::Integer(value) if *value < -100 => output.push(" ", budget)?,
            _ => {}
        }
    }
    Ok(())
}
