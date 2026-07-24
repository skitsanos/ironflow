use chrono::{TimeZone, Utc};
use ironflow::engine::types::RunStatus;
use ironflow::engine::{RunEvent, RunEventType};
use ironflow::storage::StorageErrorKind;
use ironflow::storage::event_store::EventStore;

pub async fn assert_event_identity_contract(store: &dyn EventStore, run_id: &str) {
    let mut invalid = RunEvent::run(run_id, "flow", RunEventType::RunStarted, RunStatus::Running);
    invalid.id.clear();
    assert_eq!(
        store.publish(invalid).await.unwrap_err().kind(),
        StorageErrorKind::InvalidInput
    );

    let event = RunEvent::run(run_id, "flow", RunEventType::RunStarted, RunStatus::Running);
    store.publish(event.clone()).await.unwrap();
    store.publish(event.clone()).await.unwrap();

    let other_run_id = format!("{run_id}-same-event-id");
    let mut other_run = event.clone();
    other_run.run_id.clone_from(&other_run_id);
    store.publish(other_run.clone()).await.unwrap();
    assert_eq!(
        store.list_since(&other_run_id, None, 10).await.unwrap(),
        vec![other_run.clone()]
    );

    let mut conflicting = event;
    conflicting.reason = Some("different payload".to_string());
    assert_eq!(
        store.publish(conflicting).await.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );
    assert_eq!(store.delete_run(run_id).await.unwrap(), 1);
    assert_eq!(
        store.list_since(&other_run_id, None, 10).await.unwrap(),
        vec![other_run]
    );
    assert_eq!(
        store
            .list_since(run_id, Some("missing-cursor"), 10)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
}

pub fn ordered_event(run_id: &str, suffix: &str, timestamp_offset: i64) -> RunEvent {
    let mut event = RunEvent::run(
        run_id,
        "cursor-contract",
        RunEventType::ContextUpdated,
        RunStatus::Running,
    );
    event.id = format!("{run_id}-{suffix}");
    event.timestamp = Utc
        .timestamp_opt(1_700_000_000 + timestamp_offset, 0)
        .single()
        .unwrap();
    event
}

pub async fn assert_event_cursor_contract(store: &dyn EventStore, run_id: &str) {
    let empty_run = format!("{run_id}-empty");
    assert!(
        store
            .list_since(&empty_run, None, 2)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .list_since(&empty_run, Some(""), 2)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .list_since(&empty_run, Some("unknown-empty-cursor"), 2)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );

    let events = vec![
        ordered_event(run_id, "event-1", 1),
        ordered_event(run_id, "event-2", 2),
        ordered_event(run_id, "event-3", 3),
        ordered_event(run_id, "event-4", 4),
    ];
    for event in &events {
        store.publish(event.clone()).await.unwrap();
    }

    assert!(store.list_since(run_id, None, 0).await.unwrap().is_empty());
    assert!(
        store
            .list_since(run_id, Some(&events[0].id), 0)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .list_since(run_id, Some("unknown-cursor"), 0)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );

    let first_page = store.list_since(run_id, None, 2).await.unwrap();
    assert_eq!(first_page, events[..2]);
    assert_eq!(
        store.list_since(run_id, Some(""), 2).await.unwrap(),
        events[..2]
    );
    assert_eq!(
        store
            .list_since(run_id, Some(&events[0].id), 1)
            .await
            .unwrap(),
        events[1..2]
    );
    let second_page = store
        .list_since(run_id, Some(&events[1].id), 2)
        .await
        .unwrap();
    assert_eq!(second_page, events[2..]);
    let mut concatenated = first_page;
    concatenated.extend(second_page);
    assert_eq!(concatenated, events);
    assert!(
        store
            .list_since(run_id, Some(&events[3].id), 2)
            .await
            .unwrap()
            .is_empty()
    );

    let foreign_run = format!("{run_id}-foreign");
    let foreign = ordered_event(&foreign_run, "event", 5);
    store.publish(foreign.clone()).await.unwrap();
    assert_eq!(
        store
            .list_since(run_id, Some(&foreign.id), 2)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
    assert_eq!(
        store
            .list_since(&foreign_run, Some(&events[0].id), 2)
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::NotFound
    );
}
