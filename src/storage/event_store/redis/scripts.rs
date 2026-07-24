use std::sync::LazyLock;

use redis::Script;

const LEGACY_COMMON: &str = include_str!("../scripts/legacy_common.lua");

fn legacy_script(body: &str) -> Script {
    Script::new(&format!("{LEGACY_COMMON}\n{body}"))
}

pub(super) static PUBLISH: LazyLock<Script> =
    LazyLock::new(|| Script::new(include_str!("../scripts/publish.lua")));
pub(super) static LIST: LazyLock<Script> =
    LazyLock::new(|| Script::new(include_str!("../scripts/list.lua")));
pub(super) static DELETE: LazyLock<Script> =
    LazyLock::new(|| Script::new(include_str!("../scripts/delete.lua")));
pub(super) static LEGACY_STATUS: LazyLock<Script> =
    LazyLock::new(|| legacy_script(include_str!("../scripts/legacy_status.lua")));
pub(super) static LEGACY_FETCH: LazyLock<Script> =
    LazyLock::new(|| legacy_script(include_str!("../scripts/legacy_fetch.lua")));
pub(super) static LEGACY_COMMIT: LazyLock<Script> =
    LazyLock::new(|| legacy_script(include_str!("../scripts/legacy_commit.lua")));
pub(super) static LEGACY_RESET: LazyLock<Script> =
    LazyLock::new(|| legacy_script(include_str!("../scripts/legacy_reset.lua")));
pub(super) static LEGACY_TRANSITION: LazyLock<Script> =
    LazyLock::new(|| legacy_script(include_str!("../scripts/legacy_transition.lua")));
