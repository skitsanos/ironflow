use super::*;
use crate::nodes::extract::resource::Limits;
use crate::util::execution::run_blocking_step;
use lopdf::{Stream, dictionary};

const OUTPUT_LIMIT: &str = "IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES";

fn cmap(section: &str) -> String {
    format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n{section}\n\
         endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    )
}

/// Decode `operand` with the font's decoder into a page limited to `limit`.
async fn show(font: Dictionary, doc: Document, operand: Vec<u8>, limit: usize) -> Result<String> {
    run_blocking_step(move |execution| {
        let budget = Budget::new("test", Limits::current(), &execution);
        let longest_target = font
            .get_deref(b"ToUnicode", &doc)
            .and_then(Object::as_stream)
            .map(|stream| cmap_scan::scan(&stream.content, || Ok(())))
            .map_or(Ok(cmap_scan::MAX_TARGET_UNITS), |cost| {
                cost.map(|cost| cost.longest_target)
            })?;
        let decoder = Decoder::new(font.get_font_encoding(&doc)?, longest_target);
        let mut output = Output::new(limit);
        decoder.show(&operand, &mut output, &budget)?;
        let text = output.finish();
        anyhow::ensure!(
            text.len() <= operand.len() * decoder.expansion,
            "{} bytes from {} at expansion {}",
            text.len(),
            operand.len(),
            decoder.expansion
        );
        Ok(text)
    })
    .await
}

fn simple_font() -> (Dictionary, Document) {
    (
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "Encoding" => "WinAnsiEncoding" },
        Document::new(),
    )
}

fn mapped_font(section: &str, encoding: Object) -> (Dictionary, Document) {
    let mut doc = Document::new();
    let stream = doc.add_object(Stream::new(dictionary! {}, cmap(section).into_bytes()));
    (
        dictionary! { "Type" => "Font", "Encoding" => encoding, "ToUnicode" => stream },
        doc,
    )
}

#[tokio::test]
async fn a_single_byte_string_over_the_direct_ceiling_is_decoded_in_pieces() {
    // 600 KiB can become 1.8 MiB, more than the page has left: decoded in
    // pieces, the text that actually results is admitted exactly.
    let operand = vec![b'a'; 600 * 1024];
    let (font, doc) = simple_font();
    let text = show(font, doc, operand.clone(), 700 * 1024).await.unwrap();
    assert_eq!(text.as_bytes(), operand);

    let (font, doc) = simple_font();
    let error = show(font, doc, operand, 599 * 1024).await.unwrap_err();
    assert!(error.to_string().contains(OUTPUT_LIMIT), "{error}");
}

#[tokio::test]
async fn a_code_mapped_string_is_admitted_by_its_worst_case() {
    let section = "1 beginbfchar\n<01> <0041>\nendbfchar";
    let operand = vec![1_u8; 400 * 1024];
    let (font, doc) = mapped_font(section, "Identity-H".into());
    let text = show(font, doc, operand.clone(), 6 * 400 * 1024)
        .await
        .unwrap();
    assert_eq!(text.len(), operand.len());

    let (font, doc) = mapped_font(section, "Identity-H".into());
    let error = show(font, doc, operand, 6 * 400 * 1024 - 1)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(OUTPUT_LIMIT), "{error}");
    assert!(error.contains("glyph string of 409600 bytes"), "{error}");

    // Below the direct ceiling the decoded text is admitted exactly, as before.
    let (font, doc) = mapped_font(section, "Identity-H".into());
    assert_eq!(
        show(font, doc, vec![1; 64], 64).await.unwrap(),
        "A".repeat(64)
    );
    let (font, doc) = mapped_font(section, "Identity-H".into());
    let error = show(font, doc, vec![1; 64], 63).await.unwrap_err();
    assert!(error.to_string().contains(OUTPUT_LIMIT), "{error}");
}

#[tokio::test]
async fn the_worst_case_covers_long_destinations_and_byte_order_marks() {
    // One code to 256 units.
    let long = format!("1 beginbfchar\n<01> <{}>\nendbfchar", "0041".repeat(256));
    let (font, doc) = mapped_font(&long, "Identity-H".into());
    assert_eq!(show(font, doc, vec![1; 8], 4096).await.unwrap().len(), 2048);

    // Decoded units that start with a UTF-8 byte-order mark are re-read as
    // UTF-8 by lopdf: every remaining byte becomes a three-byte U+FFFD.
    let mark = "3 beginbfchar\n<01> <EFBB>\n<02> <BFFF>\n<03> <FFFF>\nendbfchar";
    let mut operand = vec![1, 2];
    operand.extend([3; 100]);
    let (font, doc) = mapped_font(mark, "Identity-H".into());
    let text = show(font, doc, operand, 4096).await.unwrap();
    assert_eq!(text.len(), 3 * 201, "more than three bytes for each unit");

    // Differences decode unlisted bytes one at a time through the base map.
    let differences = dictionary! {
        "Type" => "Encoding", "BaseEncoding" => "Identity-H",
        "Differences" => vec![65.into(), Object::Name(b"B".to_vec())]
    };
    let (font, doc) = mapped_font(&long, differences.into());
    let text = show(font, doc, vec![1, 65, 1], 4096).await.unwrap();
    assert_eq!(text, format!("{0}B{0}", "A".repeat(256)));
}
