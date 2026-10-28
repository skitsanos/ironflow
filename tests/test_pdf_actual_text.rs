#[path = "pdf_actual_text/fixture.rs"]
mod fixture;

use fixture::{document, extract, marked, operation, show};
use ironflow::{engine::types::Context, nodes::NodeRegistry};
use lopdf::{Object, dictionary, text_string};
use serde_json::json;

// The in-process render needs the bundled CJK font; default builds cover this
// path through the CLI tests in test_markdown_fonts.rs with IRONFLOW_FONT_DIR.
#[cfg(feature = "cjk-fonts")]
#[tokio::test]
async fn generated_multilingual_pdf_round_trips_through_the_extraction_node() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("multilingual.pdf");
    let registry = NodeRegistry::with_builtins();
    registry.get("markdown_to_pdf").unwrap().execute(
        &json!({"input":include_str!("../examples/fixtures/markdown-multilingual.md"), "output_path":path}),
        &Context::new()).await.unwrap();
    for format in ["text", "markdown"] {
        let output = registry
            .get("extract_pdf")
            .unwrap()
            .execute(
                &json!({"path":path, "format":format, "metadata_key":"meta"}),
                &Context::new(),
            )
            .await
            .unwrap();
        let text = output["content"].as_str().unwrap();
        for expected in [
            "Український звіт. Ґґ, Єє, Іі, Її.",
            "中文报告",
            "日本語の文書です。",
            "한국어 문서입니다.",
            "עברית",
            "العربية",
            "שלום עולם. טקסט חשוב עם English ומספר 123.",
            "שָׁלוֹם, ישראל. (ID: 42) כתובת לבדיקה.",
            "مرحبا بالعالم. نص مهم مع English والرقم 123.",
            "السَّلَامُ عَلَيْكُمْ. (ID: 42) هذا نص عربي مع علامات التشكيل.",
            "English first, then العربية and עברית, then Україна and 中文. Total: 123.45.",
        ] {
            assert!(
                text.contains(expected),
                "{format}: missing {expected}: {text}"
            );
        }
        assert_eq!(output["meta"]["pages"], 2);
    }
}

#[tokio::test]
async fn outer_actual_text_replaces_nested_glyphs_once_without_extra_breaks() {
    let (mut doc, _, _) = document(vec![
        show("before "),
        marked(text_string("עברית العربية 123")),
        show("wrong glyphs"),
        operation("BMC", vec![Object::Name(b"Span".to_vec())]),
        marked(text_string("must not duplicate")),
        show("also wrong"),
        operation("T*", vec![]),
        operation("EMC", vec![]),
        operation("EMC", vec![]),
        operation("EMC", vec![]),
        show(" after"),
    ]);
    assert_eq!(
        extract(&mut doc).await.unwrap(),
        "before עברית العربية 123 after\n"
    );
}

#[tokio::test]
async fn empty_actual_text_suppresses_content_and_plain_pdf_keeps_legacy_spacing() {
    let (mut doc, _, _) = document(vec![
        show("visible"),
        marked(text_string("")),
        show("hidden"),
        operation("EMC", vec![]),
        operation("T*", vec![]),
        operation("'", vec![Object::string_literal("second")]),
        operation(
            "\"",
            vec![0.into(), 0.into(), Object::string_literal("third")],
        ),
        operation(
            "TJ",
            vec![Object::Array(vec![
                Object::string_literal(" fourth"),
                (-200).into(),
                Object::string_literal("tail"),
            ])],
        ),
    ]);
    assert_eq!(
        extract(&mut doc).await.unwrap(),
        "visible\nsecond\nthird fourth tail \n"
    );
}

#[tokio::test]
async fn inherited_named_properties_and_indirect_actual_text_are_supported() {
    let (mut doc, page, resources) = document(vec![
        operation(
            "BDC",
            vec![
                Object::Name(b"Span".to_vec()),
                Object::Name(b"Replacement".to_vec()),
            ],
        ),
        show("wrong"),
        operation("EMC", vec![]),
    ]);
    let text = doc.add_object(text_string("logical עברית العربية"));
    let properties = doc.add_object(dictionary! {"ActualText"=>text});
    let properties = doc.add_object(dictionary! {"Replacement"=>properties});
    doc.get_dictionary_mut(resources)
        .unwrap()
        .set("Properties", properties);
    let parent = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Parent")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_dictionary_mut(page).unwrap().remove(b"Resources");
    doc.get_dictionary_mut(parent)
        .unwrap()
        .set("Resources", resources);
    assert_eq!(extract(&mut doc).await.unwrap(), "logical עברית العربية\n");
    doc.get_dictionary_mut(page)
        .unwrap()
        .set("Resources", Object::Null);
    assert_eq!(extract(&mut doc).await.unwrap(), "logical עברית العربية\n");
}

#[tokio::test]
async fn pdfdoc_utf16_and_utf8_replacement_strings_keep_logical_unicode() {
    let utf8 = Object::String(
        [b"\xef\xbb\xbf".as_slice(), "العربية 🦀".as_bytes()].concat(),
        lopdf::StringFormat::Hexadecimal,
    );
    let (mut doc, _, _) = document(vec![
        marked(Object::string_literal(b"PDFDoc \x80 ".to_vec())),
        show("wrong"),
        operation("EMC", vec![]),
        marked(text_string("עברית 🦀 ")),
        show("wrong"),
        operation("EMC", vec![]),
        marked(utf8),
        show("wrong"),
        operation("EMC", vec![]),
    ]);
    assert_eq!(
        extract(&mut doc).await.unwrap(),
        "PDFDoc • עברית 🦀 العربية 🦀\n"
    );
}

#[tokio::test]
async fn malformed_and_unbalanced_marked_content_is_not_silently_accepted() {
    for operations in [
        vec![marked(42.into()), operation("EMC", vec![])],
        vec![
            marked(Object::String(
                vec![0xfe, 0xff, 0xd8, 0],
                lopdf::StringFormat::Hexadecimal,
            )),
            operation("EMC", vec![]),
        ],
        vec![
            marked(Object::String(
                vec![0xfe, 0xff, 0],
                lopdf::StringFormat::Hexadecimal,
            )),
            operation("EMC", vec![]),
        ],
        vec![operation("EMC", vec![])],
        vec![marked(text_string("unclosed"))],
        vec![
            operation("BDC", vec![Object::Name(b"Span".to_vec()), 1.into()]),
            operation("EMC", vec![]),
        ],
        vec![
            operation(
                "BDC",
                vec![
                    Object::Name(b"Span".to_vec()),
                    Object::Name(b"Missing".to_vec()),
                ],
            ),
            operation("EMC", vec![]),
        ],
        vec![
            marked(text_string("outer")),
            marked(42.into()),
            operation("EMC", vec![]),
            operation("EMC", vec![]),
        ],
    ] {
        let (mut doc, _, _) = document(operations);
        assert!(extract(&mut doc).await.is_err());
    }
}

#[tokio::test]
async fn property_reference_cycles_and_deep_marked_content_are_rejected() {
    let (mut doc, _, resources) = document(vec![
        operation(
            "BDC",
            vec![
                Object::Name(b"Span".to_vec()),
                Object::Name(b"Cycle".to_vec()),
            ],
        ),
        operation("EMC", vec![]),
    ]);
    let cycle = doc.new_object_id();
    doc.objects.insert(cycle, Object::Reference(cycle));
    doc.get_dictionary_mut(resources)
        .unwrap()
        .set("Properties", dictionary! {"Cycle"=>cycle});
    assert!(extract(&mut doc).await.is_err());
    let mut operations = Vec::new();
    for _ in 0..65 {
        operations.push(operation("BMC", vec![Object::Name(b"Span".to_vec())]));
    }
    for _ in 0..65 {
        operations.push(operation("EMC", vec![]));
    }
    let (mut doc, _, _) = document(operations);
    assert!(extract(&mut doc).await.is_err());
}

#[tokio::test]
async fn excessive_property_reference_chains_are_rejected() {
    let (mut doc, _, resources) = document(vec![
        operation(
            "BDC",
            vec![Object::Name(b"Span".to_vec()), Object::Name(b"P".to_vec())],
        ),
        operation("EMC", vec![]),
    ]);
    let mut value = doc.add_object(dictionary! {"ActualText"=>text_string("logical")});
    for _ in 0..33 {
        value = doc.add_object(Object::Reference(value));
    }
    doc.get_dictionary_mut(resources)
        .unwrap()
        .set("Properties", dictionary! {"P"=>value});
    let error = extract(&mut doc).await.unwrap_err().to_string();
    assert!(error.contains("reference depth"), "{error}");
}

#[tokio::test]
async fn page_selection_keeps_actual_text_and_omits_other_pages() {
    let (mut doc, page, _) = document(vec![
        marked(text_string("OMIT_FIRST")),
        operation("EMC", vec![]),
    ]);
    let content = lopdf::content::Content {
        operations: vec![
            marked(text_string("SELECTED עברית العربية")),
            operation("EMC", vec![]),
        ],
    }
    .encode()
    .unwrap();
    let content = doc.add_object(lopdf::Stream::new(dictionary! {}, content));
    let mut second = doc.get_dictionary(page).unwrap().clone();
    second.set("Contents", content);
    let second = doc.add_object(second);
    let parent = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Parent")
        .unwrap()
        .as_reference()
        .unwrap();
    let parent = doc.get_dictionary_mut(parent).unwrap();
    parent.set(
        "Kids",
        vec![Object::Reference(page), Object::Reference(second)],
    );
    parent.set("Count", 2);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("two-pages.pdf");
    doc.save(&path).unwrap();
    let registry = NodeRegistry::with_builtins();
    let split = registry
        .get("pdf_split")
        .unwrap()
        .execute(
            &json!({"path":path,"output_dir":temp.path().join("pages"),"pages":"2"}),
            &Context::new(),
        )
        .await
        .unwrap();
    let output = registry
        .get("extract_pdf")
        .unwrap()
        .execute(
            &json!({"source_key":"selected"}),
            &Context::from([("selected".into(), split["pdf_split_files"][0].clone())]),
        )
        .await
        .unwrap();
    let text = output["content"].as_str().unwrap();
    assert_eq!(text, "SELECTED עברית العربية");
    assert!(!text.contains("OMIT_FIRST"));
}
