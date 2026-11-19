use super::{Arc, Context, HashMap, Ordering, Outcome, ScheduleConfig, Scheduler, StubExecutor};
use ironflow::storage::json_store::JsonStateStore;
use jiff::Timestamp;

fn instant(text: &str) -> Timestamp {
    text.parse().unwrap()
}

fn nightly(zone: &str, cron: &str) -> HashMap<String, ScheduleConfig> {
    let schedule =
        ScheduleConfig::new("nightly.lua", cron, Some(zone), Some(2700), Context::new()).unwrap();
    HashMap::from([("nightly".into(), schedule)])
}

#[tokio::test]
async fn restart_recovers_the_first_berlin_0230_only_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(dir.path()));
    let executor = Arc::new(StubExecutor::default());
    let schedules = nightly("Europe/Berlin", "30 2 * * *");
    let restart = instant("2026-10-25T01:05:00Z");
    let mut scheduler = Scheduler::new(schedules.clone(), store.clone(), executor.clone(), restart);
    let decisions = scheduler.evaluate(restart).await;
    assert_eq!(
        decisions.len(),
        1,
        "expected the first 02:30 at restart: {decisions:?}"
    );
    assert_eq!(decisions[0].key, "Europe/Berlin@2026-10-25T02:30");
    assert!(matches!(decisions[0].outcome, Outcome::Fired { .. }));

    // A second replica starting inside grace must see the durable occurrence.
    let mut peer = Scheduler::new(schedules, store, executor.clone(), restart);
    let decisions = peer.evaluate(restart).await;
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].outcome, Outcome::NotClaimed);
    for at in ["2026-10-25T01:30:00Z", "2026-10-25T02:00:00Z"] {
        assert!(scheduler.evaluate(instant(at)).await.is_empty());
        assert!(peer.evaluate(instant(at)).await.is_empty());
    }
    assert_eq!(executor.runs.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn recovery_respects_the_transition_size_and_does_not_fire_early() {
    for (zone, cron, at, key) in [
        // Exactly at a 30-minute rollback: 01:45 already happened at 14:45Z.
        (
            "Australia/Lord_Howe",
            "45 1 * * *",
            "2026-04-04T15:00:00Z",
            Some("Australia/Lord_Howe@2026-04-05T01:45"),
        ),
        // Same wall-clock reading in Berlin, but still on the first pass.
        ("Europe/Berlin", "30 2 * * *", "2026-10-25T00:05:00Z", None),
        // The local 03:00 just beyond the fold has not been reached yet.
        ("Europe/Berlin", "0 3 * * *", "2026-10-25T01:05:00Z", None),
        // The whole grace window is now on the second pass; 02:30 is too old.
        ("Europe/Berlin", "30 2 * * *", "2026-10-25T01:50:00Z", None),
        // Ordinary winter day: recover the 02:30 at 03:05.
        (
            "Europe/Berlin",
            "30 2 * * *",
            "2026-10-26T02:05:00Z",
            Some("Europe/Berlin@2026-10-26T02:30"),
        ),
    ] {
        let now = instant(at);
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(JsonStateStore::new(dir.path()));
        let executor = Arc::new(StubExecutor::default());
        let mut scheduler = Scheduler::new(nightly(zone, cron), store, executor.clone(), now);
        let decisions = scheduler.evaluate(now).await;
        if let Some(key) = key {
            assert_eq!(decisions.len(), 1, "{zone} at {at}: {decisions:?}");
            assert_eq!(decisions[0].key, key);
            assert!(matches!(decisions[0].outcome, Outcome::Fired { .. }));
        } else {
            assert!(decisions.is_empty(), "{zone} at {at}: {decisions:?}");
        }
        assert_eq!(
            executor.runs.load(Ordering::SeqCst),
            usize::from(key.is_some())
        );
    }
}
