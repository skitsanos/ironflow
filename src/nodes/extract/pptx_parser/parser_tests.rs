use super::{presentation::slide_ids, relationships::parse_pptx_rels};
use crate::nodes::extract::resource::{Budget, Limits};
use crate::util::execution::run_blocking_step;

fn limits(output: u64, items: u64) -> Limits {
    Limits {
        max_output_bytes: output,
        max_items: items,
        max_zip_entries: 100,
        max_zip_bytes: 16384,
        max_pdf_pages: 1,
    }
}

fn presentation(body: &str) -> String {
    format!(
        "<p:presentation xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">{body}</p:presentation>"
    )
}

#[tokio::test]
async fn manifest_depth_and_non_element_events_are_bounded() {
    run_blocking_step(|execution| {
        let deep = presentation(&format!(
            "{}{}",
            "<p:ext>".repeat(128),
            "</p:ext>".repeat(128)
        ));
        let mut budget = Budget::new("extract_pptx", limits(16384, 1000), &execution);
        let error = slide_ids(deep.as_bytes(), &mut budget)
            .unwrap_err()
            .to_string();
        assert!(error.contains("nesting exceeds 128"), "{error}");
        let comments = presentation(&"<!--ignored-->".repeat(100));
        let mut budget = Budget::new("extract_pptx", limits(16384, 5), &execution);
        let error = slide_ids(comments.as_bytes(), &mut budget)
            .unwrap_err()
            .to_string();
        assert!(error.contains("IRONFLOW_MAX_EXTRACT_ITEMS"), "{error}");
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn manifest_and_relationship_attribute_decoding_are_admitted_before_allocation() {
    run_blocking_step(|execution| {
        let manifest = presentation(&format!(
            "<p:sldIdLst><p:sldId id=\"256\" r:id=\"{}\"/></p:sldIdLst>",
            "&#49;".repeat(10)
        ));
        let mut budget = Budget::new("extract_pptx", limits(32, 100), &execution);
        let error = slide_ids(manifest.as_bytes(), &mut budget)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("retained slide references")
                && error.contains("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"),
            "{error}"
        );
        let xml = format!(
            "<Relationships><Relationship Id=\"s\" Type=\"image\" Target=\"{}\"/></Relationships>",
            "&#49;".repeat(10)
        );
        let mut budget = Budget::new("extract_pptx", limits(32, 100), &execution);
        let error = parse_pptx_rels(xml.as_bytes(), &mut budget)
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("retained relationship attributes")
                && error.contains("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"),
            "{error}"
        );
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn only_the_direct_presentation_slide_list_controls_membership() {
    run_blocking_step(|execution| {
        let manifest = presentation("<p:extLst><p:sldIdLst><p:sldId id=\"400\" r:id=\"decoy\"/></p:sldIdLst></p:extLst><p:sldIdLst><p:sldId id=\"256\" r:id=\"actual\"></p:sldId></p:sldIdLst>");
        let mut budget = Budget::new("extract_pptx", limits(16384, 100), &execution);
        assert_eq!(slide_ids(manifest.as_bytes(), &mut budget).unwrap(), ["actual"]);
        Ok(())
    }).await.unwrap();
}
