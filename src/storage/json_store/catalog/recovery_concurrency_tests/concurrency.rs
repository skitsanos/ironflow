use std::sync::Arc;

use crate::engine::types::Context;
use crate::storage::StateStore;

use super::helpers::{ids, overlay, paged_summaries};
use crate::storage::json_store::JsonStateStore;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_store_writers_preserve_overlay_members_across_the_first_compaction() {
    let directory = tempfile::tempdir().unwrap();
    let first = Arc::new(JsonStateStore::new(directory.path()));
    let second = Arc::new(JsonStateStore::new(directory.path()));
    first.reset_catalog_io_counters();
    second.reset_catalog_io_counters();

    let first_writer = {
        let store = first.clone();
        async move {
            for index in 0..65 {
                store
                    .init_run(&format!("parallel-{index:03}"), "flow", &Context::new())
                    .await
                    .unwrap();
            }
        }
    };
    let second_writer = {
        let store = second.clone();
        async move {
            for index in 65..129 {
                store
                    .init_run(&format!("parallel-{index:03}"), "flow", &Context::new())
                    .await
                    .unwrap();
            }
        }
    };
    tokio::join!(first_writer, second_writer);

    let compactions =
        first.catalog_io_counters().compactions + second.catalog_io_counters().compactions;
    assert_eq!(compactions, 1);
    assert!(overlay(directory.path()).entries().is_empty());

    let expected = first.list_run_summaries(None).await.unwrap();
    let actual = paged_summaries(&second, None, 19).await;
    assert_eq!(actual.len(), 129);
    assert_eq!(ids(&actual), ids(&expected));
}
