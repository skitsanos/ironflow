use super::{super::resource::Budget, marked::Marked, text_output::Output};
use anyhow::{Context as _, Result, ensure};
use lopdf::{Document, Encoding, Object, ObjectId, content::Content};
use std::collections::BTreeMap;

pub(super) fn extract_page(
    doc: &Document,
    page: ObjectId,
    limit: usize,
    budget: &mut Budget<'_>,
) -> Result<String> {
    let stream_limit = limit.min(
        crate::util::limits::max_pdf_decompressed_stream_bytes().min(isize::MAX as u64) as usize,
    );
    let mut encodings = BTreeMap::new();
    for (name, font) in doc.get_page_fonts(page)? {
        budget.charge_item("PDF fonts")?;
        encodings.insert(name, font.get_font_encoding_with_limit(doc, stream_limit)?);
    }
    budget.checkpoint()?;
    let bytes = doc.get_page_content_with_limit(page, stream_limit)?;
    budget.checkpoint()?;
    let content = Content::decode(&bytes).context("cannot decode PDF page content")?;
    budget.checkpoint()?;
    let mut marked = Marked::default();
    let mut output = Output::new(limit);
    let mut encoding = None;
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
            encoding = encodings.get(name);
            continue;
        }
        if marked.hidden() {
            continue;
        }
        match operation.operator.as_str() {
            "Tj" | "TJ" => {
                if let Some(encoding) = encoding {
                    collect(&operation.operands, encoding, &mut output, budget, 0)?;
                }
            }
            "'" | "\"" => {
                if let Some(encoding) = encoding {
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
                    collect(operands, encoding, &mut output, budget, 0)?;
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
    encoding: &Encoding<'_>,
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
                let text = encoding.bytes_to_string(bytes)?;
                output.push(&text, budget)?;
            }
            Object::Array(array) => {
                collect(array, encoding, output, budget, depth + 1)?;
                output.push(" ", budget)?;
            }
            Object::Integer(value) if *value < -100 => output.push(" ", budget)?,
            _ => {}
        }
    }
    Ok(())
}
