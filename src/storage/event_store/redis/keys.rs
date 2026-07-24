use redis::{Script, ScriptInvocation};
use uuid::Uuid;

use super::RedisEventStore;
use crate::storage::redis_keys::{is_legacy_safe_run_id, run_segment};

#[derive(Debug, Clone)]
pub(super) struct EventKeyFamily {
    pub(super) list: String,
    pub(super) index: String,
    pub(super) sequence: String,
    pub(super) meta: String,
}

impl EventKeyFamily {
    fn from_base(base: String) -> Self {
        Self {
            list: base.clone(),
            index: format!("{base}:index"),
            sequence: format!("{base}:seq"),
            meta: format!("{base}:meta"),
        }
    }

    fn add_to<'a>(&self, invocation: &mut ScriptInvocation<'a>) {
        invocation
            .key(&self.list)
            .key(&self.index)
            .key(&self.sequence)
            .key(&self.meta);
    }
}

pub(super) struct LegacyMigrationKeys {
    pub(super) current: EventKeyFamily,
    pub(super) raw: EventKeyFamily,
    pub(super) state: String,
    pub(super) frozen: EventKeyFamily,
    probe_from: String,
    probe_to: String,
}

impl LegacyMigrationKeys {
    pub(super) fn prepare<'a>(&self, script: &'a Script) -> ScriptInvocation<'a> {
        let mut invocation = script.prepare_invoke();
        self.current.add_to(&mut invocation);
        self.raw.add_to(&mut invocation);
        invocation.key(&self.state);
        self.frozen.add_to(&mut invocation);
        invocation.key(&self.probe_from).key(&self.probe_to);
        invocation
    }
}

impl RedisEventStore {
    pub(super) fn event_keys(&self, run_id: &str) -> EventKeyFamily {
        EventKeyFamily::from_base(format!("{}events:{}", self.prefix, run_segment(run_id)))
    }

    pub(super) fn deletion_fence_key(&self, run_id: &str) -> String {
        format!("{}event_deletions:v1:{}", self.prefix, run_segment(run_id))
    }

    pub(super) fn legacy_migration_keys(&self, run_id: &str) -> LegacyMigrationKeys {
        let exact_run = hex::encode(run_id.as_bytes());
        let migration_base = format!("{}event_migrations:v1:{exact_run}", self.prefix);
        // The snapshot name is deterministic so a missing/corrupt state hash
        // can never make quarantined events look like an empty stream.
        let frozen_base = format!("{migration_base}:snapshot");
        let probe = Uuid::new_v4().simple();
        LegacyMigrationKeys {
            current: self.event_keys(run_id),
            raw: EventKeyFamily::from_base(format!("{}events:{run_id}", self.prefix)),
            state: migration_base,
            frozen: EventKeyFamily::from_base(frozen_base),
            probe_from: format!("{}event_migration_probes:v1:{probe}:from", self.prefix),
            probe_to: format!("{}event_migration_probes:v1:{probe}:to", self.prefix),
        }
    }

    pub(super) fn uses_encoded_event_keys(run_id: &str) -> bool {
        !is_legacy_safe_run_id(run_id)
    }
}
