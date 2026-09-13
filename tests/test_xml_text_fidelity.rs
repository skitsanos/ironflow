use serde_json::json;

#[path = "support/xml_fidelity.rs"]
mod fixture;
use fixture::{DECODED, ENCODED, execute, extract, package};

#[path = "support/xml_cli.rs"]
mod cli;
#[path = "support/xlsx.rs"]
mod xlsx;

#[tokio::test]
async fn xml_accumulates_entities_cdata_and_mixed_text() {
    let output = execute("xml_parse", json!({"input": format!(
        "<root label=\"A&amp;B &#233;\">{ENCODED}<child>x</child> tail &lt;&gt;&apos;&quot;&#128640;</root>"
    )})).await.unwrap();
    assert_eq!(
        output["xml_data"]["root"]["#text"],
        format!("{DECODED} tail <>'\"\u{1f680}")
    );
    assert_eq!(output["xml_data"]["root"]["@label"], "A&B \u{e9}");
    assert_eq!(output["xml_data"]["root"]["child"], "x");
}

#[tokio::test]
async fn word_body_metadata_comments_and_anchors_preserve_event_fragments() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("text.docx");
    let body = format!(
        "<w:document xmlns:w=\"urn:test\"><w:body><w:p>\
        <w:commentRangeStart w:id=\"&#49;\"/><w:r><w:t>{ENCODED}</w:t></w:r>\
        <w:r><w:t> tail</w:t></w:r><w:commentRangeEnd w:id=\"1\"/>\
        </w:p></w:body></w:document>"
    );
    let metadata = format!(
        "<cp:coreProperties xmlns:cp=\"urn:cp\" xmlns:dc=\"urn:dc\"><dc:title>{ENCODED}</dc:title></cp:coreProperties>"
    );
    let comments = format!(
        "<w:comments xmlns:w=\"urn:test\"><w:comment w:id=\"1\" w:author=\"A&amp;B &#233;\">\
        <w:p><w:r><w:t>{ENCODED}</w:t></w:r><w:r><w:t> tail</w:t></w:r></w:p>\
        </w:comment></w:comments>"
    );
    package(
        &path,
        &[
            ("word/document.xml", &body),
            ("docProps/core.xml", &metadata),
            ("word/comments.xml", &comments),
        ],
    );
    let output = extract("extract_word", &path, "text").await.unwrap();
    assert_eq!(output["content"], format!("{DECODED} tail"));
    assert_eq!(output["metadata"]["title"], DECODED);
    assert_eq!(output["comments"][0]["text"], format!("{DECODED} tail"));
    assert_eq!(
        output["comments"][0]["anchored_text"],
        format!("{DECODED} tail")
    );
    assert_eq!(output["comments"][0]["author"], "A&B \u{e9}");
}

#[tokio::test]
async fn pptx_slide_notes_metadata_comments_and_attributes_preserve_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("text.pptx");
    let slide = format!(
        "<p:sld xmlns:p=\"urn:p\" xmlns:a=\"urn:a\"><p:cSld><p:spTree>\
        <p:sp><p:txBody><a:p><a:r><a:t>{ENCODED}</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:pic><p:cNvPr descr=\"A&amp;B &#233;\"/></p:pic></p:spTree></p:cSld></p:sld>"
    );
    let notes = format!(
        "<p:notes xmlns:p=\"urn:p\" xmlns:a=\"urn:a\"><a:p><a:r><a:t>{ENCODED}</a:t></a:r>\
        <a:r><a:t> tail</a:t></a:r></a:p><a:p><a:r><a:t>Next</a:t></a:r></a:p></p:notes>"
    );
    let metadata = format!(
        "<cp:coreProperties xmlns:cp=\"urn:cp\" xmlns:dc=\"urn:dc\"><dc:title>{ENCODED}</dc:title></cp:coreProperties>"
    );
    let comments = format!(
        "<p:cmLst xmlns:p=\"urn:p\"><p:cm authorId=\"&#49;\"><p:text>{ENCODED}</p:text></p:cm></p:cmLst>"
    );
    package(
        &path,
        &[
            ("ppt/slides/slide1.xml", &slide),
            ("ppt/notesSlides/notesSlide1.xml", &notes),
            ("docProps/core.xml", &metadata),
            ("ppt/comments/comment1.xml", &comments),
            (
                "ppt/commentAuthors.xml",
                "<p:cmAuthorLst xmlns:p=\"urn:p\"><p:cmAuthor id=\"1\" name=\"A&amp;B &#233;\"/></p:cmAuthorLst>",
            ),
        ],
    );
    let output = extract("extract_pptx", &path, "json").await.unwrap();
    let slide = &output["content"]["slides"][0];
    assert_eq!(slide["elements"][0]["paragraphs"][0]["text"], DECODED);
    assert_eq!(slide["elements"][1]["alt_text"], "A&B \u{e9}");
    assert_eq!(slide["speaker_notes"], format!("{DECODED} tail\nNext"));
    assert_eq!(output["metadata"]["title"], DECODED);
    assert_eq!(output["comments"][0]["text"], DECODED);
    assert_eq!(output["comments"][0]["author"], "A&B \u{e9}");
}

#[tokio::test]
async fn invalid_references_and_dtds_fail_as_node_errors() {
    let directory = tempfile::tempdir().unwrap();
    for fragment in ["&unknown;", "&#0;", "&#xD800;", "&#xZZ;", "&#x110000;"] {
        for (node, part, xml) in [
            (
                "extract_word",
                "word/document.xml",
                format!(
                    "<w:document xmlns:w=\"urn:w\"><w:p><w:r><w:t>{fragment}</w:t></w:r></w:p></w:document>"
                ),
            ),
            (
                "extract_pptx",
                "ppt/slides/slide1.xml",
                format!(
                    "<p:sld xmlns:p=\"urn:p\"><p:sp><p:txBody><p:p><p:r><p:t>{fragment}</p:t></p:r></p:p></p:txBody></p:sp></p:sld>"
                ),
            ),
        ] {
            let path = directory.path().join(node);
            package(&path, &[(part, &xml)]);
            let error = extract(node, &path, "text").await.unwrap_err();
            assert!(format!("{error:#}").contains("XML"), "{node}: {error:#}");
        }
        assert!(
            execute("xml_parse", json!({"input": format!("<r>{fragment}</r>")}))
                .await
                .is_err()
        );
        assert!(
            execute(
                "xml_parse",
                json!({"input": format!("<r a=\"{fragment}\"/>")})
            )
            .await
            .is_err()
        );
    }
    for (node, part, root) in [
        ("extract_word", "word/document.xml", "w:document"),
        ("extract_pptx", "ppt/slides/slide1.xml", "p:sld"),
    ] {
        let path = directory.path().join(node);
        let xml = format!(
            "<!DOCTYPE {root} [<!ENTITY a 'payload'><!ENTITY b '&a;&a;'>]><{root}>&b;</{root}>"
        );
        package(&path, &[(part, &xml)]);
        let error = extract(node, &path, "text").await.unwrap_err();
        assert!(error.to_string().contains("DTDs"), "{error:#}");
    }
    assert!(
        execute(
            "xml_parse",
            json!({"input": "<!DOCTYPE r SYSTEM 'file:///must-not-open'><r/>"})
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("DTDs")
    );
}

#[tokio::test]
async fn word_reads_text_only_inside_text_elements_and_preserves_run_boundaries() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runs.docx");
    let xml = "<w:document xmlns:w=\"urn:w\"><w:p><w:r>\n <w:rPr><w:b/></w:rPr>\n <w:t>foo&amp;</w:t>\n </w:r><w:r><w:t><![CDATA[bar]]></w:t><w:tab/><w:t>tail</w:t><w:br/><w:t>end</w:t></w:r></w:p></w:document>";
    package(&path, &[("word/document.xml", xml)]);
    let output = extract("extract_word", &path, "json").await.unwrap();
    assert_eq!(output["content"]["blocks"][0]["text"], "foo&bar\ttail\nend");
    assert_eq!(output["content"]["blocks"][0]["runs"][0]["text"], "foo&");
    assert_eq!(output["content"]["blocks"][0]["runs"][0]["bold"], true);
}

#[tokio::test]
async fn xlsx_inline_and_shared_strings_keep_existing_fidelity() {
    use std::io::Write;
    for shared in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let cell = if shared {
            "<c r=\"A1\" t=\"s\"><v>0</v></c>".to_owned()
        } else {
            xlsx::text("A1", ENCODED)
        };
        let path = xlsx::write_workbook(
            directory.path(),
            "strings.xlsx",
            &[xlsx::SheetSpec::new("Sheet", xlsx::row(1, &[cell]))],
        );
        if shared {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            let mut zip = zip::ZipWriter::new_append(file).unwrap();
            zip.start_file(
                "xl/sharedStrings.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            write!(zip, "<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" uniqueCount=\"1\"><si><t>{ENCODED}</t></si></sst>").unwrap();
            zip.finish().unwrap();
        }
        let output = execute("extract_xlsx", json!({"path": path, "has_header": false}))
            .await
            .unwrap();
        assert_eq!(output["content"]["Sheet"][0][0], DECODED, "shared={shared}");
    }
}

#[tokio::test]
async fn xml_deadline_rejects_work_and_attributes_are_strict() {
    use ironflow::util::execution::with_execution_deadline;
    let error = with_execution_deadline(
        Some(tokio::time::Instant::now()),
        execute("xml_parse", json!({"input": "<r>&amp;</r>"})),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("deadline exceeded"));
    for xml in [
        "<r a=unquoted/>",
        "<r a=\"1\" a=\"2\"/>",
        "<r>unclosed",
        "<r></r>tail",
    ] {
        assert!(
            execute("xml_parse", json!({"input": xml})).await.is_err(),
            "{xml}"
        );
    }
}

#[tokio::test]
async fn word_property_numbering_and_theme_attributes_decode_consistently() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("properties.docx");
    let document = "<?xml version=\"1.1\"?><w:document xmlns:w=\"urn:w\"><w:p><w:pPr>\
        <w:pStyle w:val=\"A&amp;B\u{85}C\"/><w:numPr><w:ilvl w:val=\"&#49;\"/>\
        <w:numId w:val=\"&#50;\"/></w:numPr></w:pPr><w:r><w:rPr>\
        <w:color w:themeColor=\"accent&#49;\"/><w:highlight w:val=\"yel&#108;ow\"/>\
        </w:rPr><w:t>x</w:t></w:r></w:p></w:document>";
    let numbering = "<w:numbering xmlns:w=\"urn:w\"><w:abstractNum w:abstractNumId=\"&#49;\">\
        <w:lvl><w:numFmt w:val=\"deci&#109;al\"/></w:lvl></w:abstractNum>\
        <w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>";
    let theme = "<a:theme xmlns:a=\"urn:a\"><a:clrScheme><a:accent1>\
        <a:srgbClr val=\"FF00&#70;F\"/></a:accent1></a:clrScheme></a:theme>";
    package(
        &path,
        &[
            ("word/document.xml", document),
            ("word/numbering.xml", numbering),
            ("word/theme/theme1.xml", theme),
        ],
    );
    let output = extract("extract_word", &path, "json").await.unwrap();
    let paragraph = &output["content"]["blocks"][0];
    assert_eq!(paragraph["style"], "A&B C");
    assert_eq!(paragraph["list"]["level"], 1);
    assert_eq!(paragraph["list"]["numbered"], true);
    assert_eq!(paragraph["runs"][0]["color"], "FF00FF");
    assert_eq!(paragraph["runs"][0]["highlight"], "yellow");
}

#[tokio::test]
async fn pptx_titles_tables_and_list_levels_decode_in_every_format() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("table.pptx");
    let slide = format!(
        "<p:sld xmlns:p=\"urn:p\" xmlns:a=\"urn:a\"><p:spTree>\
        <p:sp><p:nvSpPr><p:ph type=\"ti&#116;le\"/></p:nvSpPr><p:txBody><a:p><a:r><a:t>{ENCODED}</a:t></a:r></a:p></p:txBody></p:sp>\
        <p:sp><p:txBody><a:p><a:pPr lvl=\"&#49;\"/><a:r><a:t>{ENCODED}</a:t></a:r></a:p></p:txBody></p:sp>\
        <a:tbl><a:tr><a:tc><a:txBody><a:p><a:r><a:t>{ENCODED}</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl>\
        </p:spTree></p:sld>"
    );
    package(&path, &[("ppt/slides/slide1.xml", &slide)]);
    for format in ["text", "markdown", "json"] {
        let output = extract("extract_pptx", &path, format).await.unwrap();
        if format == "json" {
            let slide = &output["content"]["slides"][0];
            assert_eq!(slide["title"], DECODED);
            assert_eq!(slide["elements"][0]["paragraphs"][0]["text"], DECODED);
            assert_eq!(slide["elements"][0]["paragraphs"][0]["list_level"], 1);
            assert_eq!(slide["elements"][1]["rows"][0][0], DECODED);
        } else {
            let text = output["content"].as_str().unwrap();
            assert_eq!(text.matches(DECODED).count(), 3, "{format}: {text}");
        }
    }
}
