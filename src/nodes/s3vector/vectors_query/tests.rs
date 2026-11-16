use super::*;

#[test]
fn query_vectors_names_input_sets_only_name_form() {
    let request = prepare_query_vectors_input(
        &serde_json::json!({
            "vector_bucket_name": "demo-bucket",
            "index_name": "demo-index",
            "top_k": 2,
            "query_vector": [0.1, 0.2, 0.3]
        }),
        &Context::new(),
    )
    .unwrap()
    .request
    .build()
    .unwrap();

    assert_eq!(request.vector_bucket_name(), Some("demo-bucket"));
    assert_eq!(request.index_name(), Some("demo-index"));
    assert_eq!(request.index_arn(), None);
}

#[test]
fn query_vectors_arn_input_sets_only_arn_form() {
    let arn = "arn:aws:s3vectors:us-east-1:123456789012:bucket/demo/index/demo";
    let request = prepare_query_vectors_input(
        &serde_json::json!({
            "index_arn": arn,
            "top_k": 2,
            "query_vector": [0.1, 0.2, 0.3]
        }),
        &Context::new(),
    )
    .unwrap()
    .request
    .build()
    .unwrap();

    assert_eq!(request.vector_bucket_name(), None);
    assert_eq!(request.index_name(), None);
    assert_eq!(request.index_arn(), Some(arn));
}

fn filter_config(extra: serde_json::Value) -> serde_json::Value {
    let mut config = serde_json::json!({
        "vector_bucket_name": "demo-bucket",
        "index_name": "demo-index",
        "top_k": 2,
        "query_vector": [0.1, 0.2, 0.3],
        // Unroutable on purpose: a regression that reaches the client fails
        // locally instead of calling a real service.
        "endpoint_url": "http://127.0.0.1:1",
        "region": "us-east-1"
    });
    config
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    config
}

fn filter_error(extra: serde_json::Value, ctx: &Context) -> String {
    match prepare_query_vectors_input(&filter_config(extra), ctx) {
        Ok(prepared) => panic!(
            "expected a filter error, built a request with filter {:?}",
            prepared.request.build().unwrap().filter()
        ),
        Err(error) => error.to_string(),
    }
}

#[test]
fn filter_key_value_is_sent_as_the_request_filter() {
    let ctx = Context::from([(
        "tenant_filter".to_owned(),
        serde_json::json!({ "tenant": "acme" }),
    )]);
    let request = prepare_query_vectors_input(
        &filter_config(serde_json::json!({ "filter_key": "tenant_filter" })),
        &ctx,
    )
    .unwrap()
    .request
    .build()
    .unwrap();

    let filter = request.filter().expect("request carries the filter");
    assert_eq!(
        document_to_json(filter),
        serde_json::json!({ "tenant": "acme" })
    );
}

#[test]
fn filter_key_missing_from_context_builds_no_request() {
    let error = filter_error(
        serde_json::json!({ "filter_key": "tenant_filter" }),
        &Context::new(),
    );
    assert_eq!(
        error,
        "s3vector_query_vectors requires filter_key 'tenant_filter' to exist in context"
    );
}

#[test]
fn filter_key_must_be_a_non_empty_string() {
    let ctx = Context::from([("7".to_owned(), serde_json::json!({ "tenant": "acme" }))]);
    for filter_key in [
        serde_json::json!(7),
        serde_json::json!(true),
        serde_json::json!(null),
        serde_json::json!(["tenant_filter"]),
        serde_json::json!(""),
        serde_json::json!("${ctx.unset}"),
    ] {
        let error = filter_error(serde_json::json!({ "filter_key": filter_key }), &ctx);
        assert_eq!(
            error,
            "s3vector_query_vectors requires 'filter_key' to be a non-empty string"
        );
    }
}

#[test]
fn filter_key_value_must_be_a_non_empty_object() {
    for value in [
        serde_json::json!(null),
        serde_json::json!({}),
        serde_json::json!([{ "tenant": "acme" }]),
        serde_json::json!("tenant = acme"),
        serde_json::json!(false),
    ] {
        let ctx = Context::from([("tenant_filter".to_owned(), value)]);
        let error = filter_error(serde_json::json!({ "filter_key": "tenant_filter" }), &ctx);
        assert_eq!(
            error,
            "s3vector_query_vectors requires filter_key 'tenant_filter' to hold a non-empty filter object"
        );
    }
}

#[test]
fn filter_error_does_not_echo_the_context_value() {
    let ctx = Context::from([("tenant_filter".to_owned(), serde_json::json!("s3cr3t"))]);
    let error = filter_error(serde_json::json!({ "filter_key": "tenant_filter" }), &ctx);
    assert!(!error.contains("s3cr3t"), "{error}");
}

#[test]
fn inline_filter_without_placeholders_is_unchanged_and_wins_over_filter_key() {
    let request = prepare_query_vectors_input(
        &filter_config(serde_json::json!({
            "filter": { "source": "vtt" },
            "filter_key": "absent"
        })),
        &Context::new(),
    )
    .unwrap()
    .request
    .build()
    .unwrap();

    assert_eq!(
        document_to_json(request.filter().unwrap()),
        serde_json::json!({ "source": "vtt" })
    );
}

#[tokio::test]
async fn execute_fails_on_a_missing_filter_key_before_any_request() {
    let error = S3VectorQueryVectorsNode
        .execute(
            &filter_config(serde_json::json!({ "filter_key": "tenant_filter" })),
            &Context::new(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert_eq!(
        error,
        "s3vector_query_vectors requires filter_key 'tenant_filter' to exist in context"
    );
}

#[test]
fn flags_default_to_false_only_when_absent() {
    let ctx = Context::from([("on".to_owned(), serde_json::json!(true))]);
    let prepared =
        prepare_query_vectors_input(&filter_config(serde_json::json!({})), &ctx).unwrap();
    assert!(!prepared.strict && !prepared.return_metadata && !prepared.return_distance);

    for accepted in [
        serde_json::json!(true),
        serde_json::json!("yes"),
        serde_json::json!("${ctx.on}"),
    ] {
        let prepared = prepare_query_vectors_input(
            &filter_config(serde_json::json!({
                "strict": accepted, "return_metadata": accepted, "return_distance": accepted
            })),
            &ctx,
        )
        .unwrap();
        assert!(prepared.strict && prepared.return_metadata && prepared.return_distance);
    }

    for flag in ["strict", "return_metadata", "return_distance"] {
        for rejected in [
            serde_json::json!("tru"),
            serde_json::json!(1),
            serde_json::json!(null),
        ] {
            let mut extra = serde_json::json!({});
            extra[flag] = rejected;
            let error = prepare_query_vectors_input(&filter_config(extra), &ctx)
                .err()
                .expect(flag)
                .to_string();
            assert_eq!(
                error,
                format!("s3vector_query_vectors requires '{flag}' to be a boolean")
            );
        }
    }
}

#[test]
fn inline_filter_placeholders_resolve_once_and_keys_stay_as_written() {
    let ctx = Context::from([
        ("tenant".to_owned(), serde_json::json!("acme")),
        ("indirect".to_owned(), serde_json::json!("${ctx.tenant}")),
        ("count".to_owned(), serde_json::json!(2)),
    ]);
    let request = prepare_query_vectors_input(
        &filter_config(serde_json::json!({ "filter": {
            "tenant": "${ctx.tenant}", "${ctx.tenant}": ["a-${ctx.count}", "${ctx.indirect}"],
            "escaped": "\\${ctx.tenant}", "limit": 2
        }})),
        &ctx,
    )
    .unwrap()
    .request
    .build()
    .unwrap();
    assert_eq!(
        document_to_json(request.filter().unwrap()),
        serde_json::json!({
            "tenant": "acme", "${ctx.tenant}": ["a-2", "${ctx.tenant}"],
            "escaped": "${ctx.tenant}", "limit": 2
        })
    );
}

#[test]
fn an_inline_filter_placeholder_that_does_not_resolve_builds_no_request() {
    let ctx = Context::from([
        ("tenant".to_owned(), serde_json::json!("acme-secret")),
        ("nothing".to_owned(), serde_json::Value::Null),
        ("object".to_owned(), serde_json::json!({ "id": 1 })),
    ]);
    for (filter, expected) in [
        (
            serde_json::json!({ "tenant": "${ctx.tenant_id}" }),
            "s3vector_query_vectors could not interpolate 'filter.tenant': invalid context \
             interpolation `ctx.tenant_id`: the value is missing from context",
        ),
        (
            serde_json::json!({ "$and": [{ "a": "${ctx.tenant}" }, { "b": "${ctx.nothing}" }] }),
            "s3vector_query_vectors could not interpolate 'filter.$and.1.b': invalid context \
             interpolation `ctx.nothing`: the value is missing from context",
        ),
        (
            serde_json::json!("${ctx.object}"),
            "s3vector_query_vectors could not interpolate 'filter': invalid context \
             interpolation `ctx.object`: the value is an object or array, not text",
        ),
    ] {
        let error = filter_error(serde_json::json!({ "filter": filter }), &ctx);
        assert_eq!(error, expected);
        assert!(!error.contains("acme-secret"));
    }
}

#[test]
fn top_k_above_the_request_range_is_refused() {
    for top_k in [
        serde_json::json!(2_147_483_648_u64),
        serde_json::json!(4_294_967_295_u64),
    ] {
        let error = prepare_query_vectors_input(
            &filter_config(serde_json::json!({ "top_k": top_k })),
            &Context::new(),
        )
        .err()
        .expect("top_k out of range")
        .to_string();
        assert_eq!(
            error,
            "s3vector_query_vectors requires 'top_k' to be at most 2147483647"
        );
    }
}
