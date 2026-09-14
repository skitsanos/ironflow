use ironflow::engine::types::{Context, RunStatus, TaskState, TaskStatus};
use ironflow::storage::{RunLease, StateStore, StorageErrorKind};
use serde_json::json;

use super::support::{Fixture, OWNER, RUN};

#[tokio::test]
async fn owned_status_context_and_task_writes_preflight_every_secondary_key() {
    for ttl in [None, Some(60)] {
        for operation in ["running", "success", "context", "task"] {
            for index in 2..=10 {
                let Some(mut fixture) = Fixture::new(ttl).await else {
                    return;
                };
                fixture.fault(index).await;
                let before = fixture.snapshot().await;
                let result = match operation {
                    "running" => {
                        fixture
                            .store
                            .set_run_status_owned(RUN, RunStatus::Running, OWNER)
                            .await
                    }
                    "success" => {
                        fixture
                            .store
                            .set_run_status_owned(RUN, RunStatus::Success, OWNER)
                            .await
                    }
                    "context" => {
                        fixture
                            .store
                            .update_ctx_owned(
                                RUN,
                                &Context::from([("changed".into(), json!(true))]),
                                OWNER,
                            )
                            .await
                    }
                    _ => {
                        fixture
                            .store
                            .upsert_task_owned(
                                RUN,
                                &TaskState {
                                    name: "task".into(),
                                    node_type: "log".into(),
                                    status: TaskStatus::Success,
                                    attempt: 1,
                                    started: None,
                                    finished: None,
                                    input: None,
                                    output: Some(json!({"changed": true})),
                                    error: None,
                                },
                                OWNER,
                            )
                            .await
                    }
                };
                fixture
                    .assert_unchanged(before, &format!("{operation} key={index} ttl={ttl:?}"))
                    .await;
                assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Corruption);
            }
        }
    }
}

#[tokio::test]
async fn reconciliation_preflights_secondary_keys_before_stalling_a_run() {
    for index in 2..=10 {
        let Some(mut fixture) = Fixture::new(Some(60)).await else {
            return;
        };
        fixture.expire_lease().await;
        fixture.fault(index).await;
        let before = fixture.snapshot().await;
        let result = fixture
            .store
            .reconcile_expired_run_leases(chrono::Utc::now())
            .await;
        fixture
            .assert_unchanged(before, &format!("reconcile key={index}"))
            .await;
        assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Corruption);
    }
}

#[tokio::test]
async fn renewal_preserves_the_hash_deadline_and_ttl_when_expiry_index_is_invalid() {
    for ttl in [None, Some(60)] {
        let Some(mut fixture) = Fixture::new(ttl).await else {
            return;
        };
        fixture.fault(10).await;
        let before = fixture.snapshot().await;
        let result = fixture
            .store
            .renew_run_lease(RUN, &RunLease::renewed(OWNER.into()))
            .await;
        fixture.assert_unchanged(before, "renewal").await;
        assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Corruption);
    }
}

#[tokio::test]
async fn deletion_preflights_all_indexes_for_terminal_expired_and_missing_runs() {
    for state in ["terminal", "expired", "missing"] {
        for index in 1..=10 {
            let Some(mut fixture) = Fixture::new(Some(60)).await else {
                return;
            };
            match state {
                "terminal" => {
                    fixture
                        .store
                        .set_run_status_owned(RUN, RunStatus::Success, OWNER)
                        .await
                        .unwrap();
                }
                "expired" => fixture.expire_lease().await,
                _ => {
                    let _: () = redis::cmd("DEL")
                        .arg(&fixture.keys[0])
                        .query_async(&mut fixture.conn)
                        .await
                        .unwrap();
                }
            }
            fixture.fault(index).await;
            let before = fixture.snapshot().await;
            let result = fixture.store.delete_run(RUN).await;
            fixture
                .assert_unchanged(before, &format!("delete {state} key={index}"))
                .await;
            assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Corruption);
        }
    }
}

#[tokio::test]
async fn ordinary_cas_keeps_its_existing_secondary_key_preflight() {
    for index in 2..=9 {
        let Some(mut fixture) = Fixture::new(Some(60)).await else {
            return;
        };
        fixture.fault(index).await;
        let before = fixture.snapshot().await;
        let result = fixture.store.set_run_status(RUN, RunStatus::Failed).await;
        fixture.assert_unchanged(before, "ordinary CAS").await;
        assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Corruption);
    }
}

#[tokio::test]
async fn listing_sweeps_preflight_indexes_before_removing_stale_members() {
    for summaries in [false, true] {
        for index in 1..=10 {
            let Some(mut fixture) = Fixture::new(Some(60)).await else {
                return;
            };
            let _: () = redis::cmd("DEL")
                .arg(&fixture.keys[0])
                .query_async(&mut fixture.conn)
                .await
                .unwrap();
            fixture.fault(index).await;
            let before = fixture.snapshot().await;
            let result = if summaries {
                fixture.store.list_run_summaries(None).await.map(|_| ())
            } else {
                fixture.store.list_runs(None).await.map(|_| ())
            };
            fixture
                .assert_unchanged(before, &format!("sweep summaries={summaries} key={index}"))
                .await;
            assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Corruption);
        }
    }
}
