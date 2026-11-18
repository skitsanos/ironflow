use std::time::Duration;

use jiff::Timestamp;
use uuid::Uuid;

use super::*;
use crate::engine::types::RunStatus;
use crate::storage::StateStore as _;

fn summary(id: &str, micros: Option<i64>) -> RunSummary {
    RunSummary {
        id: id.to_string(),
        flow_name: "flow".to_string(),
        status: RunStatus::Pending,
        started: micros.map(|micros| Timestamp::from_microsecond(micros).unwrap()),
        finished: None,
        task_count: 0,
    }
}

/// Golden values recorded from the `chrono` implementation (IF-231).
#[test]
fn ordered_members_floor_to_the_microsecond_on_both_sides_of_the_epoch() {
    for (seconds, nanos, expected) in [
        (0, 0, "1:8000000000000000:r"),
        (0, 1_999, "1:8000000000000001:r"),
        (1_767_323_045, 123_456_789, "1:8006475ef64ed580:r"),
        (-1, 999_999_999, "1:7fffffffffffffff:r"),
        (-2, 999_999_001, "1:7ffffffffff0bdbf:r"),
    ] {
        let mut value = summary("r", None);
        value.started = Some(Timestamp::new(seconds, nanos).unwrap());
        assert_eq!(ordered_member(&value), expected);
    }
}

#[test]
fn ordered_members_match_public_newest_first_order() {
    let mut values = [
        summary("same-a", Some(10)),
        summary("missing-z", None),
        summary("new", Some(11)),
        summary("same-z", Some(10)),
        summary("before-epoch", Some(-1)),
    ];
    values.sort_by_key(ordered_member);
    values.reverse();
    assert_eq!(
        values
            .iter()
            .map(|summary| summary.id.as_str())
            .collect::<Vec<_>>(),
        ["new", "same-z", "same-a", "before-epoch", "missing-z"]
    );
    for value in values {
        assert_eq!(
            run_id_from_member(&ordered_member(&value)).unwrap(),
            value.id
        );
    }
}

#[test]
fn malformed_ordered_members_are_rejected() {
    for member in ["", "1:123:run", "2:0000000000000000:run"] {
        assert!(run_id_from_member(member).is_err());
    }
}

#[tokio::test]
async fn stale_catalog_snapshot_is_deferred_after_one_attempt() {
    let Ok(url) = std::env::var("IRONFLOW_REDIS_TEST_URL") else {
        return;
    };
    let prefix = format!(
        "ironflow:test:catalog-repair-conflict:{}:",
        Uuid::new_v4().simple()
    );
    let store = RedisStateStore::new(&url, Some(prefix), None)
        .await
        .unwrap();
    let run_id = "hot-run";
    store
        .init_run(run_id, "flow", &crate::engine::types::Context::new())
        .await
        .unwrap();
    let stale = store.read_catalog_snapshot(run_id).await.unwrap().unwrap();
    store
        .set_run_status(run_id, RunStatus::Running)
        .await
        .unwrap();

    let repair = tokio::time::timeout(
        Duration::from_secs(1),
        store.apply_ordered_catalog_snapshot_once(run_id, &stale),
    )
    .await
    .expect("a catalog revision conflict retried instead of being deferred")
    .unwrap();
    assert_eq!(repair, CatalogRepair::Deferred);

    store.delete_run_atomic(run_id).await.unwrap();
    let mut conn = store.conn.clone();
    let _: usize = redis::cmd("DEL")
        .arg(store.ordered_catalog_ready_key())
        .query_async(&mut conn)
        .await
        .unwrap();
}
