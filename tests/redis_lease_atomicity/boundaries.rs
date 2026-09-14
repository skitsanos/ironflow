use ironflow::engine::types::RunStatus;
use ironflow::storage::{RunLease, StateStore, StorageErrorKind};

use super::support::{Fixture, OWNER, RUN};

#[tokio::test]
async fn corrupt_primary_types_and_lease_deadlines_are_rejected_without_mutation() {
    for operation in ["status", "renew", "delete"] {
        for fault in [
            "wrong-type",
            "not-a-number",
            "9999999999999999999999999999",
            "-9999999999999999999999999999",
        ] {
            let Some(mut fixture) = Fixture::new(Some(60)).await else {
                return;
            };
            if fault == "wrong-type" {
                fixture.fault(0).await;
            } else {
                let _: () = redis::cmd("HSET")
                    .arg(&fixture.keys[0])
                    .arg("lease_expires_micros")
                    .arg(fault)
                    .query_async(&mut fixture.conn)
                    .await
                    .unwrap();
            }
            let before = fixture.snapshot().await;
            let result = match operation {
                "status" => fixture
                    .store
                    .set_run_status_owned(RUN, RunStatus::Success, OWNER)
                    .await
                    .map(|_| ()),
                "renew" => fixture
                    .store
                    .renew_run_lease(RUN, &RunLease::renewed(OWNER.into()))
                    .await
                    .map(|_| ()),
                _ => fixture.store.delete_run(RUN).await,
            };
            fixture
                .assert_unchanged(before, &format!("{operation}: {fault}"))
                .await;
            assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Corruption);
        }
    }
}

#[tokio::test]
async fn lease_ttl_boundaries_renew_release_and_delete_successfully() {
    for ttl in [None, Some(1), Some(99_999_999_999)] {
        let Some(mut fixture) = Fixture::new(ttl).await else {
            return;
        };
        assert!(
            fixture
                .store
                .renew_run_lease(RUN, &RunLease::renewed(OWNER.into()))
                .await
                .unwrap()
        );
        let active_ttl: i64 = redis::cmd("TTL")
            .arg(&fixture.keys[0])
            .query_async(&mut fixture.conn)
            .await
            .unwrap();
        if let Some(ttl) = ttl {
            assert!(
                active_ttl >= ttl.max(179) as i64 - 1,
                "active TTL {active_ttl}"
            );
        } else {
            assert_eq!(active_ttl, -1);
        }
        assert!(
            fixture
                .store
                .set_run_status_owned(RUN, RunStatus::Success, OWNER)
                .await
                .unwrap()
        );
        let (retained_ttl, owner, score): (i64, Option<String>, Option<f64>) = redis::pipe()
            .cmd("TTL")
            .arg(&fixture.keys[0])
            .cmd("HGET")
            .arg(&fixture.keys[0])
            .arg("lease_owner")
            .cmd("ZSCORE")
            .arg(&fixture.keys[10])
            .arg(RUN)
            .query_async(&mut fixture.conn)
            .await
            .unwrap();
        assert!(owner.is_none() && score.is_none());
        if let Some(ttl) = ttl {
            assert!((ttl as i64 - 1..=ttl as i64).contains(&retained_ttl));
        } else {
            assert_eq!(retained_ttl, -1);
        }
        fixture.store.delete_run(RUN).await.unwrap();
        let result = fixture.store.get_run_info(RUN).await;
        fixture.redis.cleanup().await;
        assert_eq!(result.unwrap_err().kind(), StorageErrorKind::NotFound);
    }
}
