//! IF-189: `s3_list_objects` is bounded. A listing stops at an entry ceiling,
//! and an endpoint that never stops truncating fails the step instead of
//! being followed forever.

#[path = "s3_list/support.rs"]
mod support;

use serde_json::{Value, json};
use support::{ListServer, Page, bucket_of};

/// A flow with one `s3_list_objects` step. `parameters` is a JSON object of
/// strings and numbers, written as literal node parameters.
fn flow(parameters: Value) -> String {
    let parameters: String = parameters
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, value)| format!("    {name} = {value},\n"))
        .collect();
    format!(
        "local flow = Flow.new(\"s3-list-bounds\")\n\
         flow:step(\"list\", nodes.s3_list_objects({{\n\
         {parameters}    bucket = \"fixture-bucket\",\n    output_key = \"listing\",\n}}))\n\
         return flow\n"
    )
}

fn keys(context: &Value) -> Vec<&str> {
    context["listing_objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|object| object["key"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn a_complete_listing_follows_every_page_and_reports_no_token() {
    let server = ListServer::start(bucket_of(5)).await;

    let result = server.run(&flow(json!({ "max_keys": 2 })), &[]).await;

    let context = result.context();
    assert_eq!(
        keys(&context),
        ["key-0000", "key-0001", "key-0002", "key-0003", "key-0004"]
    );
    assert_eq!(context["listing_count"], 5);
    assert_eq!(context["listing_truncated"], false);
    assert_eq!(context["listing_next_continuation_token"], "");
    assert_eq!(context["listing_common_prefixes"], json!([]));
    assert_eq!(context["listing_success"], true);
    assert_eq!(context["listing_objects"][0]["size"], 3);
    assert_eq!(context["listing_objects"][0]["storage_class"], "STANDARD");
    assert_eq!(server.queries().len(), 3);
}

#[tokio::test]
async fn common_prefixes_are_returned_with_a_delimiter() {
    let server = ListServer::start(|_| {
        Page {
            keys: vec!["readme.txt".to_string()],
            common_prefixes: vec!["logs/".to_string(), "raw/".to_string()],
            ..Page::default()
        }
        .xml()
    })
    .await;

    let result = server.run(&flow(json!({ "delimiter": "/" })), &[]).await;

    let context = result.context();
    assert_eq!(keys(&context), ["readme.txt"]);
    assert_eq!(context["listing_count"], 1);
    assert_eq!(context["listing_common_prefixes"], json!(["logs/", "raw/"]));
}

#[tokio::test]
async fn max_objects_stops_the_listing_and_the_token_resumes_it() {
    let server = ListServer::start(bucket_of(7)).await;

    let first = server
        .run(&flow(json!({ "max_objects": 3 })), &[])
        .await
        .context();
    assert_eq!(keys(&first), ["key-0000", "key-0001", "key-0002"]);
    assert_eq!(first["listing_truncated"], true);
    let token = first["listing_next_continuation_token"].as_str().unwrap();
    assert_eq!(token, "next-3");

    let second = server
        .run(
            &flow(json!({ "max_objects": 10, "continuation_token": token })),
            &[],
        )
        .await
        .context();
    assert_eq!(
        keys(&second),
        ["key-0003", "key-0004", "key-0005", "key-0006"]
    );
    assert_eq!(second["listing_truncated"], false);
    assert_eq!(second["listing_next_continuation_token"], "");

    // The first call never asked for more than the three it could return.
    let queries = server.queries();
    assert_eq!(queries[0].max_keys, Some(3));
    assert_eq!(queries[1].continuation_token.as_deref(), Some("next-3"));
    assert_eq!(queries.len(), 2);
}

#[tokio::test]
async fn a_listing_larger_than_the_process_ceiling_fails_without_max_objects() {
    let server = ListServer::start(bucket_of(40)).await;

    let failure = server
        .run(
            &flow(json!({ "max_keys": 10 })),
            &[("IRONFLOW_MAX_S3_LIST_OBJECTS", "25")],
        )
        .await
        .failure();

    assert!(
        failure.contains("more than 25 entries")
            && failure.contains("IRONFLOW_MAX_S3_LIST_OBJECTS"),
        "{failure}"
    );
    // 10 + 10 + 5: the last page is cut to what the ceiling still admits.
    let asked: Vec<_> = server
        .queries()
        .iter()
        .map(|query| query.max_keys)
        .collect();
    assert_eq!(asked, [Some(10), Some(10), Some(5)]);
}

#[tokio::test]
async fn max_objects_above_the_process_ceiling_is_refused_before_any_request() {
    let server = ListServer::start(bucket_of(1)).await;

    let failure = server
        .run(
            &flow(json!({ "max_objects": 26 })),
            &[("IRONFLOW_MAX_S3_LIST_OBJECTS", "25")],
        )
        .await
        .failure();

    assert!(
        failure.contains("'max_objects' must be between 1 and 25"),
        "{failure}"
    );
    assert!(server.queries().is_empty());
}

#[tokio::test]
async fn a_truncated_page_without_a_token_fails_instead_of_restarting() {
    let server = ListServer::start(|_| {
        Page {
            keys: vec!["again".to_string()],
            truncated: true,
            ..Page::default()
        }
        .xml()
    })
    .await;

    let failure = server.run(&flow(json!({})), &[]).await.failure();

    assert!(
        failure.contains("truncated page without a continuation token"),
        "{failure}"
    );
    assert_eq!(server.queries().len(), 1);
}

#[tokio::test]
async fn a_repeated_continuation_token_fails_the_step() {
    // Two tokens that name each other: the listing would cycle forever.
    let server = ListServer::start(|query| {
        let next = match query.continuation_token.as_deref() {
            Some("first") => "second",
            _ => "first",
        };
        Page {
            keys: vec![format!("after-{next}")],
            truncated: true,
            next_token: Some(next.to_string()),
            ..Page::default()
        }
        .xml()
    })
    .await;

    let failure = server.run(&flow(json!({})), &[]).await.failure();

    assert!(
        failure.contains("repeated a continuation token"),
        "{failure}"
    );
    assert_eq!(server.queries().len(), 3);
}

#[tokio::test]
async fn the_token_the_caller_supplied_counts_as_seen() {
    let server = ListServer::start(|_| {
        Page {
            keys: vec!["again".to_string()],
            truncated: true,
            next_token: Some("resume".to_string()),
            ..Page::default()
        }
        .xml()
    })
    .await;

    let failure = server
        .run(&flow(json!({ "continuation_token": "resume" })), &[])
        .await
        .failure();

    assert!(
        failure.contains("repeated a continuation token"),
        "{failure}"
    );
    assert_eq!(server.queries().len(), 1);
}

#[tokio::test]
async fn an_endpoint_that_truncates_forever_with_empty_pages_is_abandoned() {
    let counter = std::sync::atomic::AtomicUsize::new(0);
    let server = ListServer::start(move |_| {
        let page = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Page {
            truncated: true,
            next_token: Some(format!("empty-{page}")),
            ..Page::default()
        }
        .xml()
    })
    .await;

    let failure = server.run(&flow(json!({})), &[]).await.failure();

    assert!(
        failure.contains("consecutive truncated pages without an entry"),
        "{failure}"
    );
    assert_eq!(server.queries().len(), 16);
}

#[tokio::test]
async fn a_page_larger_than_requested_fails_the_step() {
    let server = ListServer::start(|_| {
        Page {
            keys: (0..4).map(|index| format!("key-{index}")).collect(),
            ..Page::default()
        }
        .xml()
    })
    .await;

    let failure = server
        .run(&flow(json!({ "max_objects": 3 })), &[])
        .await
        .failure();

    assert!(
        failure.contains("returned 4 entries for a page of at most 3"),
        "{failure}"
    );
}
