use super::support::{DELETE, Fixture, RENEW, RUN, STATUS, SWEEP};

#[tokio::test]
async fn invalid_numeric_arguments_fail_before_any_write() {
    for script in [STATUS, RENEW] {
        let positions = if script == STATUS {
            vec![5, 13]
        } else {
            vec![2, 3, 4]
        };
        for position in positions {
            for invalid in [
                "-2",
                "1.5",
                "1e20",
                "99999999999999999999999999999999",
                &"9".repeat(400),
            ] {
                let Some(mut fixture) = Fixture::new(Some(60)).await else {
                    return;
                };
                let (keys, mut args) = fixture.invocation(script).await;
                args[position] = invalid.into();
                let before = fixture.snapshot().await;
                let result = fixture.invoke(script, &keys, &args).await;
                fixture
                    .assert_unchanged(before, &format!("invalid numeric argument {position}"))
                    .await;
                assert!(result.unwrap_err().to_string().contains("IRONFLOW_INVALID"));
            }
        }
    }
}

#[tokio::test]
async fn retention_limits_and_computed_lease_overflow_are_rejected() {
    for (script, position, value, release) in [
        (STATUS, 5, "100000000000", "0"),
        (STATUS, 5, "100000000000", "1"),
        (STATUS, 13, "9007199254740991", "0"),
        (RENEW, 3, "100000000000", "0"),
        (RENEW, 2, "9007199254740991", "0"),
        (RENEW, 4, "9007199254740991", "0"),
    ] {
        let Some(mut fixture) = Fixture::new(Some(60)).await else {
            return;
        };
        let (keys, mut args) = fixture.invocation(script).await;
        args[position] = value.into();
        if script == STATUS {
            args[12] = release.into();
        }
        let before = fixture.snapshot().await;
        let result = fixture.invoke(script, &keys, &args).await;
        fixture.assert_unchanged(before, "numeric boundary").await;
        assert!(result.unwrap_err().to_string().contains("IRONFLOW_INVALID"));
    }
}

#[tokio::test]
async fn arity_and_key_aliases_are_rejected_before_any_write() {
    for script in [STATUS, RENEW, DELETE, SWEEP] {
        for fault in [
            "missing-key",
            "extra-key",
            "missing-arg",
            "extra-arg",
            "alias",
            "same-type-alias",
        ] {
            let Some(mut fixture) = Fixture::new(Some(60)).await else {
                return;
            };
            let (mut keys, mut args) = fixture.invocation(script).await;
            // Deletion must reach its write path if argument validation is absent.
            if script == DELETE {
                fixture.expire_lease().await;
            } else if script == SWEEP {
                let _: () = redis::cmd("DEL")
                    .arg(&fixture.keys[0])
                    .query_async(&mut fixture.conn)
                    .await
                    .unwrap();
            }
            match fault {
                "missing-key" => {
                    keys.remove(0);
                }
                "extra-key" => keys.push(format!("{}extra", fixture.redis.prefix)),
                "missing-arg" => {
                    args.pop();
                }
                "extra-arg" => args.push("extra".into()),
                "alias" => keys[1] = keys[0].clone(),
                _ => {
                    let last = keys.len() - 1;
                    if script == RENEW {
                        keys[1] = keys[0].clone();
                    } else {
                        keys[last] = keys[if script == STATUS { 2 } else { 3 }].clone();
                    }
                }
            }
            let before = fixture.snapshot().await;
            let result = fixture.invoke(script, &keys, &args).await;
            fixture.assert_unchanged(before, fault).await;
            assert!(result.unwrap_err().to_string().contains("IRONFLOW_"));
        }
    }
}

#[tokio::test]
async fn stale_revision_incarnation_and_owner_guards_do_not_mutate() {
    for (position, value, expected) in [
        (1, "stale-revision", 0),
        (2, "stale-incarnation", -1),
        (11, "other-owner", -2),
        (10, "expired", -2),
    ] {
        let Some(mut fixture) = Fixture::new(Some(60)).await else {
            return;
        };
        let (keys, mut args) = fixture.invocation(STATUS).await;
        args[position] = value.into();
        let before = fixture.snapshot().await;
        let result = fixture.invoke(STATUS, &keys, &args).await;
        fixture.assert_unchanged(before, "stale lease guard").await;
        assert_eq!(result.unwrap(), expected);
    }
}

#[tokio::test]
async fn status_preflight_precedes_missing_and_ownerless_cleanup() {
    for missing in [false, true] {
        for fault in [
            "catalog",
            "lease-index",
            "guard",
            "status",
            "release",
            "ttl",
        ] {
            let Some(mut fixture) = Fixture::new(Some(60)).await else {
                return;
            };
            let (keys, mut args) = fixture.invocation(STATUS).await;
            if missing {
                let _: () = redis::cmd("DEL")
                    .arg(&fixture.keys[0])
                    .query_async(&mut fixture.conn)
                    .await
                    .unwrap();
            } else {
                let _: () = redis::cmd("HDEL")
                    .arg(&fixture.keys[0])
                    .arg("lease_owner")
                    .arg("lease_expires_micros")
                    .query_async(&mut fixture.conn)
                    .await
                    .unwrap();
            }
            match fault {
                "catalog" => fixture.fault(2).await,
                "lease-index" => fixture.fault(10).await,
                "guard" => args[10] = "invalid".into(),
                "status" => args[9] = "invalid".into(),
                "release" => args[12] = "invalid".into(),
                _ => args[5] = "100000000000".into(),
            }
            let before = fixture.snapshot().await;
            let result = fixture.invoke(STATUS, &keys, &args).await;
            fixture
                .assert_unchanged(before, &format!("cleanup missing={missing} fault={fault}"))
                .await;
            assert!(result.unwrap_err().to_string().contains("IRONFLOW_"));
        }
    }
}

#[tokio::test]
async fn valid_stale_status_cleanup_still_removes_the_lease_index_member() {
    for missing in [false, true] {
        let Some(mut fixture) = Fixture::new(Some(60)).await else {
            return;
        };
        let (keys, args) = fixture.invocation(STATUS).await;
        if missing {
            let _: () = redis::cmd("DEL")
                .arg(&fixture.keys[0])
                .query_async(&mut fixture.conn)
                .await
                .unwrap();
        } else {
            let _: () = redis::cmd("HDEL")
                .arg(&fixture.keys[0])
                .arg("lease_owner")
                .arg("lease_expires_micros")
                .query_async(&mut fixture.conn)
                .await
                .unwrap();
        }
        assert_eq!(
            fixture.invoke(STATUS, &keys, &args).await.unwrap(),
            if missing { -3 } else { -2 }
        );
        let score: Option<f64> = redis::cmd("ZSCORE")
            .arg(&fixture.keys[10])
            .arg(RUN)
            .query_async(&mut fixture.conn)
            .await
            .unwrap();
        fixture.redis.cleanup().await;
        assert!(score.is_none());
    }
}
