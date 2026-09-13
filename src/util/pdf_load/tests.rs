use super::*;

#[test]
fn options_fail_closed_with_an_explicit_stream_limit() {
    let options = Limits {
        stream_bytes: 1024,
        objects: 10,
    }
    .options();
    assert!(options.strict);
    assert_eq!(options.max_decompressed_size, Some(1024));
    assert!(options.filter.is_none());
    assert!(options.password.is_none());
}

#[tokio::test]
async fn loaded_object_count_is_inclusive_and_independent_of_stream_bytes() {
    crate::util::execution::run_tracked_blocking_step(|execution| {
        let limits = Limits {
            stream_bytes: 1024,
            objects: 2,
        };
        let mut document = Document::new();
        document.add_object(lopdf::Object::Null);
        document.add_object(lopdf::Object::Null);
        load(limits, "test", &execution, |_| Ok(document.clone()))?;
        document.add_object(lopdf::Object::Null);
        let error = load(limits, "test", &execution, |_| Ok(document)).unwrap_err();
        assert!(error.to_string().contains("IRONFLOW_MAX_PDF_OBJECTS"));
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn expired_deadline_prevents_entering_parser() {
    let result = super::super::execution::with_execution_deadline(
        Some(tokio::time::Instant::now() - std::time::Duration::from_secs(1)),
        super::super::execution::run_tracked_blocking_step(|execution| {
            load(
                Limits {
                    stream_bytes: 1024,
                    objects: 2,
                },
                "test",
                &execution,
                |_| panic!("expired work entered the parser"),
            )
        }),
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("deadline"));
}
