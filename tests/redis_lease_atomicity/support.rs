use std::collections::BTreeMap;

use ironflow::engine::types::{Context, RunStatus};
use ironflow::storage::{RunLease, StateStore, redis_store::RedisStateStore};
use redis::aio::ConnectionManager;

use crate::redis_support::RedisTest;

pub const RUN: &str = "lease-run";
pub const OWNER: &str = "lease-owner";
pub type Snapshot = BTreeMap<String, (Vec<u8>, i64)>;

pub const STATUS: &str = include_str!("../../src/storage/redis_store/scripts/lease_status.lua");
pub const RENEW: &str = include_str!("../../src/storage/redis_store/scripts/lease_renew.lua");
pub const DELETE: &str = include_str!("../../src/storage/redis_store/scripts/delete.lua");
pub const SWEEP: &str = include_str!("../../src/storage/redis_store/scripts/sweep.lua");

pub struct Fixture {
    pub redis: RedisTest,
    pub store: RedisStateStore,
    pub conn: ConnectionManager,
    pub keys: Vec<String>,
}

impl Fixture {
    pub async fn new(ttl: Option<u64>) -> Option<Self> {
        let redis = RedisTest::connect("lease_atomicity").await?;
        let store = redis.state_store(ttl).await;
        for id in [RUN, "neighbor"] {
            store
                .init_run_owned(
                    id,
                    "flow",
                    &Context::new(),
                    &RunLease::renewed(OWNER.into()),
                )
                .await
                .unwrap();
            store
                .set_run_status_owned(id, RunStatus::Running, OWNER)
                .await
                .unwrap();
        }
        let keys = [
            "runs:lease-run",
            "runs:index",
            "run_catalog:v1:members",
            "run_catalog:v1:all",
            "run_catalog:v1:status:pending",
            "run_catalog:v1:status:running",
            "run_catalog:v1:status:success",
            "run_catalog:v1:status:failed",
            "run_catalog:v1:status:stalled",
            "run_catalog:v1:status:cancelled",
            "run_leases:v1:expiry",
        ]
        .map(|suffix| format!("{}{suffix}", redis.prefix))
        .to_vec();
        let conn = redis.connection().await.unwrap();
        Some(Self {
            redis,
            store,
            conn,
            keys,
        })
    }

    pub async fn fault(&mut self, index: usize) {
        let _: () = redis::cmd("SET")
            .arg(&self.keys[index])
            .arg("wrong-type")
            .arg("EX")
            .arg(600)
            .query_async(&mut self.conn)
            .await
            .unwrap();
    }

    pub async fn expire_lease(&mut self) {
        let _: () = redis::pipe()
            .cmd("HSET")
            .arg(&self.keys[0])
            .arg("lease_expires_micros")
            .arg(0)
            .cmd("ZADD")
            .arg(&self.keys[10])
            .arg(0)
            .arg(RUN)
            .query_async(&mut self.conn)
            .await
            .unwrap();
    }

    pub async fn snapshot(&mut self) -> Snapshot {
        let mut snapshot = Snapshot::new();
        let mut cursor = 0_u64;
        loop {
            let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(format!("{}*", self.redis.prefix))
                .arg("COUNT")
                .arg(100)
                .query_async(&mut self.conn)
                .await
                .unwrap();
            for key in keys {
                // Absolute expiry avoids clock-tolerance assertions on a decreasing TTL.
                let value: (Vec<u8>, i64) = redis::pipe()
                    .cmd("DUMP")
                    .arg(&key)
                    .cmd("PEXPIRETIME")
                    .arg(&key)
                    .query_async(&mut self.conn)
                    .await
                    .unwrap();
                snapshot.insert(key, value);
            }
            cursor = next;
            if cursor == 0 {
                return snapshot;
            }
        }
    }

    pub async fn assert_unchanged(&mut self, before: Snapshot, label: &str) {
        let after = self.snapshot().await;
        self.redis.cleanup().await;
        let changed: Vec<_> = before
            .keys()
            .chain(after.keys())
            .filter(|key| before.get(*key) != after.get(*key))
            .collect();
        assert!(changed.is_empty(), "{label} changed keys: {changed:?}");
    }

    pub async fn invocation(&mut self, script: &str) -> (Vec<String>, Vec<String>) {
        if script == DELETE || script == SWEEP {
            return (self.keys.clone(), vec![RUN.into()]);
        }
        if script == RENEW {
            return (
                vec![self.keys[0].clone(), self.keys[10].clone()],
                [OWNER, RUN, "90000000", "60", "90000000"]
                    .map(String::from)
                    .to_vec(),
            );
        }
        let fields: BTreeMap<String, String> = redis::cmd("HGETALL")
            .arg(&self.keys[0])
            .query_async(&mut self.conn)
            .await
            .unwrap();
        let member: String = redis::cmd("HGET")
            .arg(&self.keys[2])
            .arg(RUN)
            .query_async(&mut self.conn)
            .await
            .unwrap();
        let keys = std::iter::once(self.keys[0].clone())
            .chain(self.keys[2..].iter().cloned())
            .collect();
        (
            keys,
            vec![
                "__ironflow_legacy_revision__".into(),
                fields["revision"].clone(),
                fields["incarnation"].clone(),
                fields["info"].clone(),
                fields["summary"].clone(),
                "60".into(),
                "changed-revision".into(),
                RUN.into(),
                member,
                "running".into(),
                "owner".into(),
                OWNER.into(),
                "0".into(),
                "90000000".into(),
            ],
        )
    }

    pub async fn invoke(
        &mut self,
        script: &str,
        keys: &[String],
        args: &[String],
    ) -> redis::RedisResult<i64> {
        let script = redis::Script::new(script);
        let mut call = script.prepare_invoke();
        for key in keys {
            call.key(key);
        }
        for arg in args {
            call.arg(arg);
        }
        call.invoke_async(&mut self.conn).await
    }
}
