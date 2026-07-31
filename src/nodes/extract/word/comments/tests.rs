use std::collections::HashSet;

use crate::nodes::extract::docx_parser::parse_docx_blocks;

#[tokio::test]
async fn comment_anchor_fan_out_is_charged_before_expansion() {
    let starts = (0..8)
        .map(|id| format!(r#"<w:commentRangeStart w:id="{id}"/>"#))
        .collect::<String>();
    let ends = (0..8)
        .map(|id| format!(r#"<w:commentRangeEnd w:id="{id}"/>"#))
        .collect::<String>();
    let xml = format!(
        r#"<w:document xmlns:w="urn:test"><w:body><w:p>{starts}<w:r><w:t>payload</w:t></w:r>{ends}</w:p></w:body></w:document>"#
    );
    let ids = (0..8).map(|id| id.to_string()).collect::<Vec<_>>();

    let error = crate::util::execution::run_blocking_step(move |execution| {
        let limits = crate::nodes::extract::resource::Limits {
            max_output_bytes: 1024,
            max_items: 55,
            max_zip_entries: 10,
            max_zip_bytes: 1024,
            max_pdf_pages: 10,
        };
        let mut budget =
            crate::nodes::extract::resource::Budget::new("extract_word", limits, &execution);
        let comment_ids = ids.iter().map(String::as_str).collect::<HashSet<_>>();
        parse_docx_blocks(
            xml.as_bytes(),
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            Some(&comment_ids),
            &mut budget,
        )
        .map(|_| ())
    })
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("comment anchor fan-out"), "{error}");
    assert!(error.contains("IRONFLOW_MAX_EXTRACT_ITEMS"), "{error}");
}
