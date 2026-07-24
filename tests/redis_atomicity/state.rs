use std::collections::HashMap;
use std::sync::Arc;

use ironflow::engine::types::{Context, RunStatus, RunSummary};
use ironflow::storage::StateStore;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

use super::redis_support::RedisTest;
use super::{WRITERS, finish_writers, task};

const HIGH_CONTENTION_WRITERS: usize = 320;

#[tokio::test]
async fn redis_parallel_state_mutations_are_lossless_and_preserve_terminal_status() {
    let Some(fixture) = RedisTest::connect("parallel_state").await else {
        return;
    };
    let run_id = "parallel-run";
    let store = fixture.state_store(None).await;
    store
        .init_run(run_id, "parallel-flow", &Context::new())
        .await
        .unwrap();
    store
        .set_run_status(run_id, RunStatus::Running)
        .await
        .unwrap();

    let barrier = Arc::new(Barrier::new(WRITERS * 2 + 2));
    let mut writers = JoinSet::new();
    for index in 0..WRITERS {
        let writer = fixture.state_store(None).await;
        let task_barrier = barrier.clone();
        writers.spawn(async move {
            task_barrier.wait().await;
            writer.upsert_task(run_id, &task(index)).await
        });

        let writer = fixture.state_store(None).await;
        let context_barrier = barrier.clone();
        writers.spawn(async move {
            context_barrier.wait().await;
            let update = HashMap::from([(format!("ctx-{index}"), serde_json::json!(index))]);
            writer.update_ctx(run_id, &update).await
        });
    }
    let finalizer = fixture.state_store(None).await;
    let finalizer_barrier = barrier.clone();
    writers.spawn(async move {
        finalizer_barrier.wait().await;
        finalizer.set_run_status(run_id, RunStatus::Success).await
    });

    let mut pause_conn = fixture.connection().await.unwrap();
    let _: () = redis::cmd("CLIENT")
        .arg("PAUSE")
        .arg(200)
        .arg("WRITE")
        .query_async(&mut pause_conn)
        .await
        .unwrap();
    barrier.wait().await;
    finish_writers(writers).await;

    let info = store.get_run_info(run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert!(info.finished.is_some());
    assert_eq!(info.tasks.len(), WRITERS);
    assert_eq!(info.ctx.len(), WRITERS);
    for index in 0..WRITERS {
        assert!(info.tasks.contains_key(&format!("task-{index}")));
        assert_eq!(info.ctx[&format!("ctx-{index}")], serde_json::json!(index));
    }

    let mut conn = fixture.connection().await.unwrap();
    let key = format!("{}runs:{run_id}", fixture.prefix);
    let (raw_info, raw_summary, revision): (String, String, String) = redis::cmd("HMGET")
        .arg(&key)
        .arg("info")
        .arg("summary")
        .arg("revision")
        .query_async(&mut conn)
        .await
        .unwrap();
    let persisted_info: ironflow::engine::types::RunInfo = serde_json::from_str(&raw_info).unwrap();
    let summary: RunSummary = serde_json::from_str(&raw_summary).unwrap();
    assert_eq!(summary.task_count, persisted_info.tasks.len());
    assert_eq!(summary.status, persisted_info.status);
    assert!(!revision.is_empty());
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_cas_outlives_the_previous_fixed_conflict_budget() {
    let Some(fixture) = RedisTest::connect("cas_contention").await else {
        return;
    };
    let run_id = "high-contention-run";
    let store = fixture.state_store(None).await;
    store
        .init_run(run_id, "parallel-flow", &Context::new())
        .await
        .unwrap();

    let mut stores = Vec::with_capacity(HIGH_CONTENTION_WRITERS);
    for _ in 0..HIGH_CONTENTION_WRITERS {
        stores.push(fixture.state_store(None).await);
    }

    let barrier = Arc::new(Barrier::new(HIGH_CONTENTION_WRITERS + 1));
    let mut writers = JoinSet::new();
    for (index, writer) in stores.into_iter().enumerate() {
        let barrier = barrier.clone();
        writers.spawn(async move {
            barrier.wait().await;
            let update = HashMap::from([(format!("contended-{index}"), serde_json::json!(index))]);
            writer.update_ctx(run_id, &update).await
        });
    }

    let mut pause_conn = fixture.connection().await.unwrap();
    let _: () = redis::cmd("CLIENT")
        .arg("PAUSE")
        .arg(200)
        .arg("WRITE")
        .query_async(&mut pause_conn)
        .await
        .unwrap();
    barrier.wait().await;
    finish_writers(writers).await;

    let info = store.get_run_info(run_id).await.unwrap();
    assert_eq!(info.ctx.len(), HIGH_CONTENTION_WRITERS);
    for index in 0..HIGH_CONTENTION_WRITERS {
        assert_eq!(
            info.ctx[&format!("contended-{index}")],
            serde_json::json!(index)
        );
    }
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_delete_cannot_be_undone_by_inflight_writers() {
    let Some(fixture) = RedisTest::connect("terminal_delete").await else {
        return;
    };
    let run_id = "terminal-run";
    let store = fixture.state_store(None).await;
    store
        .init_run(run_id, "flow", &Context::new())
        .await
        .unwrap();
    store
        .set_run_status(run_id, RunStatus::Running)
        .await
        .unwrap();
    let barrier = Arc::new(Barrier::new(WRITERS + 2));
    let mut writers = JoinSet::new();
    for index in 0..WRITERS {
        let writer = fixture.state_store(None).await;
        let barrier = barrier.clone();
        writers.spawn(async move {
            barrier.wait().await;
            writer.upsert_task(run_id, &task(index)).await
        });
    }
    let deleter = fixture.state_store(None).await;
    let delete_barrier = barrier.clone();
    let delete_handle = tokio::spawn(async move {
        delete_barrier.wait().await;
        deleter.delete_run(run_id).await
    });
    barrier.wait().await;
    let mut committed = 0;
    let mut fenced = 0;
    while let Some(result) = writers.join_next().await {
        match result.expect("Redis writer task panicked") {
            Ok(()) => committed += 1,
            Err(error) => {
                assert!(!error.to_string().is_empty());
                fenced += 1;
            }
        }
    }
    delete_handle.await.unwrap().unwrap();
    assert_eq!(committed + fenced, WRITERS);

    assert!(store.get_run_info(run_id).await.is_err());
    let mut conn = fixture.connection().await.unwrap();
    let indexed: bool = redis::cmd("SISMEMBER")
        .arg(format!("{}runs:index", fixture.prefix))
        .arg(run_id)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(!indexed);
    fixture.cleanup().await;
}
