use super::*;
use crate::nodes::extract::resource::Limits;
use crate::util::execution::run_blocking_step;

#[tokio::test]
async fn decoded_references_and_cdata_consume_real_extraction_budget() {
    run_blocking_step(|execution| {
        for content in ["&#x1F680;&#x1F680;", "<![CDATA[12345678]]>"] {
            let limits = Limits {
                max_output_bytes: 7,
                max_items: 100,
                max_zip_entries: 10,
                max_zip_bytes: 1024,
                max_pdf_pages: 10,
            };
            let mut budget = Budget::new("extract_word", limits, &execution);
            let xml = format!("<r>{content}</r>");
            let mut reader = quick_xml::Reader::from_str(&xml);
            let mut document = XmlDocument::new("test.xml");
            let error = loop {
                let event = reader.read_event()?;
                assert!(!matches!(event, Event::Eof), "expected budget rejection");
                if let Err(error) = document.decode(event, &mut budget) {
                    break error;
                }
            };
            assert!(
                error.to_string().contains(
                    "DOCX XML decoded text exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES (7)"
                ),
                "{error}"
            );
        }
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn temporary_attribute_decoding_is_admitted_before_normalization() {
    run_blocking_step(|execution| {
        let limits = Limits {
            max_output_bytes: 4,
            max_items: 100,
            max_zip_entries: 10,
            max_zip_bytes: 1024,
            max_pdf_pages: 10,
        };
        let mut budget = Budget::new("extract_word", limits, &execution);
        let mut reader = quick_xml::Reader::from_str("<r a=\"&amp;\"/>");
        let mut document = XmlDocument::new("test.xml");
        let error = document
            .decode(reader.read_event()?, &mut budget)
            .unwrap_err();
        assert!(
            error.to_string().contains(
                "DOCX XML attribute decoding exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES (4)"
            ),
            "{error}"
        );
        Ok(())
    })
    .await
    .unwrap();
}
