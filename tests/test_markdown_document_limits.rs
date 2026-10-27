use ironflow::{engine::types::Context, nodes::NodeRegistry};
use serde_json::json;
use std::fs;

#[tokio::test]
async fn ast_depth_and_table_limits_reject_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let registry = NodeRegistry::with_builtins();
    let header = format!(
        "|{}|\n|{}|\n",
        vec!["column"; 33].join("|"),
        vec!["---"; 33].join("|")
    );
    for (input, expected) in [
        ("a\n\n".repeat(11_000), "AST nodes"),
        (format!("{}text", "> ".repeat(40)), "nesting"),
        (header, "32 columns"),
        (
            format!("[link](https://example.com/{})", "x".repeat(2048)),
            "2048 URL bytes",
        ),
    ] {
        for node in ["markdown_to_pdf", "markdown_to_docx"] {
            let path = temp.path().join(node);
            let result = registry
                .get(node)
                .unwrap()
                .execute(&json!({"input":input, "output_path":path}), &Context::new())
                .await;
            let error = result.unwrap_err().to_string();
            assert!(error.contains(expected), "{error}");
            assert!(!path.exists());
        }
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn long_pdf_tables_repeat_headers_and_retain_the_final_row() {
    let temp = tempfile::tempdir().unwrap();
    let registry = NodeRegistry::with_builtins();
    let mut markdown = "# Long table\n\n| Item | Detail |\n| --- | --- |\n".to_owned();
    for index in 0..150 {
        markdown.push_str(&format!("| Row{index} | Retained table content |\n"));
    }
    let path = temp.path().join("table.pdf");
    registry
        .get("markdown_to_pdf")
        .unwrap()
        .execute(
            &json!({"input":markdown, "output_path":path}),
            &Context::new(),
        )
        .await
        .unwrap();
    let pdf = lopdf::Document::load(&path).unwrap();
    let pages = pdf.get_pages();
    assert!(pages.len() > 1);
    let text = pdf
        .extract_text(&pages.keys().copied().collect::<Vec<_>>())
        .unwrap();
    assert!(text.contains("Row149"), "{text}");
    assert!(text.matches("Detail").count() >= pages.len());
    let error = registry.get("markdown_to_pdf").unwrap().execute(&json!({"input":"| A | B | C | D | E | F |\n|---|---|---|---|---|---|\n| x | x | x | x | x | x |", "font_size":24, "margin":144, "output_path":temp.path().join("wide.pdf")}), &Context::new()).await.unwrap_err().to_string();
    assert!(error.contains("table is too wide"), "{error}");
}

#[tokio::test]
async fn output_replacement_and_missing_parent_creation_are_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let registry = NodeRegistry::with_builtins();
    for node in ["markdown_to_pdf", "markdown_to_docx"] {
        let path = temp.path().join(node).join("nested").join("report");
        let mut config = json!({"input":"# First", "output_path":path});
        registry
            .get(node)
            .unwrap()
            .execute(&config, &Context::new())
            .await
            .unwrap();
        let first = fs::read(&path).unwrap();
        config["input"] = json!("# Second version");
        assert!(
            registry
                .get(node)
                .unwrap()
                .execute(&config, &Context::new())
                .await
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), first);
        config["overwrite"] = json!(true);
        registry
            .get(node)
            .unwrap()
            .execute(&config, &Context::new())
            .await
            .unwrap();
        assert_ne!(fs::read(&path).unwrap(), first);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }
}

#[tokio::test]
async fn nested_images_and_excessive_bindings_fail_but_unused_bindings_are_not_opened() {
    let temp = tempfile::tempdir().unwrap();
    let registry = NodeRegistry::with_builtins();
    for node in ["markdown_to_pdf", "markdown_to_docx"] {
        let path = temp.path().join(node);
        let base = json!({"input":"# No images", "output_path":path, "images":{"unused":"does-not-exist.png"}});
        registry
            .get(node)
            .unwrap()
            .execute(&base, &Context::new())
            .await
            .unwrap();
        for source in [
            "- ![Nested](unused)",
            "**![Nested](unused)**",
            "> ![Nested](unused)",
            "# ![Nested](unused)",
        ] {
            let mut config = base.clone();
            config["input"] = json!(source);
            config["overwrite"] = json!(true);
            assert!(
                registry
                    .get(node)
                    .unwrap()
                    .execute(&config, &Context::new())
                    .await
                    .is_err()
            );
        }
        let mut config = base;
        config["images"] = (0..17)
            .map(|i| (i.to_string(), json!("unused.png")))
            .collect::<serde_json::Map<_, _>>()
            .into();
        assert!(
            registry
                .get(node)
                .unwrap()
                .execute(&config, &Context::new())
                .await
                .unwrap_err()
                .to_string()
                .contains("16 image bindings")
        );
    }
}

#[tokio::test]
async fn images_are_admitted_against_aggregate_pixels_before_decode() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("large.bmp");
    // BMP headers declare geometry without requiring a large compressed fixture.
    // The incomplete payload would fail decode if admission were skipped.
    let mut header = [0u8; 54];
    header[0..2].copy_from_slice(b"BM");
    header[2..6].copy_from_slice(&(54u32 + 27_000_000).to_le_bytes());
    header[10..14].copy_from_slice(&54u32.to_le_bytes());
    header[14..18].copy_from_slice(&40u32.to_le_bytes());
    header[18..22].copy_from_slice(&3000u32.to_le_bytes());
    header[22..26].copy_from_slice(&3000u32.to_le_bytes());
    header[26..28].copy_from_slice(&1u16.to_le_bytes());
    header[28..30].copy_from_slice(&24u16.to_le_bytes());
    fs::write(&source, header).unwrap();
    let registry = NodeRegistry::with_builtins();
    for node in ["markdown_to_pdf", "markdown_to_docx"] {
        let path = temp.path().join(node);
        let error = registry
            .get(node)
            .unwrap()
            .execute(
                &json!({"input":"![Large](image)", "output_path":path, "images":{"image":source}}),
                &Context::new(),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("8 million decoded pixels"), "{error}");
        assert!(!path.exists());
    }
}
