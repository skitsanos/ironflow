use crate::util::execution::ExecutionControl;
use anyhow::{Result, ensure};
use lopdf::{Document, Stream, content::Content, dictionary};
use std::collections::BTreeSet;

// Only process our own freshly generated PDF. Krilla emits glyph-level
// ActualText/ReversedChars inside our logical line spans; remove those inner
// overrides so readers see exactly one authoritative Unicode string per line.
pub(super) fn normalize(bytes: Vec<u8>, execution: &ExecutionControl) -> Result<Vec<u8>> {
    let mut document = Document::load_mem(&bytes)?;
    let mut previous = BTreeSet::new();
    for id in document.get_pages().values() {
        execution.checkpoint()?;
        previous.extend(document.get_page_contents(*id));
        let content = Content::decode(&document.get_page_content(*id))?;
        let mut operations = Vec::new();
        let mut stack = Vec::new();
        let mut in_line = false;
        for mut operation in content.operations {
            execution.checkpoint()?;
            match operation.operator.as_str() {
                "BDC" | "BMC" => {
                    let properties = operation
                        .operands
                        .get_mut(1)
                        .and_then(|o| o.as_dict_mut().ok());
                    let outer = properties
                        .as_ref()
                        .is_some_and(|p| p.has(b"MCID") && p.has(b"ActualText"));
                    let reversed = operation
                        .operands
                        .first()
                        .is_some_and(|o| o.as_name().is_ok_and(|name| name == b"ReversedChars"));
                    let inner_actual = operation
                        .operands
                        .get(1)
                        .and_then(|o| o.as_dict().ok())
                        .is_some_and(|p| p.has(b"ActualText"));
                    let keep = !in_line || !(reversed || inner_actual) || outer;
                    stack.push((keep, in_line));
                    if outer {
                        let properties = operation.operands[1].as_dict_mut()?;
                        let mut text = lopdf::decode_text_string(properties.get(b"ActualText")?)?;
                        if !text.ends_with('\n') {
                            text.push('\n');
                        }
                        properties.set("ActualText", lopdf::text_string(&text));
                        in_line = true;
                    }
                    if keep {
                        operations.push(operation);
                    }
                }
                "EMC" => {
                    let (keep, previous_line) = stack
                        .pop()
                        .ok_or_else(|| anyhow::anyhow!("unbalanced generated text spans"))?;
                    in_line = previous_line;
                    if keep {
                        operations.push(operation);
                    }
                }
                _ => operations.push(operation),
            }
        }
        ensure!(stack.is_empty(), "unclosed generated text span");
        let bytes = Content { operations }.encode()?;
        let mut stream = Stream::new(dictionary! {}, bytes);
        stream.compress()?;
        let stream = document.add_object(stream);
        document
            .get_object_mut(*id)?
            .as_dict_mut()?
            .set("Contents", stream);
    }
    for id in previous {
        document.objects.remove(&id);
    }
    execution.checkpoint()?;
    let mut bytes = Vec::new();
    document.save_to(&mut bytes)?;
    Ok(bytes)
}
